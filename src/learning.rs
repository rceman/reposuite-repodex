//! Continuous project-learning foundation (V1).
//!
//! Two strictly separated planes (§2):
//!   Plane A CURRENT_REPOSITORY_TRUTH — only current-view evidence.
//!   Plane B LEARNED_PROJECT_NAVIGATION_KNOWLEDGE — how to investigate, never
//!   what is currently true. LEARNED != CURRENT; memory can never upgrade a
//!   current CANDIDATE/UNKNOWN/STALE to FACT without current evidence (§4-§5).
//!
//! Deterministic only: no LLM in the path, JEV off. Read-only over the project.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::graph::{GraphIndex, GraphNode, NodeKind};

// ---------------------------------------------------------------------------
// Coverage ledger (§12-§14)
// ---------------------------------------------------------------------------

/// A coverage dimension — a meaningful evidence obligation, not a scalar %.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageDimension {
    Entrypoint,
    Lifecycle,
    ModuleBoundaries,
    Ownership,
    CallPath,
    ConfigControl,
    StateStorage,
    ExternalBoundary,
    TestVerification,
    FailureRecovery,
    Dependency,
    CrossCuttingBehavior,
}

/// Explicit bounded coverage state (§13). `Known` requires satisfied current
/// evidence obligations; it must never be a fabricated "understood" claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// All required current evidence satisfied.
    Known,
    /// Some required evidence exists but material gaps remain.
    Partial,
    /// Can be investigated but the required evidence is not yet established.
    Unresolved,
    /// Current RepoDex capabilities cannot investigate this class reliably.
    Unsupported,
    /// Deterministically not applicable to this project (with a reason).
    NotApplicable,
}

impl CoverageState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::Partial => "partial",
            Self::Unresolved => "unresolved",
            Self::Unsupported => "unsupported",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// One coverage item — a dimension tied to the repository state that
/// justified it (current-view bound, §14).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageItem {
    pub dimension: CoverageDimension,
    pub state: CoverageState,
    /// Deterministic basis: what current evidence established/downgraded this.
    pub basis: String,
    /// The RepositoryView fingerprint/digest this state was computed on.
    pub view_digest: String,
    /// Why NOT_APPLICABLE (required when that state is used) or the gap.
    pub reason: Option<String>,
}

/// The durable coverage ledger for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoverageLedger {
    pub view_digest: String,
    pub items: Vec<CoverageItem>,
}

// ---------------------------------------------------------------------------
// Question curriculum (§15-§18)
// ---------------------------------------------------------------------------

/// A bounded self-question family — questions come from evidence gaps only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionFamily {
    Entrypoint,
    Lifecycle,
    Ownership,
    CallPath,
    ConfigControl,
    StateStorage,
    ExternalBoundary,
    FailureRecovery,
    TestVerification,
    Dependency,
    CrossCuttingBehavior,
    Ambiguity,
}

impl QuestionFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entrypoint => "entrypoint",
            Self::Lifecycle => "lifecycle",
            Self::Ownership => "ownership",
            Self::CallPath => "call_path",
            Self::ConfigControl => "config_control",
            Self::StateStorage => "state_storage",
            Self::ExternalBoundary => "external_boundary",
            Self::FailureRecovery => "failure_recovery",
            Self::TestVerification => "test_verification",
            Self::Dependency => "dependency",
            Self::CrossCuttingBehavior => "cross_cutting_behavior",
            Self::Ambiguity => "ambiguity",
        }
    }
}

/// Why a question exists — always a concrete evidence gap origin (§16).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionOrigin {
    CoverageGap(CoverageDimension),
    AmbiguousAnchor,
    UnresolvedEdge,
    ModuleBoundary,
    ManifestSurface,
    EntrypointDownstream,
    AgentFallback,
    TaskDemand,
}

/// A deterministic, auditable investigation question candidate (§45).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionCandidate {
    pub question_id: String,
    pub family: QuestionFamily,
    pub question: String,
    pub anchors: Vec<String>,
    pub origin: String,
    /// auditable priority factors (§18) — no Jev.
    pub factors: BTreeMap<String, i64>,
    /// deterministic priority score.
    pub priority: i64,
    /// estimated investigation cost (RepoDex calls + read bytes proxy).
    pub est_cost: i64,
}

/// Deterministic priority heuristic (§18,§33): structural importance +
/// uncertainty reduction + demand - cost - churn risk. A ranking heuristic,
/// not a probability.
pub fn question_priority(
    structural_importance: i64,
    uncertainty_reduction: i64,
    demand: i64,
    est_cost: i64,
    churn_risk: i64,
) -> i64 {
    structural_importance * 4 + uncertainty_reduction * 3 + demand * 2 - est_cost - churn_risk
}

// ---------------------------------------------------------------------------
// Learned project memory (§22-§25,§30)
// ---------------------------------------------------------------------------

/// Explicit memory lifecycle state (§23). Conservative promotion only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryState {
    /// Newly derived; not yet trusted as active.
    Candidate,
    /// Reused successfully / independently revalidated -> active.
    Validated,
    /// Current rebind failed or dependencies changed.
    Stale,
    /// Repeatedly unhelpful or superseded.
    Demoted,
    /// Retired from active retrieval (kept for audit).
    Retired,
}

/// A reusable navigation artifact — HOW to investigate, not WHAT is true.
/// No natural-language answer is stored as authoritative truth (§3,§22).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryArtifact {
    pub memory_id: String,
    pub family: QuestionFamily,
    /// Anchor selectors used to rebind to a current view.
    pub anchor_names: Vec<String>,
    /// The productive structural route (typed relations followed), e.g.
    /// ["contains","call_candidate"] — a route, not a cached answer.
    pub route: Vec<String>,
    /// Evidence classes the route produced.
    pub evidence_classes: Vec<String>,
    /// Source/content dependencies for safe reuse (digests, not line nos).
    pub source_dependencies: Vec<String>,
    /// RepositoryView digest where this memory was learned.
    pub origin_view: String,
    /// View digest at last successful rebind.
    pub last_validated_view: String,
    pub state: MemoryState,
    pub reuse_count: u64,
    pub useful_reuse_count: u64,
    pub failed_reuse_count: u64,
}

/// The rebind outcome for a memory against the current view (§6). Never a
/// silent discard and never partial-as-full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRebind {
    CurrentValid,
    CurrentPartial,
    Stale,
    Ambiguous,
    Missing,
    Unsupported,
}

impl MemoryRebind {
    /// Only CurrentValid is usable as current evidence; Partial is bounded,
    /// everything else is a gap. None upgrade FACT/CANDIDATE.
    pub fn usable_current(self) -> bool {
        matches!(self, MemoryRebind::CurrentValid)
    }
}

// ---------------------------------------------------------------------------
// Bootstrap + coverage derivation (§11-§13,§31-§33)
// ---------------------------------------------------------------------------

/// The deterministic project inventory the bootstrap builds (§11).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectInventory {
    pub languages: Vec<String>,
    pub manifests: Vec<String>,
    pub modules: Vec<String>,
    pub entrypoints: Vec<String>,
    pub files: usize,
    pub declarations: usize,
    pub calls: usize,
    pub test_files: Vec<String>,
}

/// Build the deterministic project inventory from the index (no LLM).
pub fn inventory(index: &GraphIndex) -> ProjectInventory {
    let mut inv = ProjectInventory::default();
    let mut langs = BTreeSet::new();
    let mut modules = BTreeSet::new();
    for n in index.nodes() {
        if !n.language.is_empty() {
            langs.insert(n.language.clone());
        }
        match n.kind {
            NodeKind::File => {
                inv.files += 1;
                let p = n.path.as_str();
                if p.ends_with("go.mod") || p.ends_with("Cargo.toml") || p.ends_with("package.json")
                {
                    inv.manifests.push(p.to_string());
                }
                if p.contains("test") || p.contains("_test") || p.contains("tests/") {
                    inv.test_files.push(p.to_string());
                }
                if let Some(dir) = p.rsplit('/').nth(1) {
                    modules.insert(dir.to_string());
                }
            }
            NodeKind::Declaration => {
                inv.declarations += 1;
                if n.label == "main" || n.label == "init" {
                    inv.entrypoints.push(format!("{}:{}", n.path, n.label));
                }
            }
            NodeKind::Call => inv.calls += 1,
            _ => {}
        }
    }
    inv.languages = langs.into_iter().collect();
    inv.modules = modules.into_iter().collect();
    inv.manifests.sort();
    inv.test_files.sort();
    inv
}

/// Derive the coverage ledger from the inventory + graph (deterministic, §12).
/// `view_digest` binds every item to the repository state that justified it.
pub fn coverage(_index: &GraphIndex, inv: &ProjectInventory, view_digest: &str) -> CoverageLedger {
    let mk = |d, s, basis: String, r: Option<String>| CoverageItem {
        dimension: d,
        state: s,
        basis,
        view_digest: view_digest.to_string(),
        reason: r,
    };
    let has_manifest = !inv.manifests.is_empty();
    let has_tests = !inv.test_files.is_empty();
    let has_main = !inv.entrypoints.is_empty();
    let items = vec![
        mk(
            CoverageDimension::Entrypoint,
            if has_main {
                CoverageState::Known
            } else {
                CoverageState::Unresolved
            },
            format!("entrypoints={}", inv.entrypoints.len()),
            if has_main {
                None
            } else {
                Some("no main/init declaration detected".into())
            },
        ),
        mk(
            CoverageDimension::ModuleBoundaries,
            if inv.modules.len() > 1 {
                CoverageState::Partial
            } else {
                CoverageState::NotApplicable
            },
            format!("modules={}", inv.modules.len()),
            if inv.modules.len() <= 1 {
                Some("single-module project".into())
            } else {
                None
            },
        ),
        mk(
            CoverageDimension::ConfigControl,
            if has_manifest {
                CoverageState::Partial
            } else {
                CoverageState::NotApplicable
            },
            format!("manifests={}", inv.manifests.len()),
            if has_manifest {
                None
            } else {
                Some("no manifest/config file".into())
            },
        ),
        mk(
            CoverageDimension::TestVerification,
            if has_tests {
                CoverageState::Partial
            } else {
                CoverageState::Unresolved
            },
            format!("test_files={}", inv.test_files.len()),
            None,
        ),
        mk(
            CoverageDimension::CallPath,
            if inv.calls > 0 {
                CoverageState::Partial
            } else {
                CoverageState::NotApplicable
            },
            format!("calls={}", inv.calls),
            if inv.calls == 0 {
                Some("no call sites detected".into())
            } else {
                None
            },
        ),
        mk(
            CoverageDimension::Dependency,
            if has_manifest {
                CoverageState::Partial
            } else {
                CoverageState::NotApplicable
            },
            "manifest dependency surface".into(),
            if has_manifest {
                None
            } else {
                Some("no manifest".into())
            },
        ),
        // dimensions current deterministic extraction cannot establish reliably
        mk(
            CoverageDimension::FailureRecovery,
            CoverageState::Unsupported,
            "no runtime failure signal in static index".into(),
            None,
        ),
        mk(
            CoverageDimension::StateStorage,
            CoverageState::Unsupported,
            "state mutation is runtime evidence".into(),
            None,
        ),
        mk(
            CoverageDimension::Lifecycle,
            CoverageState::Unresolved,
            "lifecycle requires path tracing".into(),
            None,
        ),
        mk(
            CoverageDimension::Ownership,
            CoverageState::Partial,
            "containment/membership edges".into(),
            None,
        ),
        mk(
            CoverageDimension::ExternalBoundary,
            CoverageState::Unresolved,
            "import/link surface".into(),
            None,
        ),
        mk(
            CoverageDimension::CrossCuttingBehavior,
            CoverageState::Unresolved,
            "cross-cutting requires multi-anchor".into(),
            None,
        ),
    ];
    CoverageLedger {
        view_digest: view_digest.to_string(),
        items,
    }
}

/// Generate question candidates from coverage gaps only (§16) — never arbitrary
/// curiosity. Returns an auditable candidate set.
pub fn generate_questions(
    ledger: &CoverageLedger,
    inv: &ProjectInventory,
) -> Vec<QuestionCandidate> {
    let mut out = Vec::new();
    let mut n = 0u64;
    let mut q = |family: QuestionFamily,
                 text: String,
                 anchors: Vec<String>,
                 origin: String,
                 si: i64,
                 ur: i64,
                 dm: i64,
                 cost: i64| {
        n += 1;
        out.push(QuestionCandidate {
            question_id: format!("q-{n}"),
            family,
            question: text,
            anchors,
            origin,
            factors: BTreeMap::from([
                ("structural_importance".into(), si),
                ("uncertainty_reduction".into(), ur),
                ("demand".into(), dm),
                ("cost".into(), -cost),
            ]),
            priority: question_priority(si, ur, dm, cost, 0),
            est_cost: cost,
        });
    };
    for it in &ledger.items {
        match (it.dimension, it.state) {
            (CoverageDimension::Entrypoint, CoverageState::Unresolved | CoverageState::Partial) => {
                for e in &inv.entrypoints {
                    q(
                        QuestionFamily::Entrypoint,
                        format!("What does {e} initialize and call?"),
                        vec![e.clone()],
                        "entrypoint_downstream".into(),
                        4,
                        3,
                        2,
                        1,
                    );
                }
            }
            (CoverageDimension::CallPath, CoverageState::Unresolved | CoverageState::Partial) => {
                q(
                    QuestionFamily::CallPath,
                    "Which callers reach the primary entrypoint chain?".into(),
                    inv.entrypoints.clone(),
                    "coverage_gap:call_path".into(),
                    4,
                    3,
                    2,
                    2,
                );
            }
            (
                CoverageDimension::ConfigControl,
                CoverageState::Unresolved | CoverageState::Partial,
            ) => {
                for m in &inv.manifests {
                    q(
                        QuestionFamily::ConfigControl,
                        format!("What does {m} control and who consumes it?"),
                        vec![m.clone()],
                        "manifest_surface".into(),
                        3,
                        3,
                        2,
                        1,
                    );
                }
            }
            (
                CoverageDimension::TestVerification,
                CoverageState::Unresolved | CoverageState::Partial,
            ) => {
                q(
                    QuestionFamily::TestVerification,
                    "Which tests verify the core modules?".into(),
                    inv.test_files.clone(),
                    "coverage_gap:test_verification".into(),
                    3,
                    3,
                    2,
                    2,
                );
            }
            (CoverageDimension::Ownership, CoverageState::Unresolved | CoverageState::Partial) => {
                for m in inv.modules.iter().take(6) {
                    q(
                        QuestionFamily::Ownership,
                        format!("Which module owns {m} and what does it expose?"),
                        vec![m.clone()],
                        "module_boundary".into(),
                        3,
                        2,
                        1,
                        1,
                    );
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.question_id.cmp(&b.question_id))
    });
    out
}

// ---------------------------------------------------------------------------
// Rebind + lifecycle (§5-§8,§14,§24-§26)
// ---------------------------------------------------------------------------

/// Rebind a memory artifact's anchors to the current view. Only CurrentValid
/// becomes usable evidence; a disappeared anchor -> Missing, a now-ambiguous
/// anchor -> Ambiguous, a changed dependency -> Stale. Never partial-as-full.
pub fn rebind_memory(mem: &MemoryArtifact, index: &GraphIndex, view_digest: &str) -> MemoryRebind {
    // dependency changed -> stale
    if !mem.source_dependencies.is_empty() && view_digest != mem.last_validated_view {
        // only stale if a dependency actually changed; we treat any view digest
        // change for a memory with deps as needing revalidation.
        let mut all_present = true;
        let mut ambiguous = false;
        for name in &mem.anchor_names {
            let matches: Vec<&GraphNode> = index
                .nodes()
                .iter()
                .filter(|n| n.label == *name || n.key == *name)
                .collect();
            if matches.is_empty() {
                all_present = false;
            } else if matches.len() > 1 {
                ambiguous = true;
            }
        }
        if !all_present {
            return MemoryRebind::Missing;
        }
        if ambiguous {
            return MemoryRebind::Ambiguous;
        }
        return MemoryRebind::Stale; // deps changed; anchors present but not revalidated
    }
    // same view -> resolve anchors
    for name in &mem.anchor_names {
        let matches: Vec<&GraphNode> = index
            .nodes()
            .iter()
            .filter(|n| n.label == *name || n.key == *name)
            .collect();
        if matches.is_empty() {
            return MemoryRebind::Missing;
        }
        if matches.len() > 1 {
            return MemoryRebind::Ambiguous;
        }
    }
    MemoryRebind::CurrentValid
}

/// Conservative promotion: a derived route becomes Candidate; only a real
/// successful reuse or independent revalidation promotes to Validated (§24).
pub fn promote(mem: &mut MemoryArtifact, successful_reuse: bool) {
    match mem.state {
        MemoryState::Candidate if successful_reuse => mem.state = MemoryState::Validated,
        _ => {}
    }
}

/// Demotion: a failed rebind or repeated useless reuse demotes the memory (§25).
pub fn demote(mem: &mut MemoryArtifact, rebind: MemoryRebind, view_digest: &str) {
    match rebind {
        MemoryRebind::Missing | MemoryRebind::Stale => {
            mem.state = MemoryState::Stale;
            mem.failed_reuse_count += 1;
        }
        MemoryRebind::Ambiguous => mem.failed_reuse_count += 1,
        _ => {}
    }
    if mem.failed_reuse_count >= 3 {
        mem.state = MemoryState::Demoted;
    }
    let _ = view_digest;
}
