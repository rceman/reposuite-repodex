//! The System One service — optional decision assistance over a query.

use std::collections::BTreeMap;

use serde_json::json;

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
}

impl RoleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Used => "used",
            Self::Fallback => "fallback",
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
}

/// Max logical results sent to the rerank model in one request (§34).
pub const RERANK_BATCH: usize = 16;

impl SystemOne {
    /// Build from the loaded `[system_one]` table (None/absent => disabled).
    pub fn new(cfg: Option<SystemOneConfig>) -> Self {
        Self { cfg }
    }

    pub fn disabled() -> Self {
        Self { cfg: None }
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
        // ---- query role: bounded advice on a *default* intent only ----
        let mut plan = plan.clone();
        if !explicit_intent {
            if let Some((name, model)) = self.model("query") {
                prov.query_model = Some(name.clone());
                match self.query_advice(&model, &plan) {
                    Some(intent) => {
                        plan.intent = intent;
                        prov.query = RoleStatus::Used;
                    }
                    None => prov.query = RoleStatus::Fallback,
                }
            }
        }
        // ---- deterministic core (unchanged) ----
        let mut result = engine.run(&plan);
        // ---- rerank role: reorder seeds only ----
        if let Some((name, model)) = self.model("rerank") {
            prov.rerank_model = Some(name.clone());
            match self.rerank(&model, &plan, &result.seeds) {
                Some(order) => {
                    apply_order(&mut result.seeds, order);
                    prov.rerank = RoleStatus::Used;
                }
                None => prov.rerank = RoleStatus::Fallback,
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
            });
        }
        out
    }

    /// Query role: offer bounded intent alternatives, let the model `choice`.
    /// Only intents valid for the plan are offered (§26). The model never
    /// generates a plan — it picks one of RepoDex's own options.
    fn query_advice(&self, model: &HttpSystemOneModel, plan: &QueryPlan) -> Option<QueryIntent> {
        // Build the bounded option set from already-supported intents.
        let mut options: BTreeMap<String, String> = BTreeMap::new();
        options.insert("find".into(), "generic symbol/file lookup".into());
        options.insert("related".into(), "bounded neighborhood of matches".into());
        if plan.target.is_some() {
            options.insert(
                "callers".into(),
                "incoming candidate callers of the target".into(),
            );
            options.insert(
                "callees".into(),
                "outgoing candidate callees from the target".into(),
            );
        }
        let mut req = SystemOneRequest::new(
            model.name().to_string(),
            json!({
                "raw": plan.raw,
                "terms": plan.terms,
                "mode": plan.mode.as_str(),
            }),
        );
        req.questions.insert(
            "intent".into(),
            Question::choice(
                "Which deterministic lookup best fits the user query?",
                options.clone(),
            ),
        );
        let resp = self.call("query", 0, model, &req).ok()?;
        let ans = resp.answers.get("intent")?;
        let Answer::Choice(c) = ans else {
            return None;
        };
        // Validate: the chosen option must be one we offered (§29).
        let chosen = options.get(&c.choice)?;
        let _ = chosen;
        Some(match c.choice.as_str() {
            "related" => QueryIntent::Related,
            "callers" => QueryIntent::Callers,
            "callees" => QueryIntent::Callees,
            _ => QueryIntent::Find,
        })
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
        let mut scored: Vec<(usize, f64)> = Vec::with_capacity(seeds.len());
        for chunk in seeds.chunks(RERANK_BATCH).enumerate() {
            let (bi, batch) = chunk;
            let mut req = SystemOneRequest::new(
                model.name().to_string(),
                json!({ "raw": plan.raw, "terms": plan.terms }),
            );
            for (j, s) in batch.iter().enumerate() {
                req.questions.insert(
                    format!("r{}", bi * RERANK_BATCH + j),
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
            let resp = self.call("rerank", bi, model, &req).ok()?; // any batch failure -> whole fallback
            for (j, s) in batch.iter().enumerate() {
                let id = format!("r{}", bi * RERANK_BATCH + j);
                let Some(Answer::Score(a)) = resp.answers.get(&id) else {
                    return None; // missing/invalid answer -> fallback
                };
                if !a.score.is_finite() {
                    return None;
                }
                let _ = s;
                scored.push((bi * RERANK_BATCH + j, a.score));
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

/// Apply a permutation to seeds (reorder only; the set is unchanged).
fn apply_order(seeds: &mut Vec<ScoredNode>, order: Vec<usize>) {
    let orig: Vec<ScoredNode> = std::mem::take(seeds);
    for i in order {
        if let Some(s) = orig.get(i) {
            seeds.push(s.clone());
        }
    }
}
