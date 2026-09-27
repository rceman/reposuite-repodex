# Benchmark Harness & Trace Correction — report

## Defects fixed
- **Tool rejections**: scored sessions used default `--permission-mode auto`
  which auto-approves only read-only tools -> edits rejected. Fixed:
  `--permission-mode dangerous` scoped to isolated benchmark worktrees ->
  0 rejections/cancellations across all 32 smoke sessions.
- **Truncated traces**: full-fidelity trace preserves every observable event
  (full tool results, untruncated messages, per-call token metrics incl.
  cached, timestamps, execution status).
- **Model-call counting**: model_requests counted from MODEL_TOKENS events,
  not total_steps.
- **repodex_calls miscount**: counted from actual repodex exec args / repo_query,
  not any exec.
- **Validation**: obligation-based contract; code tasks validated from the
  actual disposable worktree (edit request != success; rejected edit =
  INFRA_INVALID).
- **Session status model**: VALID_SUCCESS / VALID_FAILURE / INFRA_INVALID /
  MODEL_PROVIDER_FAILURE / TIMEOUT.

## Smoke results (32 sessions)
All 32 VALID_SUCCESS, 0 INFRA_INVALID. Code edits executed + validated.

## Verdicts
- REPODEX_BENCHMARK_HARNESS_VALIDATED
- FULL_FIDELITY_OBSERVABLE_TRACE_VALIDATED
- READY_FOR_REPODEX_FIRST_EVIDENCE_NAVIGATION_V1
