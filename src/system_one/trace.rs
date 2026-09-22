//! Optional per-call observability for System One (live benchmark §6).
//!
//! When `REPODEX_SO_TRACE=<path>` is set, each `decide` call appends one JSON
//! line recording role/batch/model/latency/token-usage/status. Never records
//! the Authorization header or API key.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

/// One observed System One API call.
#[derive(Debug, Clone, Serialize)]
pub struct CallRecord {
    pub role: String,
    pub batch: usize,
    /// Configured model instance name (the `[models.<name>]` key).
    pub configured_model: String,
    /// Provider-returned resolved model, if reported.
    pub resolved_model: Option<String>,
    pub wall_latency_ms: f64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub question_count: usize,
    pub state_bytes: usize,
    /// `ok` or an error kind.
    pub status: String,
    /// Whether the role then fell back to the deterministic baseline.
    pub fallback: bool,
    pub fallback_reason: Option<String>,
}

/// The env var that enables tracing.
pub const TRACE_ENV: &str = "REPODEX_SO_TRACE";

/// Configured trace path, if any.
pub fn trace_path() -> Option<PathBuf> {
    std::env::var(TRACE_ENV)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Extract provider usage/model metadata from a response's extra fields.
pub fn usage_from_extra(
    extra: &std::collections::BTreeMap<String, Value>,
) -> (Option<String>, Option<u64>, Option<u64>) {
    let resolved = extra
        .get("model")
        .or_else(|| extra.get("resolved_model"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let usage = extra.get("usage").or_else(|| extra.get("token_usage"));
    let get = |k: &str| -> Option<u64> {
        usage
            .and_then(|u| u.get(k))
            .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
    };
    let input = get("input_tokens")
        .or_else(|| get("prompt_tokens"))
        .or_else(|| get("input"));
    let output = get("output_tokens")
        .or_else(|| get("completion_tokens"))
        .or_else(|| get("output"));
    (resolved, input, output)
}

/// Append a call record as a JSONL line (best-effort; never fails a query).
pub fn record(rec: &CallRecord) {
    let Some(path) = trace_path() else {
        return;
    };
    if let Ok(line) = serde_json::to_string(rec) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
    }
}
