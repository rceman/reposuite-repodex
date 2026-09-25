//! Evidence-utility model (Part II-IV, §5-§14).
//!
//! Measures *which* evidence representations actually reduce downstream Agent
//! work — strictly distinguishing OBSERVED / OUTCOME_ASSOCIATED /
//! CAUSALLY_VALIDATED. No score is a truth probability (§1).

use serde::{Deserialize, Serialize};

/// Utility model schema identity.
pub const UTILITY_SCHEMA: &str = "reposuite.repodex.utility.v1";

/// Representation kind (§6) — the *form* of evidence, distinct from the entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReprKind {
    PathHint,
    Locator,
    DeclarationRange,
    BodyRange,
    FactRelation,
    CandidateRelation,
    ConnectorRoute,
    FileMemory,
    SymbolMemory,
    RecipeWitness,
}

/// Evidence role (what obligation a unit serves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole {
    Definition,
    Disambiguation,
    Relationship,
    Connector,
    TestCandidate,
    Orientation,
}

/// The version-safe identity of one piece of delivered evidence (§5-§7).
/// No line numbers — identity is entity+representation+role, not coordinates.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceUnit {
    /// Canonical project scope.
    pub scope: String,
    /// Entity identity: a locator or node key (path-qualified, not a line).
    pub entity: String,
    pub repr: ReprKind,
    pub role: EvidenceRole,
    /// `fact` | `candidate` — preserved, never changed by policy.
    pub truth_class: String,
}

impl EvidenceUnit {
    /// Stable unit id = content digest of the identity tuple (no coordinates).
    pub fn id(&self) -> String {
        let key = format!(
            "{}|{}|{:?}|{:?}|{}",
            self.scope, self.entity, self.repr, self.role, self.truth_class
        );
        crate::repository::digest::content_digest(key.as_bytes())
    }
}

/// Whether a unit is optional (selectable) or mandatory protocol metadata (§9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Optionality {
    Mandatory,
    Optional,
}

/// One delivery decision: the exact evidence slate a query produced (§8-§10).
/// Persisted/derived so later trajectory observations reference the slate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryDecision {
    pub schema: String,
    /// Stable unique decision id.
    pub decision_id: String,
    pub scope: String,
    pub query_family: String,
    pub query_shape: String,
    pub policy_version: String,
    /// Optional evidence units eligible for selection.
    pub eligible: Vec<String>,
    /// Mandatory units always emitted (not in the competition).
    pub mandatory: Vec<String>,
    /// Selected optional units, in presentation order.
    pub selected: Vec<String>,
    /// Estimated bytes before emission and actual serialized bytes after (§15).
    pub est_bytes: usize,
    pub actual_bytes: usize,
}

/// Bounded downstream-continuation metrics observed after a delivery (§11-§14).
/// All RUNTIME_OBSERVABLE or DERIVED; evaluator-only truth stays out of prod.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContinuationMetrics {
    /// Searches/reads/tool-calls before first primary-evidence use.
    pub searches_before_primary: Option<u64>,
    pub reads_before_primary: Option<u64>,
    pub tool_calls_before_primary: Option<u64>,
    /// Same for sufficient-evidence attainment.
    pub searches_before_sufficient: Option<u64>,
    pub reads_before_sufficient: Option<u64>,
    /// Did the agent expand a delivered representation (e.g. signature->body)?
    pub expansions: u64,
    /// Total subsequent investigation work (tool calls).
    pub subsequent_tool_calls: u64,
    /// A delivered unit was referenced in the final answer (OUTCOME_ASSOCIATED).
    pub final_mentioned: bool,
}
