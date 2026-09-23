# InvestigationEpisode + SourceExposure + AgentActivity v1

Derived intelligence layer on top of canonical `AgentEvent v1`. Rebuildable;
the raw Agent Event Store stays authoritative. **Observational only** — no
usefulness/ranking scores (those belong to a future Investigation Memory).

```text
Agent Event Store -> Episode Aggregator
   -> InvestigationEpisode   (per investigation_id)
   -> SourceExposure         (per investigation+path)
   -> AgentActivity index    (per repository_id+path)
```

## Layers (§3)

- `AgentEvent` — raw canonical observation (frozen, `reposuite.agent-event.v1`).
- `InvestigationEpisode` — derived summary of ONE logical investigation.
- `SourceExposure` — source content actually delivered to the Agent.
- `AgentActivity` — accumulated usage stats over repo paths.
- `InvestigationMemory` — future learned prior. NOT built here.

## Derivation pipeline

```text
for each session (append-only):
    SessionPart = fold(session events)          # bounded per session
    fold NEW tail events -> AgentActivity       # add-only incremental
    fold session final-answer path/evidence mentions -> AgentActivity (idempotent)
for each investigation with a changed session:
    InvestigationEpisode = merge(session parts)  # only that investigation
```

`SessionPart` is persisted per session (`sessions/<sid>.part.json`) so an
investigation is rebuilt from bounded parts, not a global rescan.

## Schemas

### InvestigationEpisode (`reposuite.investigation-episode.v1`)
`investigation_id`, `session_ids[]`, `project_id`, `repository_id`,
`repo_heads[]`, `started_at`, `completed_at`, `state`
(`open|completed|failed|partial`), `query_text`, `normalized_query`,
`query_digest`, `model_calls`, `input/output/cached_input/cached_output/
reasoning_tokens`, `usage_is_actual`, `per_model`, `tools{...}`,
`source_exposures[]`, `unique_source_files_*`, `source_observation_events`,
`source_exposure_bytes_total`, `final_answers[]`, `final_answer_path_mentions[]`
(joined to observed/read), `evidence_path_mentions[]` (structured EVIDENCE),
`unresolved_path_mentions[]`, `session_outcomes[]`, `task_outcomes[]`,
`quality_outcome` (reserved, absent).

### PathExposure (SourceExposure, §11-§14)
`path`, first/last `observed_sequence`/`observed_at`,
`observation_event_count`, per-kind counts (`explicit_read`, `search_snippet`,
`symbol_preview`, `diff`, `other`), `observed_bytes_total`, merged bounded
`line_ranges`.

### PathActivity (AgentActivity, §23-§29)
`repository_id`, `path`, lifetime counters (`observation_event_count_total`,
per-kind, `exposed_bytes_total`), distinct `session_ids`/`investigation_ids`
(counts derived), `mention_session_ids`, `structured_evidence_session_ids`,
first/last observed, bounded `daily` buckets (day→{observations,
explicit_reads, investigation_ids}).

## Semantics

- **Query** (§6-§7): `query_text` = first user-role `agent_message` (the task);
  `session_started.task` is only a fallback label. `normalized_query` =
  whitespace+case normalize (deterministic; no embeddings/similarity).
- **Tokens** (§9): episode totals = sum of authoritative per-call actuals; no
  session-summary double-counting; `usage_is_actual=false` if any call was
  estimated.
- **SourceObserved→exposure** (§11): only `source_observed` events count —
  never path-arguments, filename discovery, or RDX1-packet paths (§50).
- **Mentions** (§17-§21): `final_answer_path_mentions` = canonical repo paths
  exactly matched against `known_paths` (git ls-files at build time);
  `EVIDENCE:`-block paths additionally go to `evidence_path_mentions`
  (stricter). `unresolved_path_mentions` = path-like strings not resolving.
  Mentions ≠ citations; no usefulness implied (§20).
- **State** (§5,§22): `session_completed` (runtime ended), `task_outcome`
  (external), `quality_outcome` (future evaluator) stay distinct — never one
  boolean.

## Incremental + checkpoint (§30-§34)

`checkpoint.json` = `{derived_schema_version, event_store_digest,
session_processed{sid:count}}`. Update folds only the tail beyond each
session's checkpoint count (sessions append-only). Late events
(`task_outcome` after `session_completed`) extend a session's tail → reprocess
+ rebuild just that investigation. New session for an existing
`investigation_id` → rebuild only that investigation.

## Rebuild (§34-§35)

`agent-events derive --full` rebuilds all derived state from raw events;
deterministic given identical store + schema version.

## CLI

```text
agent-events derive --store <event-store> --output <derived> [--full] [--repo-root <repo>]
agent-events derive-verify --output <derived>
investigations show|list|stats
agent-activity file <path>|stats
```

## Anti-patterns guaranteed absent (§71)

No full raw-event-store load for a normal update (per-session tail only); no
raw scan for an activity lookup (persisted index); no ATIF parsing at lookup;
no repo source scan / git at lookup; no ranking or System One changes.

## Future boundary (§63-§67)

The layer already exposes the factual evidence a memory system needs —
surfaced/observed/read/mentioned, cost (tokens/tools), outcome, staleness
provenance (`repo_head`, path, content digest). A future memory MAY distinguish
`surfaced`/`observed`/`read`/`evidence-mentioned`/`succeeded`; ignored results
are NOT negative evidence (§65); strong negatives need explicit evidence
(§66); staleness via `repo_head`+path+digest (§67). None of that is
implemented here. `GitActivity` and `AgentActivity` stay separate (§68).
