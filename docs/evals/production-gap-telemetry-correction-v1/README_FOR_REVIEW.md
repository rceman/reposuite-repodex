# Production Gap Telemetry Correction V1 — review map

- `DEFECT_A_CANDIDATE_REASON.md` — why `no_candidate:<reason>` was dropped + the structured fix.
- `DEFECT_B_OPERATION_CLASSIFICATION.md` — why shell discovery was misclassified + `repository_operation`.
- `STRUCTURED_DISPOSITION_CONTRACT.md` / `TOOL_OPERATION_CONTRACT.md` / `FALLBACK_ASSOCIATION_CONTRACT.md` — corrected contracts.
- `UPDATED_PRODUCTION_EMITTER_HANDOFF.md` — Relay handoff incl. `repository_operation`.
- `PRODUCTION_PATH_GAP_TESTS.json` — real-query generated reason codes.
- `SYNTHETIC_OPERATION_TESTS.json` — 18-session production-path stream.
- `REPLAY_RESULTS.json` — corrected 929-session replay.
- `COUNT_RECONCILIATION.json` — reconciled totals + overlap explanation.
- `fixtures/observed-stream.jsonl` — the production-path stream itself.
