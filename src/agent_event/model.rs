//! Canonical AgentEvent v1 — a **RepoSuite-level**, harness-neutral telemetry
//! contract (§2). Harness-neutral: the envelope and
//! semantics are runtime-agnostic so RepoSuite Relay can emit the same events
//! natively without changing consumers (§40/§86).
//!
//! The ingest layer records *observations*, not judgments — it never decides
//! useful/irrelevant/correct/wrong (§4). Raw events preserve neutral evidence.

use serde::{Deserialize, Serialize};

/// Canonical schema identifier (§5).
pub const AGENT_EVENT_SCHEMA: &str = "reposuite.agent-event.v1";
/// Envelope version this build understands.
pub const AGENT_EVENT_VERSION: u32 = 1;

/// Producer provenance (§9). Consumers must not branch on `runtime` for normal
/// event semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenance {
    /// Agent runtime (opaque producer metadata — RepoDex never branches on it).
    pub runtime: String,
    /// Adapter/normalizer name (opaque provenance, e.g. a Relay-side adapter).
    pub adapter: String,
    /// Adapter contract version.
    pub adapter_version: String,
    /// Native runtime session id, preserved for provenance (§44).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_session_id: Option<String>,
}

/// The canonical event envelope. `event_type`+`data` are intentionally loosely
/// typed so an unknown future event type round-trips without corruption (§11):
/// the store keeps `data` opaque rather than rejecting an unknown type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvent {
    /// Must equal [`AGENT_EVENT_SCHEMA`].
    pub schema: String,
    /// Globally/idempotently unique within ingestion scope. Derived as
    /// `"{session_id}:{sequence}"` — replaying the same trajectory reproduces
    /// the same ids (§6, §29).
    pub event_id: String,
    /// One concrete agent/runtime session.
    pub session_id: String,
    /// Logical investigation/task id — may span multiple sessions (§6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub investigation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Stable repository identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    /// Git commit the agent investigated — for memory staleness (§6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_head: Option<String>,
    /// Monotonic order within the session (§7). Primary ordering authority.
    pub sequence: u64,
    /// RFC3339/ISO-8601 UTC timestamp (§8). For wall-time analysis only.
    pub timestamp: String,
    pub source: SourceProvenance,
    /// Event family, e.g. `session_started`, `tool_call_completed`, or a
    /// future/unknown type preserved verbatim.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Typed payload. Opaque for unknown types.
    #[serde(default)]
    pub data: serde_json::Value,
    /// sha256 of canonical semantic content — conflict detection for a
    /// repeated `event_id` (§30).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
}

impl AgentEvent {
    /// Deterministic event_id for a session+sequence (§6/§29).
    pub fn make_event_id(session_id: &str, sequence: u64) -> String {
        format!("{session_id}:{sequence}")
    }
}

// --- canonical event families (§10) -----------------------------------------

/// `session_started` payload (§12).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStarted {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_head: Option<String>,
    /// Task/query metadata reference or summary (never a giant prompt dump).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

/// `session_completed` payload (§53) — runtime execution ended; NOT a product
/// quality outcome (§23).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionCompleted {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_steps: Option<u64>,
}

/// `model_call_started` (§13).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCallStarted {
    pub model_call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// `model_call_completed` (§14). Usage fields are optional individually and are
/// ACTUAL provider/harness values — never estimated. `usage_estimated` marks a
/// proxy when the runtime only exposes estimates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCallCompleted {
    pub model_call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// True when token usage is a proxy/estimate rather than provider-reported.
    #[serde(default)]
    pub usage_estimated: bool,
}

/// Normalized tool category (§15). Native `tool_name` is always retained;
/// `category` is the harness-neutral bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Search,
    FileRead,
    DirectoryList,
    GitInspection,
    Shell,
    #[default]
    OtherRepositoryTool,
    NonRepositoryTool,
}

/// `tool_call_started` (§15).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallStarted {
    pub tool_call_id: String,
    /// Native runtime tool name (e.g. `read`, `grep`, `find_file_by_name`).
    pub tool_name: String,
    pub category: ToolCategory,
    /// Safe structured arguments (redacted); never raw secrets (§24).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

/// `tool_call_completed` (§16). Full output is NOT duplicated — only
/// bytes/digest/metadata plus derived `source_observed` events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallCompleted {
    pub tool_call_id: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_digest: Option<String>,
    /// Optional external/raw reference for a large payload (§16/§33).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// `source_observed` observation kind (§18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    ExplicitRead,
    SearchSnippet,
    SymbolPreview,
    Diff,
    #[default]
    Other,
}

impl ObservationKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "explicit_read" => Self::ExplicitRead,
            "search_snippet" => Self::SearchSnippet,
            "symbol_preview" => Self::SymbolPreview,
            "diff" => Self::Diff,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitRead => "explicit_read",
            Self::SearchSnippet => "search_snippet",
            Self::SymbolPreview => "symbol_preview",
            Self::Diff => "diff",
            Self::Other => "other",
        }
    }
}

/// `source_observed` (§17-19): repository source content actually delivered to
/// the model via a tool result. Never emitted for path-argument-only exposure
/// (§50).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceObserved {
    /// Canonical repository-relative normalized path (§19). Absent for
    /// external/non-repository paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub observation_kind: ObservationKind,
    /// The tool call whose output delivered this source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Bytes of the source fragment delivered to the agent (§51).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u64>,
    /// Digest of the source fragment delivered to the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    /// Authoritative whole-file content version (`sha256:` content digest of
    /// the source bytes the agent observed). Binds the observation to an exact
    /// file version for SymbolExposure (symbol-exposure §6). Absent when the
    /// version could not be captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_content_digest: Option<String>,
    /// Optional byte range when the observation is byte-precise (§8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_end: Option<u64>,
}

/// One repository reference inside a presented context artifact (§5).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactReference {
    /// e.g. "path" | "entity" | "symbol"
    pub reference_kind: String,
    /// Canonical repository-relative path (absent for non-repo references).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Entity/symbol identity when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// Presentation rank/position when meaningful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u64>,
    /// Relation/reason metadata when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
}

/// `context_artifact_presented` (§3-§6): external/precomputed context supplied
/// to the Agent before/during investigation — RepoDex RDX packet, retrieval
/// bundle, planner context. Harness-neutral; distinct from `source_observed`
/// (tool-delivered) — a surfaced path is NOT an observed one (§7).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextArtifactPresented {
    /// Stable artifact identity (e.g. packet digest).
    pub artifact_id: String,
    /// e.g. "rdx1_packet" | "retrieval_bundle" | "document_context"
    pub artifact_kind: String,
    /// Producing system, e.g. "repodex".
    pub producer: String,
    /// Digest of the full artifact payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    /// Artifact payload bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_bytes: Option<u64>,
    /// When presented (RFC3339); sequence provides order authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presented_at: Option<String>,
    /// Structured repository references (paths/entities, with rank).
    #[serde(default)]
    pub references: Vec<ArtifactReference>,
}

/// `agent_message` (§20).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
}

/// `final_answer` (§21).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FinalAnswer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
}

/// `task_outcome` (§22) — external lifecycle/product outcome, distinct from
/// `session_completed` and from any future evaluator score (§23).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskOutcome {
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}
