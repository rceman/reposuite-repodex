//! Live service memory derivation (Part XII, §42-§47).
//!
//! Completes the generic live loop: AgentEvents fold into episodes, symbol
//! exposures, and InvestigationMemory. Events are mirrored into an
//! `AgentEventStore` (`{state}/agent-events`), then `derive()` +
//! `derive_symbol_exposures()` + `build_memory()`/`enrich_symbols()` update
//! the derived/memory stores incrementally (checkpoint-bounded). Replay of the
//! same durable event history converges to the same memory (§44). Symbol
//! resolution only binds exposures whose source version is addressable —
//! unresolved versions are recorded, never invented (§46).

use std::collections::BTreeSet;
use std::path::Path;

use crate::agent_event::AgentEventStore;
use crate::derived;
use crate::memory::build_memory;
use crate::symbol_exposure as sx;

/// Incrementally derive episodes + memory (and symbol exposures where a view
/// snapshot is resolvable) from the durable AgentEvent history under `state_dir`.
/// `view_roots` = repos the service is currently serving (for snapshot lookup).
/// Returns a small diagnostic report. Never blocks the ingest ACK path.
pub fn derive_live(state_dir: &Path, view_roots: &[String]) -> serde_json::Value {
    let ev_dir = state_dir.join("agent-events");
    let estore = match AgentEventStore::open(&ev_dir) {
        Ok(s) => s,
        Err(e) => return serde_json::json!({"error": format!("event store: {e}")}),
    };
    // 1) Episodes (incremental via the derived-store checkpoint).
    let known: BTreeSet<String> = BTreeSet::new();
    let cur: BTreeSet<String> = BTreeSet::new();
    let (ds, drep) = match derived::derive(&estore, &state_dir.join("derived"), false, &known, None)
    {
        Ok(x) => x,
        Err(e) => return serde_json::json!({"error": format!("derive: {e}")}),
    };
    // 2) Symbol exposures for each served repo snapshot (bounded).
    let mut sexp_reports = Vec::new();
    let sexp_dir = state_dir.join("symbol-exposure");
    for root in view_roots {
        if let Some(rep) = derive_repo_symbols(&estore, root, &sexp_dir, state_dir) {
            sexp_reports.push(rep);
        }
    }
    // 3) Memory: rebuild changed entries, enrich with symbol evidence.
    let (mut ms, _mrep) = match build_memory(&ds, &state_dir.join("memory"), false, &known, &cur) {
        Ok(x) => x,
        Err(e) => return serde_json::json!({"error": format!("memory: {e}")}),
    };
    let sexp_store = sx::SymbolExposureStore::open(&sexp_dir);
    ms.enrich_symbols(&sexp_store, &cur);
    let _ = ms.save(&state_dir.join("memory"));
    serde_json::json!({
        "episodes": ds.episodes.len(),
        "derived": drep.sessions_processed,
        "memory_investigations": ms.entries.len(),
        "symbol_enriched": sexp_reports.len(),
    })
}

/// Derive symbol exposures for one served repo: resolve the repo's current view
/// snapshot -> provider -> fold that repo's events. Returns a report or None
/// when the repo has no resolvable snapshot yet.
fn derive_repo_symbols(
    estore: &AgentEventStore,
    root: &str,
    sexp_dir: &Path,
    state_dir: &Path,
) -> Option<serde_json::Value> {
    let view = crate::view::resolve(&crate::view::ViewLocator::Root(std::path::PathBuf::from(
        root,
    )))
    .ok()?;
    let analyzer = crate::parser::Analyzer::new(crate::parser::AnalyzerConfig::default()).ok()?;
    let mut view = view;
    let ensure = crate::view::index::ensure_index(&analyzer, &mut view, Some(state_dir)).ok()?;
    let snap = ensure.index_dir().join("snapshot");
    let manifest = crate::repository::artifact::read_manifest(&snap).ok()?;
    let sources = sx::SourceStore::default();
    let provider = sx::SymbolMapProvider::new(&snap, &manifest, &sources)
        .with_source_root(&view.canonical_root);
    let mut store = sx::SymbolExposureStore::open(sexp_dir);
    // Only this repo's sessions contribute (multi-project isolation §47).
    let scope = crate::memory::project::project_scope(&view);
    let mut n = 0u64;
    for sid in estore.sessions().ok()? {
        let events = estore.session_events(&sid).ok()?;
        let mine: Vec<_> = events
            .iter()
            .filter(|e| {
                crate::memory::project::scope_from_episode(
                    e.project_id.as_deref(),
                    e.repository_id.as_deref(),
                )
                .as_deref()
                    == Some(scope.as_str())
            })
            .cloned()
            .collect();
        if !mine.is_empty() {
            sx::store::derive_symbol_exposures(&mine, &provider, &mut store);
            n += 1;
        }
    }
    let _ = store.save();
    Some(serde_json::json!({"root": root, "sessions": n, "scope": scope}))
}
