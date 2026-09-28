# Harness Final Correction V2

- tc-ver gold corrected: go.mod has NO go directive -> absent_claim obligation;
  'no version declared' now scores TASK_SUCCESS.
- Status split: execution_status (EXECUTION_VALID/INFRA_INVALID/...) vs
  validation_status (TASK_SUCCESS/FAILURE/NOT_EVALUATED); the ambiguous
  'VALID_SUCCESS + correct=false' is impossible.
- Harness committed to tools/eval/ (runner, tracer, validator, metrics, gold,
  classify) — reproducible, not /tmp.
- Code tasks validate the actual worktree: marker predicate + diff +
  validator-owned build/test exit code; agent prose can't fake success.
- exec classified by content (repodex vs search vs build); exit code distinct
  from transport status.
- repodex_prepare_wall + agent_wall + combined_wall recorded.
- Gold source-proved (GOLD_SOURCE_PROOFS.json, AUDIT_PASS).
- Validator tests: negative, equivalent-correct, unchanged-worktree, status
  model, classify, reproducible — all pass.
- Smoke: 32 sessions, all EXECUTION_VALID, 0 INFRA_INVALID.

Verdicts: REPODEX_BENCHMARK_INSTRUMENT_TRUSTWORTHY ·
READY_FOR_REPODEX_FIRST_EVIDENCE_NAVIGATION_V1
