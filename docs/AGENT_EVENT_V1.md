# AgentEvent v1 — canonical RepoSuite telemetry contract

```text
schema: reposuite.agent-event.v1
```

`AgentEvent v1` is a **RepoSuite-level** contract (§2). It is not a
`DevinEvent`/`RepoDexEvent`/`CodexEvent`. RepoDex currently hosts the
schema/library/CLI because it is the active implementation repository, but the
contract is designed to move to / be emitted by **RepoSuite Relay** without
semantic redesign.

```text
Devin / Codex / OpenCode / ACP / future
        -> adapter OR RepoSuite Relay
        -> AgentEvent v1
        -> Agent Telemetry Ingest
        -> Agent Event Store
        -> (later) InvestigationEpisode -> AgentActivity -> Investigation Memory
```

## Layers (§3)

- `AgentEvent` — the canonical unit of observation.
- `Agent Telemetry Ingest` — the ingress boundary (`AgentEventSink`).
- `Agent Event Store` — durable append-oriented raw storage.
- `InvestigationEpisode` — a FUTURE derived aggregate. Not built here.

## Fundamental evidence rule (§4)

The ingest layer records **observations**. It never decides useful /
irrelevant / correct / wrong / good / bad. Those are later derived
interpretations. The raw store preserves neutral evidence so future algorithms
can be recomputed.

## Envelope (§5-§9)

```text
schema            = "reposuite.agent-event.v1"   (const, else reject)
event_id          = "{session_id}:{sequence}"    (idempotency authority)
session_id        = one concrete agent/runtime session
investigation_id? = logical task/investigation (NOT a session synonym)
project_id?
repository_id?    = stable repository identity
repo_head?        = Git commit investigated (memory staleness anchor)
sequence          = monotonic order within session (primary ordering)
timestamp         = RFC3339/ISO-8601 UTC (wall-time, not ordering)
source            = {runtime, adapter, adapter_version, native_session_id?}
type              = event family (known OR future/unknown)
data              = typed payload (opaque for unknown types)
content_digest?   = sha256 of canonical content (conflict detection)
```

`session_id` and `investigation_id` are distinct: one investigation may span
multiple sessions (review/rework). `repo_head` is Git history, never filesystem
mtime.

## Event families (§10, §12-§23)

| type | payload essentials |
|------|--------------------|
| `session_started` | model, reasoning_level, repository, repo_head, task |
| `model_call_started` | model_call_id, model |
| `model_call_completed` | model_call_id, model, input/output/cached_input/cached_output/reasoning_tokens, duration_ms, status, `usage_estimated` |
| `tool_call_started` | tool_call_id, tool_name, category, arguments |
| `tool_call_completed` | tool_call_id, ok, duration_ms, output_bytes, output_digest, output_ref, status |
| `source_observed` | path?, observation_kind, tool_call_id, bytes, line_start/end, content_digest |
| `agent_message` | role, content, content_bytes, content_digest |
| `final_answer` | content/content_ref, content_bytes, content_digest |
| `task_outcome` | outcome, detail |
| `session_completed` | reason, duration_ms, total_steps |

Three outcomes stay distinct (§23): `session_completed` = runtime ended;
`task_outcome` = external lifecycle/product outcome; a future evaluator score =
correctness/quality. Never collapsed to one boolean.

## Token semantics (§14, §46)

`model_call_completed` carries **actual** provider/harness usage. Fields are
optional individually. Estimated/proxy usage must set `usage_estimated=true` —
never placed in an "actual" field. One authoritative aggregation policy: a
session's totals are the sum of its `model_call_completed` usage, and they must
reproduce the source trajectory's authoritative totals exactly.

## Tool categories (§15, §48)

`tool_name` is the native runtime name (retained verbatim); `category` is the
harness-neutral bucket: `search`, `file_read`, `directory_list`,
`git_inspection`, `shell`, `other_repository_tool`, `non_repository_tool`.
Core schema does NOT bake specific native tool names into enums.

## SourceObserved (§17-§19, §50-§51)

`source_observed` means repository **source content was actually delivered to
the model** through a tool result. Kinds: `explicit_read`, `search_snippet`,
`symbol_preview`, `diff`, `other` (extensible).

- NOT emitted for a path that appeared only as a search *argument*.
- NOT emitted for file-name-only discovery (`find_file_by_name`,
  `files_with_matches`) — names are not content.
- `path` is a canonical repository-relative normalized path. External /
  non-repository / unparseable paths leave `path` absent — never fabricated.
- `bytes` = bytes of the source fragment delivered, not whole-file size.

## Ordering & idempotency (§7, §29, §30)

`sequence` is the primary ordering authority (monotonic within a session);
`timestamp` is for wall-time only. `event_id` is the idempotency authority:
replaying the same canonical batch yields `duplicates`, never double-writes.
The same `event_id` with different semantic content (per `content_digest`) is a
**conflict** — rejected explicitly, not silently deduped.

## Forward compatibility (§11)

`type`+`data` are loosely typed so an **unknown future event type round-trips
with opaque data** — it neither corrupts nor invalidates the stream. A schema
identifier other than `reposuite.agent-event.v1` (a genuinely incompatible
version) fails explicitly. Late events may arrive after `session_completed`
(e.g. `task_outcome` attached by `investigation_id`/`session_id`).

## Storage semantics (§31-§34, §71)

`<store>/manifest.json` + `sessions/<safe_sid>.jsonl`, append-oriented.
Idempotency is per-session — dedup loads only the affected session's events, so
memory is bounded by one session, never the whole store. `verify` checks
schema, duplicate-conflict, sequence monotonicity, digest, identity.
`content_digest`/`output_digest` + `output_ref` keep large payloads out of the
canonical store (no full tool-output duplication).

## Privacy (§24)

Canonical events do not knowingly persist auth headers, API keys, passwords, or
secret env values (e.g. the temporary Jev key). Adapters select/redact fields
rather than dump environments.

## Serialization (§25-§26)

Canonical JSON; JSONL for streams. `serde_json` object maps are `BTreeMap`
(sorted keys) so digests are deterministic; no nondeterministic map order.

## Ingest boundary (§27-§28)

```rust
trait AgentEventSink { fn ingest_batch(&mut self, e:&[AgentEvent])
    -> Result<IngestReport, AgentEventError>; }
IngestReport { accepted, duplicates, rejected, errors }
```

## CLI (§36-§39)

```text
reposuite-repodex agent-events ingest --format reposuite-v1 events.jsonl
reposuite-repodex agent-events ingest --format devin-atif trajectory.json
reposuite-repodex agent-events verify|stats|session <id>|export
```

`ingest` reads a file or stdin (`-`). Transport-neutral — no server required.

## Future Relay (§40, §78-§79, §86)

RepoSuite Relay SHOULD emit `AgentEvent v1` natively; then the Devin ATIF
adapter becomes legacy-import/replay tooling while RepoDex consumers are
unchanged. A future batch payload is `{"events":[...]}` over HTTP/Unix/IPC/
in-process — the contract is transport-neutral. RepoDex-result provenance can
be attached via investigation/session metadata or a generic `context_artifact`
event (chosen over RepoDex-specific ToolCall fields). Outcome events
(`review accepted`, `tests passed`, `integrated`) attach late via
`investigation_id`/`session_id`.

## context_artifact_presented (GAP-TELEMETRY-V1)

Typed, producer-neutral event for evidence delivered to the Agent:

```json
{"artifact_id":"sha256:…","artifact_kind":"rdx1_packet","producer":"repodex",
 "tool_call_id":"tc-1","presentation":"adaptive",
 "content_digest":"sha256:…","content_bytes":720,"references":[],
 "metadata":{"query_intent":"callers","navigation_profile":"adaptive",
   "evidence_complete":true,
   "gap_signatures":[{"family":"no_candidate","reason_code":"late_static_binding"}],
   "seed_count":2,"candidate_count":1,"fact_count":4,"relation_count":5,
   "packet_bytes":720,"query_digest":"sha256:…"}}
```

- `metadata` is a bounded allowlist: `query_intent`, `navigation_profile`,
  `evidence_complete`, `gap_signatures`, `seed_count`, `candidate_count`,
  `fact_count`, `relation_count`, `packet_bytes`, `query_digest`. Secrets,
  auth headers, env values and raw query/task text are forbidden.
- Full artifact payloads are never embedded — digest/bytes only.
- Gap `reason_code` values are stable producer codes (e.g.
  `late_static_binding`), opaque to the core model — language-specific
  reasons are values, not fields.
