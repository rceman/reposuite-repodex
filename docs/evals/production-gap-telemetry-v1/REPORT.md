SOURCE_PROMPT_ID: REPODEX-PRODUCTION-GAP-TELEMETRY-V1-20261004-000200
PREVIOUS_PROMPT_ID: REPODEX-PHP-BOUNDED-INHERITANCE-DISPATCH-V1-20261003-225200
REPORT_DATE_TIME: 2026-10-04 13:00:00 Europe/Riga

# Production Gap Telemetry V1 — Final Report

## Architecture answers (§65)

1. `context_artifact_presented` — extended (tool_call_id, presentation,
   metadata) rather than replaced; old v1 streams stay valid.
2. Generic: harness-neutral RepoSuite-level event; RepoDex is one producer.
3. Correlation via `tool_call_id` -> `tool_call_started` (never timestamps).
4. Full RDX in event: **no** — digest/bytes/bounded metadata only.
5. Relay parses RDX: **no** — RepoDex emits `RepoQueryObservation` JSON
   (`--emit-observation`), Relay copies allowlisted keys.
6. Minimum handoff: packet + observation JSON (PRODUCTION_EMITTER_HANDOFF.md).
7. Relay query authority: **no** (contract extended, invariants preserved).
8. RepoDex task/worktree authority: **no**.
9. Telemetry changes query behavior: **no** — nothing in the query path reads
   telemetry; observation built from the same projection, write-only.
10. Memory updates: **no** — no write path from fallback analysis into
    memory/derived stores.
11. Agent-facing output changed: **no** — RDX_ABLATION 5/5 byte-identical.

## Gap answers (§66)

1. Families: no_result, ambiguous_result, result_limit, evidence_truncated,
   bounded_no_route, no_candidate, out_of_scope, unsupported_evidence_class,
   missing_anchor, ambiguous_anchor (reserved).
2. reason_code = producer's stable identifier head — bounded, never prose;
   PHP codes are opaque values, not fields.
3. `no_candidate` -> family `no_candidate`, reason_code = detailed reason head
   (e.g. `receiver_type_unavailable`).
4. `out_of_scope` -> family `out_of_scope`, code e.g. `late_static_binding`.
5. Ambiguity -> `ambiguous_result` (`no_confident_intent`); duplicate-decl
   ambiguity stays in packet evidence.
6. Bounded no-route -> `bounded_no_route`/`no_route`.
7. Truncation -> `evidence_truncated` (`truncated_continuation` /
   upstream reason head); RESULT_LIMIT -> `result_limit` (seed_bound /
   relationship_limit).
8. Unsupported evidence -> `unsupported_evidence_class`.
9. Unknown future reason codes pass through unchanged — never collapsed to
   `no_result`.

## Fallback answers (§67)

1. Window: from artifact presentation, forward <=64 events.
2. Broad native discovery: Search / DirectoryList / uncorrelated
   repository-tool calls (producer-correlated calls excluded).
3. Source verification: file_read tool calls + source_observed events.
4. Window stops: next context_artifact, edit/write/build/test/runtime action,
   session end, or bound.
5. Multiple queries: each artifact opens its own window; chained queries land
   `another_context_artifact`.
6. Sessions isolated: analyzed per-session-file, sequence-local only.
7. Repositories isolated: repository_id/repo_head carried, never merged.
8. Causal: **no** — association only; fields say "after", not "because".

## Replay answers (§68)

1. 929 sessions replayed (all committed eval suites, 11924 events).
2. 415 artifact presentations (all producer=repodex).
3. with gaps: 415 had >=1 signature family recorded where gaps existed
   (304 ambiguous_result, 156 evidence_truncated, 43 result_limit,
   32 no_result, 6 bounded_no_route — overlapping families).
4. native fallback associations: 40.
5. verification-only: 66. 6. no-followup: 11; another_artifact: 59;
   task_action: 239.
7. control sessions: 615 without artifacts.
8. false RepoDex associations: **0**.
9. duplicate-ingest delta: **0** (test: report identical after re-ingest).

## Scale answers (§69)

| sessions | events | bytes | ingest ms | analysis ms |
|---|---|---|---|---|
| 100 | 330 | 111 KiB | 11 | 27 |
| 1000 | 3300 | 1.1 MiB | 74 | 25 |
| 10000 | 33000 | 11.1 MiB | 868 | 175 |

O(sessions + events*window). Normal query never scans telemetry.

## Query safety answers (§70)

- RDX ablation differences: **0**
- direct/service/auto observation mismatches: 0 (one render path)
- telemetry-caused query failures: **0**
- median query overhead: unmeasurable (287 vs 296 ms/10q — noise)
- p95 observation bytes: 565 (target <=2048)

## Reopening gate (§71)

Preregistered in PRODUCTION_REOPENING_GATE.md: >=50 valid RepoDex-first
sessions, >=20 fallback associations, gap family in >=5 sessions AND
>=2 repositories or >=3 investigations, 0 instrumentation defects.
Until then PHP_STATIC_EXPANSION / JEV / CONTINUOUS_LEARNING stay frozen.

## Verdicts

REPODEX_PRODUCTION_GAP_TELEMETRY_READY_FOR_RELAY_INTEGRATION

RELAY_PRODUCTION_TELEMETRY_INTEGRATION_NEXT

## Exactly one next action (§74)

Separate RepoSuite Relay milestone: emit canonical AgentEvent v1 —
tool_call_started/completed, source_observed, context_artifact_presented —
from real Agent sessions, copying RepoQueryObservation metadata per the
handoff contract. Not implemented here; no Gateway changes needed — the
Gateway handoff is the observation JSON.

## Final status

REPODEX_PRODUCTION_GAP_TELEMETRY_V1_COMPLETE
