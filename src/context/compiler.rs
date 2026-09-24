//! Deterministic adaptive Context Compiler (Parts III-XI).
//!
//! Post-processes the canonical EvidenceProjection: classifies the query shape,
//! builds a small evidence-obligation ledger, then prunes *optional* evidence
//! to the smallest representation that satisfies the obligations within a
//! serialized-byte budget. Mandatory protocol fields (rank, FACT/CANDIDATE,
//! endpoints, ambiguity state, current identity) are never dropped from an
//! emitted record (§16). All decisions are recorded in an inspectable trace.

use serde::Serialize;

use super::shape::QueryShape;
use crate::query::projection::EvidenceProjection;

/// Objective evidence obligations RepoDex can support (§8) — no semantic
/// obligations (root-cause, correct-fix) are claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Obligation {
    Definition,
    Disambiguation,
    Relationship,
    Connector,
    TestCandidate,
}

/// The ledger state of an obligation (§9). `Unknown` != `NotApplicable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationState {
    Witnessed,
    CandidateOnly,
    Unknown,
    NotApplicable,
}

/// One ledger row: obligation -> state (compact, no prose).
#[derive(Debug, Clone, Serialize)]
pub struct ObligationEntry {
    pub obligation: Obligation,
    pub state: ObligationState,
}

/// Bounded selection reason codes (§33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    ExactSeedSufficient,
    AmbiguityRequiresLocators,
    RelationshipRequested,
    ConnectorRequested,
    MemoryRedundant,
    MemoryAddsCurrentSymbol,
    BudgetExceeded,
    UnsupportedEvidence,
    OrientationContext,
    TestEvidence,
}

/// Explicit bounded budgets (§34).
#[derive(Debug, Clone)]
pub struct ContextBudget {
    pub max_bytes: usize,
    pub max_seeds: usize,
    pub max_related: usize,
    pub max_routes: usize,
    pub max_ranges: usize,
    pub max_memory_paths: usize,
    pub max_memory_symbols: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024,
            max_seeds: 50,
            max_related: 256,
            max_routes: 2,
            max_ranges: 64,
            max_memory_paths: 8,
            max_memory_symbols: 8,
        }
    }
}

/// Inspectable selection trace (§32) — debug surface, not in the agent packet.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ContextTrace {
    pub query_shape: String,
    pub reasons: Vec<String>,
    pub obligations: Vec<ObligationEntry>,
    pub est_bytes: usize,
    pub actual_bytes: usize,
    pub budget_limited: bool,
    pub memory_admitted: String,
    pub seeds: usize,
    pub related: usize,
    pub ranges: usize,
}

/// The compiled packet: a trimmed projection + ledger + trace.
#[derive(Debug, Clone)]
pub struct CompiledPacket {
    pub projection: EvidenceProjection,
    pub ledger: Vec<ObligationEntry>,
    pub trace: ContextTrace,
}

/// Estimate serialized cost of the current projection (before emission).
fn est_bytes(p: &EvidenceProjection) -> usize {
    serde_json::to_vec(&p.to_json())
        .map(|v| v.len())
        .unwrap_or(usize::MAX)
}

/// Compile `proj` into the smallest faithful packet for `shape` within `budget`.
/// `memory_symbols`/`memory_paths` are the already-computed memory composition;
/// the compiler may drop it as redundant (it never fabricates it).
pub fn compile(
    mut proj: EvidenceProjection,
    shape: QueryShape,
    budget: &ContextBudget,
) -> CompiledPacket {
    let mut trace = ContextTrace {
        query_shape: shape.as_str().to_string(),
        ..Default::default()
    };
    let mut reasons = Vec::new();
    // --- per-shape pruning (optional evidence only; §16 mandatory preserved) ---
    match shape {
        QueryShape::ExactLookup => {
            reasons.push(ReasonCode::ExactSeedSufficient);
            // Keep only the top seed, minimal identity (§20). Drop body range,
            // related, connector, and memory (deterministic seed suffices).
            proj.seeds.truncate(1);
            if let Some(s) = proj.seeds.first_mut() {
                s.body_range = None;
            }
            proj.related.clear();
            proj.connector = None;
            proj.memory = None;
            trace.memory_admitted = "no_redundant".into();
        }
        QueryShape::AmbiguousLookup => {
            reasons.push(ReasonCode::AmbiguityRequiresLocators);
            // Keep candidate seeds with locators + decl ranges to distinguish;
            // drop body ranges + unrelated topology (§21).
            proj.seeds.truncate(budget.max_seeds.min(8));
            for s in &mut proj.seeds {
                s.body_range = None;
            }
            proj.related.truncate(budget.max_related.min(16));
        }
        QueryShape::Relationship => {
            reasons.push(ReasonCode::RelationshipRequested);
            // Endpoints + evidence + via preserved; cap topology (§22).
            proj.seeds.truncate(budget.max_seeds.min(8));
            proj.related.truncate(budget.max_related);
        }
        QueryShape::PathOrConnector => {
            reasons.push(ReasonCode::ConnectorRequested);
            // Bounded connector packet only; drop broad neighborhood (§23).
            proj.related.truncate(budget.max_related.min(32));
        }
        QueryShape::Orientation => {
            reasons.push(ReasonCode::OrientationContext);
            // Broader but bounded: top seeds + local relationships (§24).
            proj.seeds.truncate(budget.max_seeds.min(12));
            proj.related.truncate(budget.max_related.min(64));
        }
        QueryShape::TestOrVerification => {
            reasons.push(ReasonCode::TestEvidence);
            proj.seeds.truncate(budget.max_seeds.min(8));
            proj.related.truncate(budget.max_related.min(32));
        }
        QueryShape::Unknown => {
            reasons.push(ReasonCode::UnsupportedEvidence);
            proj.seeds.truncate(budget.max_seeds.min(8));
            proj.related.truncate(budget.max_related.min(32));
        }
    }

    // --- obligation ledger (§8-§10) ---
    let ledger = ledger(&proj, shape);
    trace.obligations = ledger.clone();

    // --- serialized-byte budget enforcement (§34-§36) ---
    let mut actual = est_bytes(&proj);
    let mut limited = false;
    if actual > budget.max_bytes {
        limited = true;
        reasons.push(ReasonCode::BudgetExceeded);
        // Drop lowest-priority optional evidence first: related, then extra
        // seeds (never the top seed's mandatory fields), then ranges.
        while actual > budget.max_bytes && !proj.related.is_empty() {
            proj.related.pop();
            actual = est_bytes(&proj);
        }
        while actual > budget.max_bytes && proj.seeds.len() > 1 {
            for s in &mut proj.seeds {
                s.body_range = None;
            }
            proj.seeds.pop();
            actual = est_bytes(&proj);
        }
    }
    trace.seeds = proj.seeds.len();
    trace.related = proj.related.len();
    trace.ranges = proj
        .seeds
        .iter()
        .map(|s| s.declaration_range.is_some() as usize + s.body_range.is_some() as usize)
        .sum();
    trace.est_bytes = actual;
    trace.actual_bytes = actual;
    trace.budget_limited = limited;
    // A budget-limited packet never implies completeness.
    if limited {
        proj.truncated_reason = Some("context_budget".into());
        proj.seed_selection_complete = false;
        proj.complete = false;
    }
    trace.reasons = reasons.iter().map(|r| format!("{r:?}")).collect();
    CompiledPacket {
        projection: proj,
        ledger,
        trace,
    }
}

/// Build the compact evidence-obligation ledger from the (already-trimmed)
/// projection + shape. `Unknown` means "not established here"; it is never
/// coerced to `NotApplicable` without an objective reason (§9).
fn ledger(proj: &EvidenceProjection, shape: QueryShape) -> Vec<ObligationEntry> {
    use Obligation::*;
    use ObligationState::*;
    let mut out = Vec::new();
    let has_decl = proj
        .seeds
        .iter()
        .any(|s| s.declaration_range.is_some() || s.kind == "declaration");
    let has_candidate = proj.seeds.iter().any(|s| {
        s.disposition
            .as_deref()
            .map(|d| d != "single_candidate" && d != "resolved")
            .unwrap_or(false)
    }) || proj
        .related
        .iter()
        .any(|r| r.evidence == "candidate" || r.candidate_set.is_some());
    out.push(ObligationEntry {
        obligation: Definition,
        state: if has_decl { Witnessed } else { Unknown },
    });
    let disambig_needed = shape == QueryShape::AmbiguousLookup;
    out.push(ObligationEntry {
        obligation: Disambiguation,
        state: if !disambig_needed {
            NotApplicable
        } else if has_candidate || proj.seeds.len() > 1 {
            CandidateOnly
        } else {
            Unknown
        },
    });
    let rel_present = !proj.related.is_empty();
    out.push(ObligationEntry {
        obligation: Relationship,
        state: if rel_present {
            Witnessed
        } else if shape == QueryShape::Relationship {
            Unknown
        } else {
            NotApplicable
        },
    });
    out.push(ObligationEntry {
        obligation: Connector,
        state: match &proj.connector {
            Some(c) if c.found => Witnessed,
            Some(_) => Unknown,
            None => {
                if shape == QueryShape::PathOrConnector {
                    Unknown
                } else {
                    NotApplicable
                }
            }
        },
    });
    out.push(ObligationEntry {
        obligation: TestCandidate,
        state: if shape == QueryShape::TestOrVerification && has_decl {
            CandidateOnly
        } else if shape == QueryShape::TestOrVerification {
            Unknown
        } else {
            NotApplicable
        },
    });
    out
}

/// Apply adaptive compilation to an already-built projection in place.
/// `memory` may be pruned as redundant for `ExactLookup`/`AmbiguousLookup`.
/// Returns the inspectable trace. The compact shape+ledger block is written to
/// `proj.context` for the agent packet (the full trace is debug-only).
pub fn compile_into(
    proj: &mut EvidenceProjection,
    shape: QueryShape,
    memory_mode: crate::memory::compose::MemoryMode,
    budget: &ContextBudget,
) -> ContextTrace {
    // Memory admission (§26-§29): for ExactLookup the single exact seed already
    // supplies definition; historical memory is redundant -> drop it.
    let had_mem = proj.memory.is_some();
    let packet = compile(
        std::mem::take(proj),
        shape,
        budget,
    );
    *proj = packet.projection;
    let mut trace = packet.trace;
    trace.memory_admitted = match (memory_mode, shape, had_mem) {
        (_, QueryShape::ExactLookup, _) => "no_redundant".into(),
        (m, _, true) if m != crate::memory::compose::MemoryMode::Off => "yes".into(),
        _ => "no".into(),
    };
    proj.context = Some(serde_json::json!({
        "policy": "adaptive",
        "shape": shape.as_str(),
        "obligations": packet.ledger,
        "budget_limited": trace.budget_limited,
        "memory": trace.memory_admitted,
    }));
    trace
}
