//! The System One service — optional decision assistance over a query.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::client::{HttpSystemOneModel, SystemOneModel};
use super::config::SystemOneConfig;
use super::protocol::{Answer, Question, SystemOneRequest};
use crate::query::{QueryEngine, QueryIntent, QueryPlan, QueryResult, ScoredNode};

/// Per-role outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleStatus {
    /// Role not enabled / not assigned.
    Disabled,
    /// Model answered and influenced advice/order.
    Used,
    /// Model configured but failed -> deterministic baseline used.
    Fallback,
    /// Deterministically skipped (e.g. `not_needed`, `not_needed_exact_match`).
    Other(&'static str),
}

impl RoleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Used => "used",
            Self::Fallback => "fallback",
            Self::Other(s) => s,
        }
    }
}

/// Compact execution metadata (§30/§39/§44). Never contains secrets.
#[derive(Debug, Clone)]
pub struct Provenance {
    pub query: RoleStatus,
    pub query_model: Option<String>,
    pub rerank: RoleStatus,
    pub rerank_model: Option<String>,
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            query: RoleStatus::Disabled,
            query_model: None,
            rerank: RoleStatus::Disabled,
            rerank_model: None,
        }
    }
}

/// The optional System One service.
pub struct SystemOne {
    cfg: Option<SystemOneConfig>,
    /// Rerank batch size override (default `RERANK_MAX_PER_REQUEST`); used by
    /// the controlled batching experiment.
    batch: usize,
}

/// Max logical results sent to the rerank model in one request (§34).
/// V2: up to 20 logical results in one request when it fits the 32k-context
/// soft budget (28k); see `RERANK_MAX_PER_REQUEST`.
pub const RERANK_BATCH: usize = 16;

/// Max logical results in one rerank request (§22). Soft-bound by context.
pub const RERANK_MAX_PER_REQUEST: usize = 20;

/// Bounded seed candidates for query-role plan generation (§12).
pub const SEED_BOUND: usize = 5;

/// Optional rerank batch-size override via `REPODEX_SO_BATCH` (experiment only).
fn batch_env() -> Option<usize> {
    std::env::var("REPODEX_SO_BATCH")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n >= 1)
}

/// The result of the query-role bounded advice.
pub enum QueryAdviceOutcome {
    /// Model chose a valid bounded plan.
    Chosen { plan: QueryPlan, confidence: f64 },
    /// No meaningful decision (0 or 1 valid plan) — deterministic baseline.
    NotNeeded,
    /// Model failed/invalid — deterministic baseline.
    Fallback,
}

impl SystemOne {
    /// Build from the loaded `[system_one]` table (None/absent => disabled).
    pub fn new(cfg: Option<SystemOneConfig>) -> Self {
        Self {
            cfg,
            batch: batch_env().unwrap_or(RERANK_MAX_PER_REQUEST),
        }
    }

    /// Override the rerank batch size (batching experiment §23).
    pub fn with_batch(cfg: Option<SystemOneConfig>, batch: usize) -> Self {
        Self { cfg, batch }
    }

    fn batch_size(&self) -> usize {
        self.batch.max(1)
    }

    pub fn disabled() -> Self {
        Self {
            cfg: None,
            batch: RERANK_MAX_PER_REQUEST,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.cfg.as_ref().map(|c| c.enabled).unwrap_or(false)
    }

    fn model(&self, role: &str) -> Option<(String, HttpSystemOneModel)> {
        let cfg = self.cfg.as_ref()?;
        let (name, mc) = cfg.model_for(role)?;
        HttpSystemOneModel::new(name.clone(), mc)
            .ok()
            .map(|m| (name.clone(), m))
    }

    /// Orchestrate a query: optional query-role advice, deterministic run,
    /// optional rerank. Always returns a valid result (§40/§41).
    pub fn run(
        &self,
        engine: &QueryEngine,
        plan: &QueryPlan,
        explicit_intent: bool,
    ) -> (QueryResult, Provenance) {
        let mut prov = Provenance::default();
        // ---- query role: seed-first bounded plan advice on default intent ----
        let mut plan = plan.clone();
        let mut query_status = RoleStatus::Disabled;
        if !explicit_intent {
            if let Some((name, model)) = self.model("query") {
                prov.query_model = Some(name.clone());
                match self.query_advice(engine, &model, &plan) {
                    QueryAdviceOutcome::Chosen { plan: p, .. } => {
                        plan = p;
                        query_status = RoleStatus::Used;
                    }
                    QueryAdviceOutcome::NotNeeded => {
                        query_status = RoleStatus::Other("not_needed");
                    }
                    QueryAdviceOutcome::Fallback => {
                        query_status = RoleStatus::Fallback;
                    }
                }
            }
        }
        prov.query = query_status;
        // ---- deterministic core (unchanged) ----
        let mut result = engine.run(&plan);
        // ---- rerank role: reorder seeds only, behind the eligibility guard ----
        let rerank_skip = rerank_skip_reason(&plan, &result.seeds);
        if let Some((name, model)) = self.model("rerank") {
            prov.rerank_model = Some(name.clone());
            match rerank_skip {
                Some(reason) => prov.rerank = RoleStatus::Other(reason),
                None => match self.rerank(&model, &plan, &result.seeds) {
                    Some(order) => {
                        apply_order(&mut result.seeds, order);
                        prov.rerank = RoleStatus::Used;
                    }
                    None => prov.rerank = RoleStatus::Fallback,
                },
            }
        }
        // Attach compact provenance (never evidence certainty).
        if prov.query != RoleStatus::Disabled {
            result.so_query = Some(prov.query.as_str().to_string());
        }
        if prov.rerank != RoleStatus::Disabled {
            result.so_rerank = Some(prov.rerank.as_str().to_string());
        }
        (result, prov)
    }

    /// Timed + traced `decide` (§6 observability). Records one `CallRecord`
    /// per attempt when `REPODEX_SO_TRACE` is set; never alters the result.
    fn call(
        &self,
        role: &str,
        batch: usize,
        model: &HttpSystemOneModel,
        req: &SystemOneRequest,
        detail: Option<Value>,
    ) -> Result<crate::system_one::protocol::SystemOneResponse, crate::system_one::SystemOneError>
    {
        let started = std::time::Instant::now();
        let out = model.decide(req);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        if super::trace::trace_path().is_some() {
            let (resolved, input_tok, output_tok) = match &out {
                Ok(r) => super::trace::usage_from_extra(&r.extra),
                Err(_) => (None, None, None),
            };
            let detail = match (detail, &out) {
                (Some(d), Ok(r)) => Some(json!({"options": d, "answers": r.answers})),
                (d, _) => d,
            };
            super::trace::record(&super::trace::CallRecord {
                role: role.to_string(),
                batch,
                configured_model: model.name().to_string(),
                resolved_model: resolved,
                wall_latency_ms: ms,
                input_tokens: input_tok,
                output_tokens: output_tok,
                question_count: req.questions.len(),
                state_bytes: req.state.to_string().len(),
                status: match &out {
                    Ok(_) => "ok".to_string(),
                    Err(e) => format!("{e:?}"),
                },
                fallback: out.is_err(),
                fallback_reason: out.as_ref().err().map(|e| e.to_string()),
                detail,
            });
        }
        out
    }

    /// Seed-first bounded plan alternatives (§11-§17). Deterministic seed
    /// discovery -> bounded target-bound plans -> Jev `choice` among them.
    /// The model picks one opaque option id; RepoDex maps it back to a real,
    /// executable plan. Returns the chosen plan (or None => use baseline).
    fn query_advice(
        &self,
        engine: &QueryEngine,
        model: &HttpSystemOneModel,
        plan: &QueryPlan,
    ) -> QueryAdviceOutcome {
        // Deterministic seed discovery (≤5 real identities).
        let seeds = engine.top_seeds(plan, SEED_BOUND);
        // Build bounded plan alternatives: opaque id -> (intent, target).
        let mut options: BTreeMap<String, String> = BTreeMap::new();
        let mut plans: BTreeMap<String, QueryPlan> = BTreeMap::new();
        let add = |options: &mut BTreeMap<String, String>,
                   plans: &mut BTreeMap<String, QueryPlan>,
                   intent: QueryIntent,
                   target: Option<String>,
                   desc: String| {
            let id = format!("p{}", plans.len());
            let mut p = plan.clone();
            p.intent = intent;
            p.target = target;
            plans.insert(id.clone(), p);
            options.insert(id, desc);
        };
        add(
            &mut options,
            &mut plans,
            QueryIntent::Find,
            None,
            "generic lookup".into(),
        );
        for s in &seeds {
            let label = &s.node.label;
            let path = &s.node.path;
            // `related` is valid for any seed node.
            add(
                &mut options,
                &mut plans,
                QueryIntent::Related,
                Some(label.clone()),
                format!("related neighborhood of {label} ({path})"),
            );
            // callers/callees only make sense for declaration targets.
            if s.node.kind == crate::graph::NodeKind::Declaration {
                add(
                    &mut options,
                    &mut plans,
                    QueryIntent::Callers,
                    Some(label.clone()),
                    format!("callers of {label} ({path})"),
                );
                add(
                    &mut options,
                    &mut plans,
                    QueryIntent::Callees,
                    Some(label.clone()),
                    format!("callees of {label} ({path})"),
                );
            }
        }
        // §15: no meaningful decision => no model call.
        if plans.len() <= 1 {
            return QueryAdviceOutcome::NotNeeded;
        }
        let mut req = SystemOneRequest::new(
            model.model_id().to_string(),
            json!({ "raw": plan.raw, "terms": plan.terms, "mode": plan.mode.as_str() }),
        );
        req.questions.insert(
            "plan".into(),
            Question::choice(
                "Pick the single deterministic lookup plan that best matches the user query intent.",
                options.clone(),
            ),
        );
        let detail = Some(serde_json::to_value(&options).unwrap_or_default());
        let Ok(resp) = self.call("query", 0, model, &req, detail) else {
            return QueryAdviceOutcome::Fallback;
        };
        let Some(Answer::Choice(c)) = resp.answers.get("plan") else {
            return QueryAdviceOutcome::Fallback;
        };
        // Validate the opaque id maps to a plan we built; never invent one.
        match plans.get(&c.choice) {
            Some(p) => QueryAdviceOutcome::Chosen {
                plan: p.clone(),
                confidence: c.confidence,
            },
            None => QueryAdviceOutcome::Fallback,
        }
    }

    /// Rerank role: score each logical result, reorder only (§31-§38).
    /// Transaction-like: all batches must answer validly or the whole rerank
    /// falls back to the deterministic order (§37). Returns a permutation.
    fn rerank(
        &self,
        model: &HttpSystemOneModel,
        plan: &QueryPlan,
        seeds: &[ScoredNode],
    ) -> Option<Vec<usize>> {
        if seeds.len() <= 1 {
            return Some((0..seeds.len()).collect());
        }
        let scale = vec![
            "irrelevant".to_string(),
            "weak".to_string(),
            "relevant".to_string(),
            "highly relevant".to_string(),
        ];
        let bsz = self.batch_size();
        let mut scored: Vec<(usize, f64)> = Vec::with_capacity(seeds.len());
        for chunk in seeds.chunks(bsz).enumerate() {
            let (bi, batch) = chunk;
            let mut req = SystemOneRequest::new(
                model.model_id().to_string(),
                json!({ "raw": plan.raw, "terms": plan.terms }),
            );
            for (j, s) in batch.iter().enumerate() {
                req.questions.insert(
                    format!("r{}", bi * bsz + j),
                    Question::score(
                        format!(
                            "Relevance of {} `{}` to the query",
                            s.node.kind.as_str(),
                            s.node.label
                        ),
                        scale.clone(),
                    ),
                );
            }
            let resp = self.call("rerank", bi, model, &req, None).ok()?; // any batch failure -> whole fallback
            for (j, s) in batch.iter().enumerate() {
                let id = format!("r{}", bi * bsz + j);
                let Some(Answer::Score(a)) = resp.answers.get(&id) else {
                    return None; // missing/invalid answer -> fallback
                };
                if !a.score.is_finite() {
                    return None;
                }
                let _ = s;
                scored.push((bi * bsz + j, a.score));
            }
        }
        // Reorder by model score desc; equal scores keep deterministic order.
        let mut order: Vec<usize> = (0..seeds.len()).collect();
        let score_of: BTreeMap<usize, f64> = scored.into_iter().collect();
        order.sort_by(|a, b| {
            let sa = score_of.get(a).copied().unwrap_or(0.0);
            let sb = score_of.get(b).copied().unwrap_or(0.0);
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b)) // deterministic tie-break
        });
        Some(order)
    }
}

/// Selective rerank eligibility guard (§18-§20). Returns Some(reason) to skip.
/// Conservative: skip empty/single results, exhaustive (order-only, no value
/// for membership), and a single exact-identifier match already at rank #1.
fn rerank_skip_reason(plan: &QueryPlan, seeds: &[ScoredNode]) -> Option<&'static str> {
    if seeds.is_empty() {
        return Some("not_needed_zero_results");
    }
    if seeds.len() == 1 {
        return Some("not_needed_single_result");
    }
    if plan.mode == crate::query::QueryMode::Exhaustive {
        return Some("not_needed_exhaustive");
    }
    // Exact-symbol guard (§19): a lone exact-name hit already at rank #1 is the
    // answer — reranking can only demote it. `is_identifier` = single token.
    let single_ident = plan.raw.split_whitespace().count() == 1
        && plan
            .raw
            .trim()
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_');
    if single_ident {
        let raw = plan.raw.trim();
        let exact: Vec<&ScoredNode> = seeds.iter().filter(|s| s.node.label == raw).collect();
        if exact.len() == 1 && seeds[0].node.label == raw {
            return Some("not_needed_exact_match");
        }
    }
    None
}

/// Apply a permutation to seeds (reorder only; the set is unchanged).
fn apply_order(seeds: &mut Vec<ScoredNode>, order: Vec<usize>) {
    let orig: Vec<ScoredNode> = std::mem::take(seeds);
    for i in order {
        if let Some(s) = orig.get(i) {
            seeds.push(s.clone());
        }
    }
}
