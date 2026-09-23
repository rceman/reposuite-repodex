//! InvestigationMemory model — historical navigation priors derived from
//! InvestigationEpisodes. NOT code truth (§2): memory may influence navigation
//! only, never promote an entity to source truth. Not a query->answer cache
//! (§17): memory points to paths/entities/investigation patterns.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::signature::QuerySignature;

/// Memory schema version.
pub const MEMORY_SCHEMA_VERSION: u32 = 1;

/// Explainable evidence-strength class for a remembered path (§23). A path may
/// hold several; they are not equally strong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStrength {
    /// Presented in a context artifact but no further interaction (§24: neutral).
    SurfacedOnly = 0,
    /// Source content delivered via a tool result.
    Observed = 1,
    /// Agent explicitly requested the source.
    ExplicitlyRead = 2,
    /// Path named in the final answer.
    FinalAnswerMention = 3,
    /// Path named in a structured EVIDENCE block.
    StructuredEvidenceMention = 4,
    /// An authoritative outcome confirmed the investigation.
    OutcomeConfirmed = 5,
}

/// Memory freshness against a current repository snapshot (§30). `changed` is
/// not automatically invalid (§32); `removed` is excluded from normal output
/// (§33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    Changed,
    Removed,
    Unknown,
}

/// Per-(investigation,path) factual memory evidence (§22). Not collapsed into
/// one opaque score — the fields stay factual.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryEvidence {
    pub path: String,
    // presentation (§7) — three facts stay separate
    pub surfaced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surfaced_rank: Option<u64>,
    // observation
    pub observed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_observed_sequence: Option<u64>,
    // explicit read
    pub explicitly_read: bool,
    pub explicit_read_count: u64,
    // final answer
    pub mentioned_in_final_answer: bool,
    pub structured_evidence_mention: bool,
    // efficiency (§26-§27)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u64>,
    // provenance (§29)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    #[serde(default)]
    pub freshness: Option<Freshness>,
    /// outcome evidence (session/task; §22) — never correctness invention
    #[serde(default)]
    pub task_outcomes: Vec<String>,
}

impl MemoryEvidence {
    /// The strongest evidence class this path holds (§23).
    pub fn strongest(&self) -> EvidenceStrength {
        if self
            .task_outcomes
            .iter()
            .any(|o| o.contains("accept") || o.contains("pass") || o.contains("success"))
        {
            return EvidenceStrength::OutcomeConfirmed;
        }
        if self.structured_evidence_mention {
            return EvidenceStrength::StructuredEvidenceMention;
        }
        if self.mentioned_in_final_answer {
            return EvidenceStrength::FinalAnswerMention;
        }
        if self.explicitly_read {
            return EvidenceStrength::ExplicitlyRead;
        }
        if self.observed {
            return EvidenceStrength::Observed;
        }
        EvidenceStrength::SurfacedOnly
    }
}

/// One remembered investigation: its query signature + per-path evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub schema: String,
    pub schema_version: u32,
    pub investigation_id: String,
    pub session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_head: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    pub signature: QuerySignature,
    /// path -> factual evidence
    #[serde(default)]
    pub evidence: BTreeMap<String, MemoryEvidence>,
    /// investigation-level cost (§26)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub tool_calls: u64,
    #[serde(default)]
    pub task_outcomes: Vec<String>,
}

/// A scored candidate investigation returned by a memory query (§46).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMatch {
    pub investigation_id: String,
    pub similarity: f64,
    /// Explainable component breakdown (§46).
    pub components: BTreeMap<String, f64>,
    /// Remembered paths sorted by evidence strength.
    pub paths: Vec<MemoryEvidence>,
}

/// Memory store manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryManifest {
    pub schema: String,
    pub schema_version: u32,
    pub investigations: u64,
    pub posting_terms: u64,
    pub artifact_bytes: u64,
    pub content_digest: String,
}

/// Memory checkpoint for incremental updates (§36).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryCheckpoint {
    pub schema_version: u32,
    /// derived-store digest the memory was built against
    pub derived_digest: String,
    /// investigation_id -> a digest of its contributing episode (detects change)
    #[serde(default)]
    pub processed: BTreeMap<String, String>,
}
