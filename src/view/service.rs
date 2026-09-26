//! Resolve a locator -> ensure view index -> run query (shared by the human
//! `query` flag mode and the machine `--json` stdin mode).

use std::path::Path;
use std::time::Instant;

use crate::parser::{Analyzer, AnalyzerConfig};
use crate::query::{QueryIntent, QueryMode, QueryPlan, QueryResult};

use super::index::ensure_index;
use super::model::{err, QueryRequest, RepositoryView, ViewError, ViewLocator, ViewResult};
use super::resolve::resolve;

/// Outcome of a view-resolved query: the engine result plus the resolved view
/// metadata and the index-ensure stats.
pub struct ViewQueryOutcome {
    pub view: RepositoryView,
    pub result: QueryResult,
    pub ensure: super::index::EnsureOutcome,
    /// Bounded memory composition for this query (additive guidance only).
    pub memory: crate::memory::compose::MemoryComposition,
    /// Deterministic query shape (for adaptive context compilation).
    pub shape: crate::context::QueryShape,
    /// The effective compiler policy for this query (static|adaptive).
    pub context_policy: crate::context::ContextPolicy,
    /// Recipe execution outcome (Some when a recipe matched + ran).
    pub recipe: Option<crate::recipe::RecipeOutcome>,
    /// Utility-policy trace (shadow/apply) — debug, not agent-facing.
    pub utility_trace: Option<crate::utility::UtilityTrace>,
    /// The logged delivery decision (eligible slate + selected).
    pub delivery: Option<crate::utility::DeliveryDecision>,
    /// Absolute view root (for current-source witness materialization).
    pub view_root: std::path::PathBuf,
    /// Effective source-witness policy for this query.
    pub source_witness: crate::witness::SourceWitnessPolicy,
    /// Milliseconds for the whole resolve+ensure+query.
    pub total_ms: f64,
}

/// Query parameters for a view-resolved query (kept together to stay under the
/// argument-count lint).
#[derive(Debug, Clone)]
pub struct ViewQueryParams<'a> {
    pub query_text: &'a str,
    pub mode: QueryMode,
    pub intent: Option<QueryIntent>,
    /// `callers`/`callees`/`paths` first endpoint (target symbol/name/key).
    pub target: Option<String>,
    /// `paths` second endpoint — bounded two-endpoint connector query (§23).
    pub to: Option<String>,
    pub max_results: usize,
    pub token_budget: Option<usize>,
    pub depth: usize,
    /// Memory policy (§33): off | file | symbol. Additive guidance only.
    pub memory_mode: crate::memory::compose::MemoryMode,
    /// Context-compiler policy (§37): static | adaptive.
    pub context_policy: crate::context::ContextPolicy,
    /// Recipe policy (§41): off | auto | force.
    pub recipes: crate::recipe::RecipePolicy,
    /// Utility policy (§24): off (default) | shadow | apply.
    pub utility_policy: crate::utility::UtilityPolicy,
    /// Source-witness policy (§16): off (default) | bounded.
    pub source_witness: crate::witness::SourceWitnessPolicy,
    /// Deterministic vocabulary bridge: expand query terms with morphological
    /// variants (encoder->encode). off (default) | on.
    pub vocab_bridge: bool,
    /// Repository-native vocabulary bridge: expand domain role terms to the
    /// concrete names the manifests declare. off (default) | on.
    pub vocab_native: bool,
    pub state_override: Option<&'a Path>,
}

/// Resolve `locator`, ensure the view's derived index, and run `query`.
/// `query_text` may come from a flag or a machine request.
pub fn run_view_query(
    locator: &ViewLocator,
    params: &ViewQueryParams<'_>,
) -> ViewResult<ViewQueryOutcome> {
    let started = Instant::now();
    let mut view = resolve(locator)?;

    let analyzer = Analyzer::new(AnalyzerConfig::default())
        .map_err(|e| (ViewError::QueryFailed, format!("analyzer: {e}")))?;
    let ensure = ensure_index(&analyzer, &mut view, params.state_override)
        .map_err(|e| (ViewError::QueryFailed, e))?;

    let index = crate::graph::GraphIndex::load(&ensure.graph_dir)
        .map_err(|e| (ViewError::QueryFailed, e.to_string()))?;
    let engine = crate::query::QueryEngine::new(&index);
    let mut plan = QueryPlan::parse(
        params.query_text,
        params.mode,
        params.intent,
        params.target.clone(),
        params.to.clone(),
        params.max_results,
        params.token_budget,
    );
    plan.max_depth = params.depth;
    if params.vocab_bridge {
        let mut ex = plan.terms.clone();
        for t in plan.terms.clone() {
            for v in crate::query::normalize::morph_variants(&t) {
                if v != t && !ex.contains(&v) {
                    ex.push(v);
                }
            }
        }
        plan.terms = ex;
    }
    if params.vocab_native {
        let manifest_terms: Vec<String> = engine
            .index()
            .nodes()
            .iter()
            .filter(|n| n.language == "manifest")
            .flat_map(|n| crate::query::normalize::identifier_terms(&n.label))
            .collect();
        let mut ex = plan.terms.clone();
        for v in crate::manifest::native_aliases(&plan.terms, &manifest_terms) {
            if !ex.contains(&v) {
                ex.push(v);
            }
        }
        plan.terms = ex;
    }
    plan.validate()
        .map_err(|e| (ViewError::InvalidRequest, e))?;
    let mut result = engine.run(&plan);

    // Guarded Evidence Recipes (§41-§43): on auto/force, match a stored recipe
    // for this project scope + execute it against the CURRENT view, appending
    // produced current-view witness relations into `related`. Never a required
    // dependency — failure/absence leaves the deterministic result intact.
    let recipe = if matches!(params.recipes, crate::recipe::RecipePolicy::Off) {
        None
    } else {
        let state_dir = params
            .state_override
            .map(|p| p.to_path_buf())
            .unwrap_or_else(crate::view::state_dir);
        let scope = crate::memory::project::project_scope(&view);
        let store = crate::recipe::RecipeStore::load(&state_dir, &scope).unwrap_or_default();
        let (m, r) = crate::recipe::match_recipe(store.recipes.values(), &plan.terms, plan.intent);
        match (m, r) {
            (crate::recipe::RecipeMatch::Match, Some(rc)) => {
                let out = crate::recipe::execute(&rc, &engine, &result);
                for h in &out.produced {
                    result.related.push(h.clone());
                }
                Some(out)
            }
            _ => None,
        }
    };

    // Trajectory utility policy (§24-§28): `off` default. `shadow` computes the
    // would-be selection without changing the packet; `apply` reorders the
    // OPTIONAL seed tail only (mandatory identity + FACT/CANDIDATE untouched).
    // historical symbols against THIS validated view (never replayed as current).
    let memory = {
        let state_dir = params
            .state_override
            .map(|p| p.to_path_buf())
            .unwrap_or_else(crate::view::state_dir);
        let index_dir = ensure.index_dir();
        let known: std::collections::BTreeSet<String> =
            result.seeds.iter().map(|s| s.node.path.clone()).collect();
        // §38-§41 minimality: a trivial single-exact decl lookup with no
        // relations/path intent stays minimal — memory adds no navigation value.
        let trivial = result.seeds.len() == 1
            && result.related.is_empty()
            && plan.intent == crate::query::QueryIntent::Find
            && matches!(
                result.seeds[0].node.kind,
                crate::graph::model::NodeKind::Declaration
            );
        crate::memory::compose::compose(
            &view,
            &index_dir,
            params.memory_mode,
            params.query_text,
            &known,
            &state_dir,
            trivial,
        )
    };

    // Deterministic query shape for adaptive compilation (§5-§7).
    let (shape, _reason) = crate::context::classify(plan.intent, &result);

    let (utility_trace, delivery) =
        if matches!(params.utility_policy, crate::utility::UtilityPolicy::Off) {
            (None, None)
        } else {
            let state_dir = params
                .state_override
                .map(|p| p.to_path_buf())
                .unwrap_or_else(crate::view::state_dir);
            let scope = crate::memory::project::project_scope(&view);
            let stats = crate::utility::UtilityStats::load(&state_dir);
            let decision_id = crate::repository::digest::content_digest(
                format!("{}|{}", params.query_text, started.elapsed().as_nanos()).as_bytes(),
            );
            let eligible: Vec<String> = result
                .seeds
                .iter()
                .skip(1)
                .map(|s| s.node.key.clone())
                .collect();
            let mandatory: Vec<String> = result
                .seeds
                .iter()
                .take(1)
                .map(|s| s.node.key.clone())
                .collect();
            let tr = crate::utility::decide(
                &mut result.seeds,
                &format!("{:?}", shape),
                &scope,
                &stats,
                params.utility_policy,
                &decision_id,
            );
            let dec = crate::utility::DeliveryDecision {
                schema: crate::utility::UTILITY_SCHEMA.into(),
                decision_id: decision_id.clone(),
                scope,
                query_family: plan.terms.join("|"),
                query_shape: format!("{:?}", shape),
                policy_version: params.utility_policy.as_str().into(),
                eligible,
                mandatory,
                selected: tr.policy_optional.clone(),
                est_bytes: 0,
                actual_bytes: 0,
            };
            // §8: persist the delivery decision so later trajectory observations can
            // reference the exact slate. Append-only decisions.jsonl, project dir.
            {
                let dir = state_dir.join("utility");
                let _ = std::fs::create_dir_all(&dir);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("decisions.jsonl"))
                {
                    use std::io::Write;
                    let _ = writeln!(f, "{}", serde_json::to_string(&dec).unwrap_or_default());
                }
            }
            (Some(tr), Some(dec))
        };

    // §31: optional memory composition — additive guidance only, rebinding

    let view_root = view.canonical_root.clone();
    Ok(ViewQueryOutcome {
        view,
        result,
        ensure,
        memory,
        shape,
        context_policy: params.context_policy,
        recipe,
        utility_trace,
        delivery,
        view_root,
        source_witness: params.source_witness,
        total_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Build a ViewLocator from a machine request body, enforcing the same
/// root XOR project rules (§9, §30).
pub fn locator_from_request(req: &QueryRequest) -> ViewResult<ViewLocator> {
    match (&req.root, &req.project, &req.view) {
        (Some(_), Some(_), _) => err(
            ViewError::LocatorConflict,
            "`root` and `project` are mutually exclusive".to_string(),
        ),
        (Some(_), _, Some(_)) => err(
            ViewError::LocatorConflict,
            "`view` requires `project`, not `root`".to_string(),
        ),
        (Some(r), None, None) => Ok(ViewLocator::Root(std::path::PathBuf::from(r))),
        (None, Some(p), v) => Ok(ViewLocator::Project {
            project: p.clone(),
            view: v.clone(),
        }),
        (None, None, Some(_)) => err(
            ViewError::LocatorConflict,
            "`view` requires `project`".to_string(),
        ),
        (None, None, None) => err(
            ViewError::InvalidRequest,
            "request needs `root` or `project`".to_string(),
        ),
    }
}
