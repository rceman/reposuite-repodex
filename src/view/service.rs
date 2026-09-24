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
    plan.validate()
        .map_err(|e| (ViewError::InvalidRequest, e))?;
    let result = engine.run(&plan);

    // §31: optional memory composition — additive guidance only, rebinding
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

    Ok(ViewQueryOutcome {
        view,
        result,
        ensure,
        memory,
        shape,
        context_policy: params.context_policy,
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
