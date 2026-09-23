//! Per-session derivation: fold one session's canonical AgentEvents into a
//! `SessionPart` — the incremental unit. A session part is re-derived from its
//! (bounded) session file when new tail events appear (§30-§33).

use std::collections::BTreeMap;

use crate::agent_event::AgentEvent;
use crate::repository::digest;

use super::model::*;

/// Day index = unix days since epoch (for bounded buckets).
fn day_index(ts: &str) -> i64 {
    // parse "YYYY-MM-DD" prefix -> days-from-civil
    let (y, rest) = match ts.split_once('-') {
        Some(x) => x,
        None => return 0,
    };
    let (m, d) = match rest.split_once('-') {
        Some(x) => x,
        None => return 0,
    };
    let (y, m, d) = match (y.parse::<i64>(), m.parse::<i64>(), d[..2].parse::<i64>()) {
        (Ok(y), Ok(m), Ok(d)) => (y, m, d),
        _ => return 0,
    };
    let yy = if m <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Minimal whitespace/case/Unicode normalization for a future join key (§7).
/// No semantic similarity — deterministic only.
pub fn normalize_query(q: &str) -> String {
    q.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A per-session derived aggregate (the incremental/rebuild unit). Persisted
/// per session so an investigation can be rebuilt from bounded parts.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SessionPart {
    pub session_id: String,
    pub investigation_id: Option<String>,
    pub project_id: Option<String>,
    pub repository_id: Option<String>,
    pub repo_head: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub completed: bool,
    // query
    pub query_text: Option<String>,
    // model usage
    pub model_calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cached_output_tokens: u64,
    pub reasoning_tokens: u64,
    pub saw_token_fields: bool,
    pub usage_is_actual: bool,
    pub per_model: BTreeMap<String, ModelUsage>,
    // tools
    pub tools: ToolTotals,
    // exposure per path (sorted by path for determinism)
    pub exposures: BTreeMap<String, PathExposure>,
    // final answers (session_id -> answer)
    pub final_answers: Vec<(String, String)>, // (digestless) content
    // outcomes
    pub session_outcomes: Vec<String>,
    pub task_outcomes: Vec<String>,
    /// Paths surfaced via `context_artifact_presented` (path, rank). Surfaced
    /// is NOT observed/read (§7).
    #[serde(default)]
    pub surfaced_paths: Vec<(String, Option<u64>)>,
    // for merge
    pub max_sequence: u64,
    pub event_count: u64,
}

fn obs_kind(e: &AgentEvent) -> &str {
    e.data
        .get("observation_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("other")
}

/// Merge a line range into a bounded sorted set (merge overlapping/adjacent).
fn merge_range(ranges: &mut Vec<(u64, u64)>, a: u64, b: u64) {
    ranges.push((a, b));
    ranges.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (x, y) in ranges.drain(..) {
        if let Some(last) = merged.last_mut() {
            if x <= last.1.saturating_add(1) {
                if y > last.1 {
                    last.1 = y;
                }
                continue;
            }
        }
        merged.push((x, y));
    }
    // bound: keep at most 256 merged ranges
    if merged.len() > 256 {
        merged.truncate(256);
    }
    *ranges = merged;
}

/// Fold one session's events into a SessionPart. Deterministic.
pub fn derive_session(events: &[AgentEvent]) -> SessionPart {
    let mut p = SessionPart {
        usage_is_actual: true,
        ..Default::default()
    };
    let mut first_ts: Option<String> = None;
    let mut last_ts: Option<String> = None;
    let mut task_hint: Option<String> = None;
    for e in events {
        p.event_count += 1;
        if e.sequence > p.max_sequence {
            p.max_sequence = e.sequence;
        }
        if p.session_id.is_empty() {
            p.session_id = e.session_id.clone();
        }
        // identity
        if p.investigation_id.is_none() {
            p.investigation_id = e.investigation_id.clone();
        }
        if p.project_id.is_none() {
            p.project_id = e.project_id.clone();
        }
        if p.repository_id.is_none() {
            p.repository_id = e.repository_id.clone();
        }
        if p.repo_head.is_none() {
            p.repo_head = e.repo_head.clone();
        }
        if first_ts.is_none() {
            first_ts = Some(e.timestamp.clone());
        }
        last_ts = Some(e.timestamp.clone());
        match e.event_type.as_str() {
            "session_started" => {
                // session/task label is only a hint; the authoritative query is
                // the first user-role message (§6).
                if task_hint.is_none() {
                    task_hint = e
                        .data
                        .get("task")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
            "agent_message" => {
                // authoritative user task query (§6): first user message.
                if e.data.get("role").and_then(|v| v.as_str()) == Some("user")
                    && p.query_text.is_none()
                {
                    if let Some(c) = e.data.get("content").and_then(|v| v.as_str()) {
                        p.query_text = Some(c.to_string());
                    }
                }
            }
            "model_call_completed" => {
                p.model_calls += 1;
                let it = e.data.get("input_tokens").and_then(|v| v.as_u64());
                let ot = e.data.get("output_tokens").and_then(|v| v.as_u64());
                let ci = e.data.get("cached_input_tokens").and_then(|v| v.as_u64());
                let co = e.data.get("cached_output_tokens").and_then(|v| v.as_u64());
                let rt = e.data.get("reasoning_tokens").and_then(|v| v.as_u64());
                if it.is_some() || ot.is_some() {
                    p.saw_token_fields = true;
                }
                // Any estimated usage flag -> the episode is not purely actual.
                if e.data.get("usage_estimated").and_then(|v| v.as_bool()) == Some(true) {
                    p.usage_is_actual = false;
                }
                p.input_tokens += it.unwrap_or(0);
                p.output_tokens += ot.unwrap_or(0);
                p.cached_input_tokens += ci.unwrap_or(0);
                p.cached_output_tokens += co.unwrap_or(0);
                p.reasoning_tokens += rt.unwrap_or(0);
                let model = e
                    .data
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let mu = p.per_model.entry(model).or_default();
                mu.calls += 1;
                if let Some(v) = it {
                    *mu.input_tokens.get_or_insert(0) += v;
                }
                if let Some(v) = ot {
                    *mu.output_tokens.get_or_insert(0) += v;
                }
                if let Some(v) = ci {
                    *mu.cached_input_tokens.get_or_insert(0) += v;
                }
                if let Some(v) = co {
                    *mu.cached_output_tokens.get_or_insert(0) += v;
                }
                if let Some(v) = rt {
                    *mu.reasoning_tokens.get_or_insert(0) += v;
                }
            }
            "tool_call_started" => {
                p.tools.tool_calls_total += 1;
                let name = e
                    .data
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                *p.tools
                    .native_tool_counts
                    .entry(name.to_string())
                    .or_insert(0) += 1;
                match e.data.get("category").and_then(|v| v.as_str()) {
                    Some("search") => p.tools.search_calls += 1,
                    Some("file_read") => p.tools.file_read_calls += 1,
                    Some("directory_list") => p.tools.directory_list_calls += 1,
                    Some("git_inspection") => p.tools.git_inspection_calls += 1,
                    Some("shell") => p.tools.shell_calls += 1,
                    Some("non_repository_tool") => p.tools.non_repository_tool_calls += 1,
                    _ => p.tools.other_repository_tool_calls += 1,
                }
            }
            "tool_call_completed" => {
                if e.data.get("ok").and_then(|v| v.as_bool()) == Some(false) {
                    p.tools.failed_tool_calls += 1;
                }
            }
            "source_observed" => {
                let path = match e.data.get("path").and_then(|v| v.as_str()) {
                    Some(x) if !x.is_empty() => x.to_string(),
                    _ => continue, // external/unparseable -> not a repo path (§19)
                };
                let seq = e.sequence;
                let ex = p
                    .exposures
                    .entry(path.clone())
                    .or_insert_with(|| PathExposure {
                        path,
                        first_observed_sequence: seq,
                        last_observed_sequence: seq,
                        first_observed_at: e.timestamp.clone(),
                        last_observed_at: e.timestamp.clone(),
                        ..Default::default()
                    });
                ex.observation_event_count += 1;
                if seq < ex.first_observed_sequence {
                    ex.first_observed_sequence = seq;
                    ex.first_observed_at = e.timestamp.clone();
                }
                if seq > ex.last_observed_sequence {
                    ex.last_observed_sequence = seq;
                    ex.last_observed_at = e.timestamp.clone();
                }
                match obs_kind(e) {
                    "explicit_read" => ex.explicit_read_count += 1,
                    "search_snippet" => ex.search_snippet_count += 1,
                    "symbol_preview" => ex.symbol_preview_count += 1,
                    "diff" => ex.diff_observation_count += 1,
                    _ => ex.other_observation_count += 1,
                }
                if let Some(b) = e.data.get("bytes").and_then(|v| v.as_u64()) {
                    ex.observed_bytes_total += b;
                }
                let sl = e.data.get("line_start").and_then(|v| v.as_u64());
                let el = e.data.get("line_end").and_then(|v| v.as_u64());
                if let (Some(a), Some(b)) = (sl, el) {
                    merge_range(&mut ex.line_ranges, a, b);
                }
            }
            "final_answer" => {
                if let Some(c) = e.data.get("content").and_then(|v| v.as_str()) {
                    p.final_answers.push((e.session_id.clone(), c.to_string()));
                }
            }
            "session_completed" => {
                p.completed = true;
                p.session_outcomes.push(
                    e.data
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("runtime_ended")
                        .to_string(),
                );
            }
            "task_outcome" => {
                p.task_outcomes.push(
                    e.data
                        .get("outcome")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                );
            }
            "context_artifact_presented" => {
                // surfaced repository references (§3-§7) — NOT observed/read.
                if let Some(refs) = e.data.get("references").and_then(|v| v.as_array()) {
                    for r in refs {
                        if let Some(path) = r.get("path").and_then(|v| v.as_str()) {
                            let rank = r.get("rank").and_then(|v| v.as_u64());
                            if !p.surfaced_paths.iter().any(|(x, _)| *x == path) {
                                p.surfaced_paths.push((path.to_string(), rank));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    p.started_at = first_ts;
    p.completed_at = last_ts;
    // fall back to the session/task label only if no user message existed (§6).
    if p.query_text.is_none() {
        p.query_text = task_hint;
    }
    p
}

/// Merge a delta part (new tail events) into a stored session part. Add-only:
/// sessions are append-only so delta contributions are purely additive (§30).
pub fn merge_part(base: &mut SessionPart, d: &SessionPart) {
    if base.investigation_id.is_none() {
        base.investigation_id = d.investigation_id.clone();
    }
    if base.project_id.is_none() {
        base.project_id = d.project_id.clone();
    }
    if base.repository_id.is_none() {
        base.repository_id = d.repository_id.clone();
    }
    if base.repo_head.is_none() {
        base.repo_head = d.repo_head.clone();
    }
    if base.query_text.is_none() {
        base.query_text = d.query_text.clone();
    }
    if let (Some(a), Some(b)) = (&base.started_at, &d.started_at) {
        if b < a {
            base.started_at = d.started_at.clone();
        }
    } else if base.started_at.is_none() {
        base.started_at = d.started_at.clone();
    }
    if let (Some(a), Some(b)) = (&base.completed_at, &d.completed_at) {
        if b > a {
            base.completed_at = d.completed_at.clone();
        }
    } else if base.completed_at.is_none() {
        base.completed_at = d.completed_at.clone();
    }
    base.completed |= d.completed;
    base.model_calls += d.model_calls;
    base.input_tokens += d.input_tokens;
    base.output_tokens += d.output_tokens;
    base.cached_input_tokens += d.cached_input_tokens;
    base.cached_output_tokens += d.cached_output_tokens;
    base.reasoning_tokens += d.reasoning_tokens;
    base.saw_token_fields |= d.saw_token_fields;
    base.usage_is_actual &= d.usage_is_actual;
    for (m, u) in &d.per_model {
        let x = base.per_model.entry(m.clone()).or_default();
        x.calls += u.calls;
        if let Some(v) = u.input_tokens {
            *x.input_tokens.get_or_insert(0) += v;
        }
        if let Some(v) = u.output_tokens {
            *x.output_tokens.get_or_insert(0) += v;
        }
        if let Some(v) = u.cached_input_tokens {
            *x.cached_input_tokens.get_or_insert(0) += v;
        }
        if let Some(v) = u.cached_output_tokens {
            *x.cached_output_tokens.get_or_insert(0) += v;
        }
        if let Some(v) = u.reasoning_tokens {
            *x.reasoning_tokens.get_or_insert(0) += v;
        }
    }
    let t = &d.tools;
    base.tools.tool_calls_total += t.tool_calls_total;
    base.tools.search_calls += t.search_calls;
    base.tools.file_read_calls += t.file_read_calls;
    base.tools.directory_list_calls += t.directory_list_calls;
    base.tools.git_inspection_calls += t.git_inspection_calls;
    base.tools.shell_calls += t.shell_calls;
    base.tools.other_repository_tool_calls += t.other_repository_tool_calls;
    base.tools.non_repository_tool_calls += t.non_repository_tool_calls;
    base.tools.failed_tool_calls += t.failed_tool_calls;
    for (k, v) in &t.native_tool_counts {
        *base.tools.native_tool_counts.entry(k.clone()).or_insert(0) += v;
    }
    for (path, pe) in &d.exposures {
        let e = base
            .exposures
            .entry(path.clone())
            .or_insert_with(|| PathExposure {
                path: path.clone(),
                first_observed_sequence: pe.first_observed_sequence,
                last_observed_sequence: pe.last_observed_sequence,
                first_observed_at: pe.first_observed_at.clone(),
                last_observed_at: pe.last_observed_at.clone(),
                ..Default::default()
            });
        e.observation_event_count += pe.observation_event_count;
        e.explicit_read_count += pe.explicit_read_count;
        e.search_snippet_count += pe.search_snippet_count;
        e.symbol_preview_count += pe.symbol_preview_count;
        e.diff_observation_count += pe.diff_observation_count;
        e.other_observation_count += pe.other_observation_count;
        e.observed_bytes_total += pe.observed_bytes_total;
        if pe.first_observed_sequence < e.first_observed_sequence {
            e.first_observed_sequence = pe.first_observed_sequence;
            e.first_observed_at = pe.first_observed_at.clone();
        }
        if pe.last_observed_sequence > e.last_observed_sequence {
            e.last_observed_sequence = pe.last_observed_sequence;
            e.last_observed_at = pe.last_observed_at.clone();
        }
        for (a, b) in &pe.line_ranges {
            merge_range(&mut e.line_ranges, *a, *b);
        }
    }
    base.final_answers.extend(d.final_answers.iter().cloned());
    base.session_outcomes
        .extend(d.session_outcomes.iter().cloned());
    base.task_outcomes.extend(d.task_outcomes.iter().cloned());
    if d.max_sequence > base.max_sequence {
        base.max_sequence = d.max_sequence;
    }
    base.event_count += d.event_count;
}

/// sha256 digest of a final answer's content.
pub fn answer_digest(text: &str) -> String {
    digest::sha256_text(text)
}

/// Day index accessor for activity bucketing.
pub fn ts_day(ts: &str) -> i64 {
    day_index(ts)
}
