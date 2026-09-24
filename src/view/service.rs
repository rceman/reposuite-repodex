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

    Ok(ViewQueryOutcome {
        view,
        result,
        ensure,
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
