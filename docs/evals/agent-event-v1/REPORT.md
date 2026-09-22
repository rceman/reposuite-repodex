# AgentEvent v1 + Telemetry Ingest — Report

```text
AGENT_EVENT_TELEMETRY_FOUNDATION_COMPLETE
INVESTIGATION_EVENT_PIPELINE_READY
```

Base `dc97a96` → impl `ae72fe4`, eval `20e425e`. No ranking/temporal/Jev/System
One changes; no Agent benchmark rerun; only trace replay.

## Acceptance invariants (§90)

```text
AGENT_EVENT_SCHEMA_V1_VALID      = true
ATIF_ADAPTER_DEVIN_READY         = true
INGEST_IDEMPOTENCY_PASS          = true
CONFLICTING_DUPLICATE_REJECTED   = true
STORE_VERIFY_PASS                = true
QUERY_RANKING_UNCHANGED          = true
TEMPORAL_RANKING_UNCHANGED       = true
SYSTEM_ONE_BEHAVIOR_UNCHANGED    = true
```

## Real replay (§91) — all 240 traces

```text
ATIF_SESSIONS_EXPECTED           = 240
ATIF_SESSIONS_REPLAYED           = 240   (canonical events: 31,814)
PROMPT_TOKEN_MISMATCH_SESSIONS   = 0
COMPLETION_TOKEN_MISMATCH_SESSIONS = 0
TOOL_CALL_COUNT_MISMATCH_SESSIONS  = 0
sequence_anomaly_sessions        = 0
duplicate_id_sessions            = 0
missing_final_answer_sessions    = 0
total_input_tokens               = 92,450,895
total_output_tokens              = 939,108
tools: read=3271 grep=2257 find_file_by_name=606 (cat: file_read 3271, search 2863)
```

## SourceObserved (§92)

Manual inspection of **16** sessions (E0–E3 × L1–L4):

```text
FALSE_SOURCE_OBSERVED = 0
MISSED_SOURCE_OBSERVED = 1
```

The single "miss" is unavoidable and correct-by-contract: a tool-**truncated**
path (`/tmp/gt…`) plus an external non-repo wiki read — both are left `path`
absent per §19 rather than fabricating repository membership. Prior benchmark
`unique_files_observed` (E0 193.8 / E1 75.4 / E2 76.0 / E3 87.3) counted
name-only file discovery too; canonical `source_observed` (E0 56.3 / E1 35.6 /
E2 32.8 / E3 34.7) counts only content actually delivered to the model — a
documented, more precise semantics (§50/§56).

## Performance / storage (§93)

```text
single-process ingest   ≈ 38,300 events/sec (31,814 ev in 0.83 s)
per-file CLI ingest     240 files in 20.4 s (process-spawn bound)
peak RSS                172 MB — dominated by CLI buffering the input file;
                        the store is per-session bounded, never full-load
store bytes/event       944 B
raw ATIF bytes          71.08 MB   canonical store 30.04 MB
duplication ratio       0.42 — canonical is SMALLER than raw (drops full
                        tool output; keeps digest+bytes+source_observed)
query: stats 172 ms / verify 176 ms / export 273 ms over 31.8k events
```

## §96 answers

1. **Harness-neutral?** Yes — envelope/semantics are runtime-agnostic; `runtime`
   is provenance metadata consumers don't branch on. Relay can emit it natively.
2. **ATIF→token/tool lossless?** Yes — per-step `metrics` map 1:1 to
   `model_call_completed` and sum exactly to `final_metrics` (0 mismatches).
3. **All sessions replayable?** Yes — 240/240.
4. **Canonical token totals = ATIF?** Yes, exactly (per-session and aggregate).
5. **Tool-call totals match?** Yes — 0 mismatch sessions.
6. **SourceObserved reliable?** Yes — 0 false positives over the 16-session
   sample; only unavoidable gap is tool-truncated/external paths (path absent).
7. **Storage added?** 944 B/event; ~30 MB for the full 240-run corpus.
8. **Duplicates raw tool output?** No — digest+bytes only; store is 42% of raw.
9. **Ingest throughput?** ~38k events/sec single-process.
10. **Peak RAM?** 172 MB, dominated by input-file buffering; store is
    per-session bounded (no full-history load).
11. **Relay can emit natively?** Yes — contract is transport-neutral; ATIF
    adapter becomes legacy-import/replay tooling.
12. **Fields for InvestigationEpisode?** `session_id`, `investigation_id`,
    `repository_id`, `repo_head`, `sequence`, `timestamp`, tool-call links,
    `source_observed` paths/bytes/lines, `final_answer` content/digest,
    `model_call_completed` usage, `task_outcome`, `context_artifact` (future).
13. **Runtime-specific, kept out?** `reasoning_content`, `telemetry`/
    `generation_model` extras, ATIF `schema_version`, `cache_creation_input_
    tokens`, system-prompt steps, full tool bodies, native tool-call args beyond
    the redacted `arguments` field.
14. **Ready for next layer?** Yes — canonical neutral evidence is persisted and
    replayable; `InvestigationEpisode`/`AgentActivity`/`SourceExposure` can be
    derived without re-crawling harness logs.

## Artifacts

`docs/evals/agent-event-v1/`: BENCHMARK_SPEC, ATIF_REPLAY, PERFORMANCE,
SUMMARY, REPORT. Canonical schema `schemas/reposuite-agent-event-v1.schema.json`;
examples `schemas/examples/`. Spec `docs/AGENT_EVENT_V1.md`, adapter map
`docs/AGENT_EVENT_DEVIN_ATIF.md`. Large replay store outside Git:
`/tmp/repodex-agent-event-v1/store`.

```text
SOURCE_PROMPT_ID: REPODEX-AGENT-EVENT-TELEMETRY-FOUNDATION-V1
PREVIOUS_PROMPT_ID: REPODEX-GIT-TEMPORAL-ACTIVITY-FOUNDATION-V1
REPORT_DATE_TIME: 2026-09-22 15:38:05 Europe/Riga
```
