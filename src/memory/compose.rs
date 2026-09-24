//! Memory composition into the production query path (Part IX, §31-§41).
//!
//! After the deterministic engine runs, an optional memory stage looks up the
//! project-scoped store, rebinds historical symbols against the *current*
//! validated view, and returns bounded annotations for the EvidenceProjection.
//! Memory is additive guidance — it never replaces deterministic retrieval and
//! never produces current evidence without rebinding (§14, §32).

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::project::project_scope;
use super::rebind::{TargetViewMemoryAnnotation, TargetViewRebinder};
use super::store::MemoryStore;
use super::MemoryMatch;

/// Explicit memory policy for a query (§33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    /// No memory lookup at all (deterministic only).
    Off,
    /// Path-level historical hints only (no symbol rebind).
    File,
    /// Path hints + current-view-rebound symbol annotations.
    Symbol,
}

impl MemoryMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "file" => Self::File,
            "symbol" => Self::Symbol,
            _ => Self::Off,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::File => "file",
            Self::Symbol => "symbol",
        }
    }
}

/// A path-level historical navigation hint (file mode + carried in symbol mode).
#[derive(Debug, Clone, Serialize)]
pub struct PathHint {
    pub path: String,
    /// Historical evidence strength label (navigation aid, not current truth).
    pub evidence_strength: String,
    /// Whether the path exists in the current view snapshot.
    pub exists_current: bool,
    /// Historical origin provenance (untouched — §8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_head: Option<String>,
}

/// The bounded memory block attached to an EvidenceProjection.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MemoryComposition {
    /// The project scope the memory was queried under.
    pub project_scope: String,
    /// off|file|symbol — the policy actually applied.
    pub mode: String,
    /// How many historical investigations matched the query.
    pub matched_investigations: u64,
    /// Path-level hints (file + symbol modes).
    #[serde(default)]
    pub paths: Vec<PathHint>,
    /// Current-view-rebound symbol annotations (symbol mode only). Each carries
    /// a BindingState — never a bare `fresh` bool.
    #[serde(default)]
    pub symbols: Vec<TargetViewMemoryAnnotation>,
    /// Counts by binding state (for diagnostics / status).
    #[serde(default)]
    pub binding_counts: std::collections::BTreeMap<String, u64>,
    /// True when memory was unavailable/empty — deterministic query unaffected.
    pub degraded: bool,
}

/// Max symbol annotations per memory path (§36 — proven bounded approach).
pub const MAX_SYMBOLS_PER_PATH: usize = 2;

/// Compose memory for a query against the current validated view.
/// `state_dir` holds the `memory/` store. `index_dir`+`view_root` enable
/// current-view rebinding. Returns an empty (degraded) composition when memory
/// is missing — the deterministic query result is always still valid.
pub fn compose(
    view: &crate::view::RepositoryView,
    index_dir: &Path,
    mode: MemoryMode,
    query_text: &str,
    known_paths: &std::collections::BTreeSet<String>,
    state_dir: &Path,
    single_exact_lookup: bool,
) -> MemoryComposition {
    let scope = project_scope(view);
    let mut out = MemoryComposition {
        project_scope: scope.clone(),
        mode: mode.as_str().to_string(),
        ..Default::default()
    };
    if mode == MemoryMode::Off {
        return out;
    }
    let store = match MemoryStore::load(&state_dir.join("memory")) {
        Ok(s) => s,
        Err(_) => {
            out.degraded = true;
            return out;
        }
    };
    // §4 hard isolation: only entries stamped with this exact project scope.
    let matches: Vec<MemoryMatch> =
        store.query_scoped(query_text, known_paths, 5, 64, Some(&scope));
    out.matched_investigations = matches.len() as u64;
    let mut symbols = Vec::new();
    let mut rebinder = if mode == MemoryMode::Symbol {
        TargetViewRebinder::open(index_dir, &view.canonical_root, &scope)
    } else {
        None
    };
    for m in &matches {
        for p in &m.paths {
            out.paths.push(PathHint {
                path: p.path.clone(),
                evidence_strength: format!("{:?}", p.strongest()).to_lowercase(),
                exists_current: view.canonical_root.join(&p.path).exists(),
                origin_head: p.repo_head.clone(),
            });
        }
        if let Some(rb) = rebinder.as_mut() {
            // §37: prefer stronger evidence — declaration/enclosing/read before
            // reference-frequency. Cap at MAX_SYMBOLS_PER_PATH per path (§36).
            for s in &m.symbols {
                // The stable qualified locator is the rebinding key; bare
                // references without a locator are skipped (no current evidence).
                let Some(loc) = s.locator.clone() else {
                    continue;
                };
                let ann = rb.rebind(
                    &loc,
                    &s.symbol_name,
                    &s.path,
                    s.facets.as_ref(),
                    Some(s.path.clone()),
                    s.file_content_digest.clone(),
                );
                let key = format!("{:?}", ann.state);
                *out.binding_counts.entry(key).or_default() += 1;
                symbols.push(ann);
            }
        }
    }
    // §36: cap symbols per path + total.
    let mut per_path: std::collections::BTreeMap<String, usize> = Default::default();
    symbols.retain(|a| {
        let p = a.origin_path.clone().unwrap_or_default();
        let n = per_path.entry(p).or_default();
        *n += 1;
        *n <= MAX_SYMBOLS_PER_PATH
    });
    symbols.truncate(8);
    out.symbols = symbols;
    // §38-§41 minimality: a trivial single-exact-match lookup keeps the packet
    // small — drop symbol annotations (the lookup is already unambiguous).
    if single_exact_lookup {
        out.symbols.clear();
    }
    out.paths.dedup_by(|a, b| a.path == b.path);
    out
}
