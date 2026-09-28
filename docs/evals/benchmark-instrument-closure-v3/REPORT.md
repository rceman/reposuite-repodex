# Instrument Closure V3 — report

## Fixes
- **No auto-pass gold**: every factual obligation carries a real proof type
  (SOURCE_PROVEN/ABSENCE_PROVEN/RELATION_PROVEN/DEPENDENCY_PROVEN/
  MUTATION_VALIDATED/EXPLICITLY_NONFACTUAL); unproven blocks the run
  (GOLD_AUDIT_PASS gates scoring).
- **Semantic obligations**: contains_text, source_decl, absence, written_call,
  relation_direction, manifest_dependency, mutation_marker/scope/position.
- **Direction validated**: relation_direction requires caller-pkg + call-verb +
  callee in order; a reversed answer fails (cc-eng.S1 TASK_FAILURE proves it).
- **Mutation rigor**: requires an actual diff + scope (top-level for const) +
  position (comment immediately above anchor) + validator-owned build/test
  exit code.
- **Committed fixtures**: tools/eval/fixtures/{microrepo,crate} +
  FIXTURE_MANIFEST digests — no /tmp dependency.
- **One-command pipeline**: run_smoke.py does fixture-verify -> gold audit ->
  preflight -> sessions -> traces -> classify -> tokens/timing -> validate ->
  aggregate -> artifacts -> manifest, all in one invocation.

## Smoke result
32 sessions, all EXECUTION_VALID, 0 INFRA_INVALID. cc-eng.S1 = TASK_FAILURE
(direction) — a trustworthy negative, not hidden.

## Verdicts
REPODEX_BENCHMARK_INSTRUMENT_CLOSED · READY_FOR_REPODEX_FIRST_EVIDENCE_NAVIGATION_V1
