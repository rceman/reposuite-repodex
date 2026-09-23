//! Derived telemetry model — InvestigationEpisode, SourceExposure,
//! AgentActivity. These are REBUILDABLE derived artifacts from canonical
//! `AgentEvent v1`; the raw store stays authoritative. Strict layer separation
//! (§3): episode = one investigation summary; exposure = what source the agent
//! actually saw; activity = accumulated per-path usage. No usefulness scores.

use serde::{Deserialize, Serialize};

/// Derived schema version — bump when the derivation semantics change.
pub const DERIVED_SCHEMA_VERSION: u32 = 1;
/// Derived artifact manifest version.
pub const DERIVED_MANIFEST_VERSION: u32 = 1;

/// Days of history retained in bounded daily buckets (§28). Lifetime totals are
/// kept separately so compaction never destroys them (§29).
pub const ACTIVITY_WINDOW_DAYS: i64 = 400;

/// Neutral lifecycle state derived from canonical events only (§5). Execution
/// end, task outcome and future quality evaluation stay distinct (§22-§23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationState {
    /// No terminal session_completed observed yet.
    Open,
    /// All observed sessions completed.
    Completed,
    /// A task_outcome reported failure/timeout (execution-level).
    Failed,
    /// Some sessions completed, some not.
    Partial,
}

/// Per-path source exposure within one session/investigation (§11-§12).
/// A path is exposed only when its CONTENT was delivered to the model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathExposure {
    /// Canonical repository-relative path.
    pub path: String,
    pub first_observed_sequence: u64,
    pub last_observed_sequence: u64,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub observation_event_count: u64,
    pub explicit_read_count: u64,
    pub search_snippet_count: u64,
    pub symbol_preview_count: u64,
    pub diff_observation_count: u64,
    pub other_observation_count: u64,
    /// Sum of delivered fragment bytes (may double-count repeated fragments §14).
    pub observed_bytes_total: u64,
    /// Merged, bounded observed line ranges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_ranges: Vec<(u64, u64)>,
}

/// A repository path present in a final answer (§17-§20). A mention is not a
/// verified citation and implies no usefulness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathMention {
    pub path: String,
    /// Whether this path was source_observed during the investigation (§20).
    pub observed_during_investigation: bool,
    /// Whether it was explicitly read.
    pub explicitly_read: bool,
    /// First sequence it was observed, if observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_observed_sequence: Option<u64>,
}

/// A final answer reference (§16, §21).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FinalAnswerRef {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_bytes: Option<u64>,
    /// Canonical repo paths present in the answer (mentions, not citations).
    #[serde(default)]
    pub path_mentions: Vec<String>,
    /// Path-like strings that did not resolve to a known repo path.
    #[serde(default)]
    pub unresolved_path_mentions: Vec<String>,
}

/// Per-model usage breakdown (§8). All fields are ACTUAL usage or absent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    pub calls: u64,
}

/// Tool totals (§10).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolTotals {
    pub tool_calls_total: u64,
    pub search_calls: u64,
    pub file_read_calls: u64,
    pub directory_list_calls: u64,
    pub git_inspection_calls: u64,
    pub shell_calls: u64,
    pub other_repository_tool_calls: u64,
    pub non_repository_tool_calls: u64,
    pub failed_tool_calls: u64,
    /// Native tool name -> count (retained, §10).
    #[serde(default)]
    pub native_tool_counts: std::collections::BTreeMap<String, u64>,
}

/// The derived per-investigation summary (§4-§22).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestigationEpisode {
    pub schema: String,
    pub schema_version: u32,
    pub investigation_id: String,
    pub session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    /// All distinct repo_heads observed (plural; merged sessions may differ §4).
    #[serde(default)]
    pub repo_heads: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub state: InvestigationState,
    // query (§6-§7)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_digest: Option<String>,
    // model usage (§8-§9)
    pub model_calls: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// True only when all token fields are actual (not estimated).
    pub usage_is_actual: bool,
    #[serde(default)]
    pub per_model: std::collections::BTreeMap<String, ModelUsage>,
    // tools (§10)
    pub tools: ToolTotals,
    // source exposure (§11-§14)
    #[serde(default)]
    pub source_exposures: Vec<PathExposure>,
    pub unique_source_files_observed: u64,
    pub unique_source_files_explicitly_read: u64,
    pub source_observation_events: u64,
    pub source_exposure_bytes_total: u64,
    // final answers (§16-§20)
    #[serde(default)]
    pub final_answers: Vec<FinalAnswerRef>,
    /// Path mentions joined to observed/read state (§20).
    #[serde(default)]
    pub final_answer_path_mentions: Vec<PathMention>,
    /// Mentions inside a structured `EVIDENCE:` block — stricter than a plain
    /// mention (§21).
    #[serde(default)]
    pub evidence_path_mentions: Vec<String>,
    /// Path-like strings not resolvable to known repo paths.
    #[serde(default)]
    pub unresolved_path_mentions: Vec<String>,
    // outcomes (§22) — neutral, no correctness invention
    #[serde(default)]
    pub session_outcomes: Vec<String>,
    #[serde(default)]
    pub task_outcomes: Vec<String>,
    /// Reserved for a future evaluator join; always absent now (§22).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_outcome: Option<String>,
}

/// One day of activity in a bounded bucket (§28).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyBucket {
    pub observations: u64,
    pub explicit_reads: u64,
    /// Distinct investigation ids active that day (bounded; enables
    /// investigations_Nd without re-scanning).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub investigation_ids: Vec<String>,
}

/// AgentActivity record — accumulated usage for one (repository_id, path)
/// (§23-§29). Observational only; no usefulness/ranking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathActivity {
    pub repository_id: String,
    pub path: String,
    // lifetime totals (§25,§29)
    pub observation_event_count_total: u64,
    pub explicit_read_count_total: u64,
    pub search_snippet_count_total: u64,
    pub symbol_preview_count_total: u64,
    pub diff_observation_count_total: u64,
    pub other_observation_count_total: u64,
    pub exposed_bytes_total: u64,
    /// Distinct sessions whose final answer mentioned this path (idempotent).
    #[serde(default)]
    pub mention_session_ids: Vec<String>,
    /// Distinct sessions whose structured EVIDENCE block mentioned this path.
    #[serde(default)]
    pub structured_evidence_session_ids: Vec<String>,
    // distinct identities (§26)
    #[serde(default)]
    pub session_ids: Vec<String>,
    #[serde(default)]
    pub investigation_ids: Vec<String>,
    pub first_observed_at: String,
    pub last_observed_at: String,
    /// Bounded recent-window buckets: day-index -> bucket (§27-§28).
    #[serde(default)]
    pub daily: std::collections::BTreeMap<i64, DailyBucket>,
}

impl PathActivity {
    pub fn sessions_observed_count(&self) -> u64 {
        self.session_ids.len() as u64
    }
    /// Distinct sessions that mentioned this path in a final answer.
    pub fn final_answer_mention_count(&self) -> u64 {
        self.mention_session_ids.len() as u64
    }
    /// Distinct sessions whose structured EVIDENCE block named this path.
    pub fn structured_evidence_mention_count(&self) -> u64 {
        self.structured_evidence_session_ids.len() as u64
    }
    pub fn investigations_observed_count(&self) -> u64 {
        self.investigation_ids.len() as u64
    }
    /// Distinct investigations observed within the last `days` before `now_day`.
    pub fn investigations_window(&self, now_day: i64, days: i64) -> u64 {
        let cutoff = now_day - days;
        let mut set = std::collections::BTreeSet::new();
        for (day, b) in &self.daily {
            if *day >= cutoff {
                set.extend(b.investigation_ids.iter().cloned());
            }
        }
        set.len() as u64
    }
    pub fn observations_window(&self, now_day: i64, days: i64) -> u64 {
        let cutoff = now_day - days;
        self.daily
            .iter()
            .filter(|(d, _)| **d >= cutoff)
            .map(|(_, b)| b.observations)
            .sum()
    }
    pub fn reads_window(&self, now_day: i64, days: i64) -> u64 {
        let cutoff = now_day - days;
        self.daily
            .iter()
            .filter(|(d, _)| **d >= cutoff)
            .map(|(_, b)| b.explicit_reads)
            .sum()
    }
}

/// Incremental checkpoint (§31): which per-session events have been folded into
/// derived state, plus store/schema binding for staleness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedCheckpoint {
    pub derived_schema_version: u32,
    /// AgentEvent store schema this derived state was built from.
    pub event_store_digest: String,
    /// session_id -> number of events already folded in (sessions are
    /// append-only; new tail events are the incremental unit).
    #[serde(default)]
    pub session_processed: std::collections::BTreeMap<String, u64>,
}

/// Derived store manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedManifest {
    pub schema: String,
    pub derived_schema_version: u32,
    pub manifest_version: u32,
    pub investigations: u64,
    pub sessions: u64,
    pub activity_paths: u64,
    pub total_events_processed: u64,
    pub artifact_bytes: u64,
    pub content_digest: String,
}
