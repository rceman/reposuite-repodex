//! Devin ATIF adapter — the first `AgentEvent` producer (§41).
//!
//! Converts a Devin `--export` ATIF trajectory into canonical `AgentEvent v1`
//! events. All ATIF-specific field names (`prompt_tokens`, `source_call_id`,
//! `<file-view>`, ...) live ONLY here (§43) — the rest of the codebase sees
//! canonical events.
//!
//! Mapping (§46-§53):
//!   step(source=agent,metrics) -> model_call_started + model_call_completed
//!   step.tool_calls[i]         -> tool_call_started
//!   observation.result         -> tool_call_completed (+ source_observed)
//!   read output                -> source_observed explicit_read
//!   grep content output        -> source_observed search_snippet per file
//!   last assistant message     -> final_answer
//!   first/last step            -> session_started / session_completed
//!
//! Token policy: per-agent-step metrics are per-model-call ACTUAL usage; they
//! sum exactly to ATIF `final_metrics` (verified). No estimation.

use serde_json::Value;

use crate::repository::digest;

use super::model::*;
use super::AgentEventError;

/// Optional run metadata for canonical identity during benchmark replay (§45).
#[derive(Debug, Clone, Default)]
pub struct AtifContext {
    /// Authoritative investigation/run id (e.g. `L1-Q1__E0__r0`).
    pub investigation_id: Option<String>,
    pub project_id: Option<String>,
    pub repository_id: Option<String>,
    /// Git HEAD of the investigated checkout.
    pub repo_head: Option<String>,
    /// Repository root used to normalize absolute tool paths to repo-relative.
    pub repo_root: Option<String>,
}

/// Classify a native Devin tool into a harness-neutral category (§15/§48).
/// Native `tool_name` is retained separately; this is only the bucket.
fn tool_category(name: &str) -> ToolCategory {
    match name {
        // Content/name search and file discovery -> `search` (§48 documents
        // find_file_by_name as search/file-discovery).
        "grep" | "find_file_by_name" | "find" | "glob" | "search" | "read_many" => {
            ToolCategory::Search
        }
        "read" | "read_file" => ToolCategory::FileRead,
        "ls" | "list_dir" | "list_files" => ToolCategory::DirectoryList,
        "git" | "git_log" | "git_blame" | "git_diff" | "git_show" => ToolCategory::GitInspection,
        "exec" | "bash" | "shell" | "run" | "sh" => ToolCategory::Shell,
        "write" | "edit" | "edit_file" | "apply_patch" | "notebook_edit" => {
            ToolCategory::OtherRepositoryTool
        }
        "web_search" | "webfetch" | "browser_preview" | "ask_user_question" | "mcp_call_tool"
        | "mcp_list_tools" | "run_subagent" | "skill" | "todo_write" => {
            ToolCategory::NonRepositoryTool
        }
        _ => ToolCategory::OtherRepositoryTool,
    }
}

/// Normalize a tool-referenced absolute path to a repository-relative path
/// (§19). Returns None for external/non-repository paths — never fabricates
/// repository membership.
fn to_repo_path(abs: &str, repo_root: Option<&str>) -> Option<String> {
    let root = repo_root?;
    let root = root.trim_end_matches('/');
    if let Some(rel) = abs.strip_prefix(root) {
        let rel = rel.trim_start_matches('/');
        if !rel.is_empty() && !rel.contains("..") {
            return Some(rel.to_string());
        }
    }
    None
}

struct Emitter<'a> {
    session_id: &'a str,
    ctx: &'a AtifContext,
    seq: u64,
    out: Vec<AgentEvent>,
}

impl Emitter<'_> {
    fn push(&mut self, timestamp: &str, event_type: &str, data: Value) {
        let seq = self.seq;
        self.seq += 1;
        self.out.push(AgentEvent {
            schema: AGENT_EVENT_SCHEMA.to_string(),
            event_id: AgentEvent::make_event_id(self.session_id, seq),
            session_id: self.session_id.to_string(),
            investigation_id: self.ctx.investigation_id.clone(),
            project_id: self.ctx.project_id.clone(),
            repository_id: self.ctx.repository_id.clone(),
            repo_head: self.ctx.repo_head.clone(),
            sequence: seq,
            timestamp: timestamp.to_string(),
            source: SourceProvenance {
                runtime: "devin".into(),
                adapter: "devin-atif".into(),
                adapter_version: "1".into(),
                native_session_id: Some(self.session_id.to_string()),
            },
            event_type: event_type.to_string(),
            data,
            content_digest: None,
        });
    }
}

/// Parse the `<file-view path="..." start_line="N" end_line="M">` header a
/// `read` result emits.
fn parse_file_view(content: &str) -> Option<(String, Option<u64>, Option<u64>)> {
    let hdr = content.split('\n').next()?;
    if !hdr.contains("<file-view") {
        return None;
    }
    let attr = |k: &str| -> Option<String> {
        let pat = format!("{k}=\"");
        let i = hdr.find(&pat)? + pat.len();
        let j = hdr[i..].find('"')? + i;
        Some(hdr[i..j].to_string())
    };
    let path = attr("path")?;
    let sl = attr("start_line").and_then(|s| s.parse().ok());
    let el = attr("end_line").and_then(|s| s.parse().ok());
    Some((path, sl, el))
}

/// Emit `source_observed` events for one tool result (§49). `read` =>
/// explicit_read; `grep` content output => search_snippet per file. Name-only
/// discovery (find_file_by_name, files_with_matches) yields nothing (§50).
fn emit_source_observed(
    em: &mut Emitter,
    ts: &str,
    tool_name: &str,
    args: &Value,
    tool_call_id: &str,
    content: &str,
) {
    let root = em.ctx.repo_root.as_deref();
    match tool_name {
        "read" | "read_file" => {
            if let Some((path, sl, el)) = parse_file_view(content) {
                // §19: external/non-repository paths get `path: None` — never
                // fabricate repository membership.
                let rp = to_repo_path(&path, root);
                em.push(
                    ts,
                    "source_observed",
                    serde_json::to_value(SourceObserved {
                        path: rp,
                        observation_kind: ObservationKind::ExplicitRead,
                        tool_call_id: Some(tool_call_id.to_string()),
                        bytes: Some(content.len() as u64),
                        line_start: sl,
                        line_end: el,
                        content_digest: Some(digest::sha256_text(content)),
                    })
                    .unwrap_or_default(),
                );
            }
        }
        "grep" | "search" => {
            // Only content mode delivers source text to the model.
            if args.get("output_mode").and_then(Value::as_str) != Some("content") {
                return;
            }
            // `-- K matches in PATH` headers then ` LINE|`/`-` context entries.
            let mut per_file: Vec<(String, u64, u64, u64)> = Vec::new(); // path,bytes,min,max
            let mut cur: Option<usize> = None;
            for line in content.lines() {
                if let Some(rest) = line.strip_prefix("--") {
                    if let Some(idx) = rest.find(" matches in ") {
                        let path = rest[idx + " matches in ".len()..].trim().to_string();
                        per_file.push((path, 0, u64::MAX, 0));
                        cur = Some(per_file.len() - 1);
                        continue;
                    }
                }
                if let Some(i) = cur {
                    // `   12|text` or `   12-text` context/match lines.
                    let t = line.trim_start();
                    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() && t[digits.len()..].starts_with(['|', '-']) {
                        if let Ok(ln) = digits.parse::<u64>() {
                            let e = &mut per_file[i];
                            e.1 += line.len() as u64;
                            if ln < e.2 {
                                e.2 = ln;
                            }
                            if ln > e.3 {
                                e.3 = ln;
                            }
                        }
                    }
                }
            }
            for (path, bytes, mn, mx) in per_file {
                if bytes == 0 {
                    continue;
                }
                // §19: external or unparseable/truncated paths -> path: None.
                let rp = to_repo_path(&path, root);
                em.push(
                    ts,
                    "source_observed",
                    serde_json::to_value(SourceObserved {
                        path: rp,
                        observation_kind: ObservationKind::SearchSnippet,
                        tool_call_id: Some(tool_call_id.to_string()),
                        bytes: Some(bytes),
                        line_start: (mn != u64::MAX).then_some(mn),
                        line_end: (mx != 0).then_some(mx),
                        content_digest: None,
                    })
                    .unwrap_or_default(),
                );
            }
        }
        _ => {}
    }
}

/// Convert one parsed ATIF trajectory into canonical events.
pub fn from_atif(
    trajectory: &Value,
    ctx: &AtifContext,
) -> Result<Vec<AgentEvent>, AgentEventError> {
    let session_id = trajectory
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AgentEventError::Adapter {
            reason: "missing session_id".into(),
        })?;
    let steps = trajectory
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| AgentEventError::Adapter {
            reason: "missing steps".into(),
        })?;
    let mut em = Emitter {
        session_id,
        ctx,
        seq: 0,
        out: Vec::new(),
    };
    let first_ts = steps
        .first()
        .and_then(|s| s.get("timestamp"))
        .and_then(Value::as_str)
        .unwrap_or("1970-01-01T00:00:00Z");
    let last_ts = steps
        .last()
        .and_then(|s| s.get("timestamp"))
        .and_then(Value::as_str)
        .unwrap_or(first_ts);
    let model = steps
        .iter()
        .find_map(|s| s.get("model_name").and_then(Value::as_str))
        .map(|s| s.to_string());

    // session_started
    em.push(
        first_ts,
        "session_started",
        serde_json::to_value(SessionStarted {
            model: model.clone(),
            reasoning_level: None,
            repository: ctx.repository_id.clone(),
            repo_head: ctx.repo_head.clone(),
            task: ctx.investigation_id.clone(),
        })
        .unwrap_or_default(),
    );

    // user task message (first user step) -> agent_message
    for s in steps {
        if s.get("source").and_then(Value::as_str) == Some("user") {
            if let Some(msg) = s.get("message").and_then(Value::as_str) {
                let ts = s
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .unwrap_or(first_ts);
                em.push(
                    ts,
                    "agent_message",
                    serde_json::to_value(AgentMessage {
                        role: "user".into(),
                        content: Some(msg.to_string()),
                        content_bytes: Some(msg.len() as u64),
                        content_digest: Some(digest::sha256_text(msg)),
                    })
                    .unwrap_or_default(),
                );
            }
            break;
        }
    }

    let mut final_answer: Option<(&str, &str)> = None; // (ts, text)
    for s in steps {
        let ts = s
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or(first_ts);
        let is_agent = s.get("source").and_then(Value::as_str) == Some("agent");
        let step_id = s.get("step_id").and_then(Value::as_u64).unwrap_or(0);
        // model call events (only when ATIF exposes usage => a real call).
        if is_agent && s.get("metrics").is_some() {
            let mc_id = format!("mc-{step_id}");
            em.push(
                ts,
                "model_call_started",
                serde_json::to_value(ModelCallStarted {
                    model_call_id: mc_id.clone(),
                    model: model.clone(),
                })
                .unwrap_or_default(),
            );
            let m = &s["metrics"];
            em.push(
                ts,
                "model_call_completed",
                serde_json::to_value(ModelCallCompleted {
                    model_call_id: mc_id,
                    model: s
                        .get("model_name")
                        .and_then(Value::as_str)
                        .map(|x| x.to_string())
                        .or_else(|| model.clone()),
                    input_tokens: m.get("prompt_tokens").and_then(Value::as_u64),
                    output_tokens: m.get("completion_tokens").and_then(Value::as_u64),
                    cached_input_tokens: m.get("cached_tokens").and_then(Value::as_u64),
                    cached_output_tokens: None,
                    reasoning_tokens: None,
                    duration_ms: None,
                    status: Some("ok".into()),
                    usage_estimated: false,
                })
                .unwrap_or_default(),
            );
        }
        // tool calls -> started; matched observations -> completed + observed.
        let obs = s.get("observation").and_then(|o| o.get("results"));
        for tc in s
            .get("tool_calls")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let tid = tc.get("tool_call_id").and_then(Value::as_str).unwrap_or("");
            let name = tc
                .get("function_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let args = tc.get("arguments").cloned().unwrap_or(Value::Null);
            em.push(
                ts,
                "tool_call_started",
                serde_json::to_value(ToolCallStarted {
                    tool_call_id: tid.to_string(),
                    tool_name: name.to_string(),
                    category: tool_category(name),
                    arguments: Some(args.clone()),
                })
                .unwrap_or_default(),
            );
            // matching observation result by source_call_id
            if let Some(results) = obs.and_then(Value::as_array) {
                for r in results {
                    if r.get("source_call_id").and_then(Value::as_str) == Some(tid) {
                        let content = r.get("content").and_then(Value::as_str).unwrap_or("");
                        em.push(
                            ts,
                            "tool_call_completed",
                            serde_json::to_value(ToolCallCompleted {
                                tool_call_id: tid.to_string(),
                                ok: true,
                                duration_ms: None,
                                output_bytes: Some(content.len() as u64),
                                output_digest: Some(digest::sha256_text(content)),
                                output_ref: None,
                                status: Some("ok".into()),
                            })
                            .unwrap_or_default(),
                        );
                        emit_source_observed(&mut em, ts, name, &args, tid, content);
                    }
                }
            }
        }
        // candidate final answer = last non-empty agent message
        if is_agent {
            if let Some(msg) = s.get("message").and_then(Value::as_str) {
                if !msg.trim().is_empty() {
                    final_answer = Some((ts, msg));
                }
            }
        }
    }

    if let Some((ts, text)) = final_answer {
        em.push(
            ts,
            "final_answer",
            serde_json::to_value(FinalAnswer {
                content: Some(text.to_string()),
                content_ref: None,
                content_bytes: Some(text.len() as u64),
                content_digest: Some(digest::sha256_text(text)),
            })
            .unwrap_or_default(),
        );
    }

    em.push(
        last_ts,
        "session_completed",
        serde_json::to_value(SessionCompleted {
            reason: Some("runtime_ended".into()),
            duration_ms: None,
            total_steps: trajectory
                .get("final_metrics")
                .and_then(|f| f.get("total_steps"))
                .and_then(Value::as_u64),
        })
        .unwrap_or_default(),
    );

    Ok(em.out)
}
