# Devin ATIF → AgentEvent v1 mapping

All ATIF-specific parsing lives in `src/agent_event/devin_atif.rs` (§43).
Outside it, no code references native ATIF names (`prompt_tokens`,
`source_call_id`, `<file-view>`, ...).

## Source

`devin -p --export` produces one ATIF trajectory JSON:

```text
{ schema_version, session_id, agent,
  steps: [ { step_id, timestamp, source(system|user|agent), message,
             model_name, reasoning_content,
             tool_calls: [{tool_call_id, function_name, arguments}],
             observation: {results: [{source_call_id, content}]},
             metrics: {prompt_tokens, completion_tokens, cached_tokens,
                       extra:{cache_creation_input_tokens}},
             extra:{generation_model, telemetry} } ],
  final_metrics: {total_prompt_tokens, total_completion_tokens,
                  total_cached_tokens, total_steps} }
```

## Identity / ordering

- `session_id` → canonical `session_id` (also `source.native_session_id`, §44).
- benchmark run id → `investigation_id` (§45), not inferred from free text.
- `repo_root`/`repository_id`/`repo_head` supplied via adapter `AtifContext`.
- `sequence` = a running counter over emitted events (a step emits several);
  `timestamp` = the step timestamp.

## Step → events

| ATIF | AgentEvent |
|------|-----------|
| first step | `session_started` (model, repository, repo_head, task) |
| first `source=user` message | `agent_message` role=user (the task) |
| `source=agent` step with `metrics` | `model_call_started` + `model_call_completed` (model_call_id=`mc-{step_id}`) |
| `tool_calls[i]` | `tool_call_started` (tool_name=`function_name`, category, arguments) |
| `observation.results[source_call_id=tid]` | `tool_call_completed` (output_bytes=len(content), output_digest=sha256(content)) |
| `read` output `<file-view path start_line end_line>` | `source_observed` `explicit_read` |
| `grep` `output_mode=content` `-- K matches in PATH` | `source_observed` `search_snippet` per file |
| last non-empty agent `message` | `final_answer` |
| last step | `session_completed` (reason=runtime_ended, total_steps) |

## Token policy (§46)

Each `source=agent` step's `metrics` is one model call: `input_tokens=
prompt_tokens`, `output_tokens=completion_tokens`, `cached_input_tokens=
cached_tokens`. These are ACTUAL provider values (`usage_estimated=false`) and
sum exactly to `final_metrics` (verified 0 mismatches across all runs).
`cache_creation_input_tokens` is intentionally NOT mapped (a different
quantity, not in canonical fields).

## Tool categories (§48)

`grep`, `find_file_by_name` → `search` (find_file_by_name = file discovery,
documented); `read` → `file_read`; `exec`/`bash` → `shell`; `git*` →
`git_inspection`; `web_*`/`mcp_*`/`run_subagent` → `non_repository_tool`;
`write`/`edit` → `other_repository_tool`. Native names retained.

## SourceObserved derivation (§49-§51)

- `read`: `path`+line range from `<file-view>`; `bytes` = content bytes.
- `grep` content: one `search_snippet` per `K matches in PATH` header;
  `bytes` = that file's delivered line bytes; line range = min/max line nums.
- `find_file_by_name` / `files_with_matches`: name-only → NOT emitted (§50).
- Path normalization: `/tmp/<root>/x` → `x` (repo-relative). External or
  unparseable/truncated paths → `path` absent (§19). A tool-truncated path
  (`/tmp/gt…`) is left absent — it cannot be recovered.

## Not carried into canonical events

`reasoning_content` (model-internal), `telemetry`/`generation_model` extras,
`schema_version`, full tool output text (kept as digest+bytes instead, §33),
`cache_creation_input_tokens`, step `message` bodies of non-final agent steps
(kept via `final_answer`), system-prompt steps.
