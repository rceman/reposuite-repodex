SOURCE_PROMPT_ID: REPODEX-PRODUCTION-GAP-TELEMETRY-CORRECTION-V1-20261004-125200
PREVIOUS_PROMPT_ID: REPODEX-PRODUCTION-GAP-TELEMETRY-V1-20261004-000200
REPORT_DATE_TIME: 2026-10-04 14:20:00 Europe/Riga

# Production Gap Telemetry Correction V1 — Final Report

## Defect A answers (§47)

1. `GraphNode.disposition` fused family+reason into one string
   (`no_candidate:<reason>`); the gap extractor used exact-match
   `matches!(d, "no_candidate"|"out_of_scope")` — every reasoned
   disposition was dropped.
2. Family now lives in `GraphNode.disposition_kind` +
   `SeedOut/RelOut.disposition_kind` (structured).
3. Reason now lives in `disposition_reason` at both levels.
4. RDX inspection: **no** — the fix reads structured node/projection fields;
   the legacy colon-split exists only for pre-schema-3 graphs (our own
   producer vocabulary, not Agent text).
5. `receiver_type_unavailable` reaches `reason_code`: YES — real
   `callers method run` on phpnav (test + production-path stream).
6. `late_static_binding`: YES — real `callers method save` on phpnav.
7. Unknown future codes pass through verbatim — never collapsed.
8. `GRAPH_SCHEMA_VERSION` 2->3. EvidenceProjection and observation schemas:
   additive fields only — no version bump (old projections deserialize;
   observation JSON unchanged in shape).
9. Reason for the bump: nodes gained serialized fields; old graph artifacts
   must not silently satisfy the new schema — version mismatch forces a
   deterministic rebuild.

## Defect B answers (§48)

1. Shell discovery was missed because `Shell` was hard-classified as task
   action — a real `rg`/`find` ended windows as `task_action_started`.
2. `OtherRepositoryTool` was lumped into the same stopper arm.
3. New field: `ToolCallStarted.repository_operation` — producer-normalized
   bounded vocabulary (`discovery_search`, `discovery_list`, `source_read`,
   `edit`, `build`, `test`, `runtime`, `git_inspection`, `other`).
4. Optional + additive — AgentEvent v1, no v2; absent => conservative legacy.
5. In production the EMITTER (Relay/runtime adapter) produces it — it saw
   the action.
6. Classification: search/directory_list -> discovery; file_read/
   source_read -> verification; shell with op=discovery_search -> discovery;
   shell op=test/build/edit/runtime -> task action; generic repo tool
   op=discovery_search -> discovery; legacy ambiguous shell ->
   `unclassified_repository_activity`.
7. Unclassified is distinguishable — it is a distinct disposition, and does
   not end the window.
8. Producer-correlated calls stay excluded via `tool_call_id` regardless of
   op/category.

## Corrected replay counts (§49)

    artifact presentations          415
    with >=1 gap                     330
    with 0 gaps                       85
    native_discovery                  41
    verification_only                 66
    another_artifact                  60
    task_action                      237
    unclassified                       0
    no_followup                       11

330+85 = 415 (reconciles). by_gap_family sums exceed 330 because one
artifact can carry several families — buckets count family-bearing
presentations, not unique artifacts.

## Safety counts (§50)

    FALSE_REPODEX_ASSOCIATIONS            0
    RDX_TELEMETRY_ABLATION_DIFFERENCES    0 (7 probes incl. candidate queries)
    DUPLICATE_INGEST_REPORT_DELTA         0
    CROSS_SESSION_ASSOCIATIONS            0
    CROSS_REPOSITORY_ASSOCIATIONS         0
    TELEMETRY_CAUSED_QUERY_FAILURES       0

## Relay readiness gate (§51)

All eight conditions verified by tests + production-path stream:
real no_candidate reason ✓, real out_of_scope reason ✓, shell discovery via
op ✓, shell test/build/edit -> task action ✓, legacy ambiguous shell ->
unclassified (not false task action) ✓, counts reconcile ✓, RDX identical ✓,
old v1 streams valid ✓.

## Verdicts

REPODEX_PRODUCTION_GAP_TELEMETRY_READY_FOR_RELAY_INTEGRATION

RELAY_PRODUCTION_TELEMETRY_INTEGRATION_NEXT

## Exactly one next action (§54)

Separate RepoSuite Relay implementation milestone: emit canonical AgentEvent
v1 from real Agent sessions — tool_call_started/completed (+ repository_operation
where observable), source_observed, context_artifact_presented carrying
RepoQueryObservation metadata. Not implemented here.

## Final status

REPODEX_PRODUCTION_GAP_TELEMETRY_CORRECTION_V1_COMPLETE
