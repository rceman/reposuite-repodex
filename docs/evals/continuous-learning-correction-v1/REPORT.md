# Continuous Learning Correction V1 — report

## What was wrong in V1
- Hand-authored curriculum labeled ordinary declarations as `entrypoint`
  questions (production `inventory()` correctly identifies only `main`).
- C5/C10/C25/C50 all carried the same 4 memory artifacts — checkpoints did not
  actually progress.
- `LEARNED_NAV_HINT` was injected into the Agent prompt — memory leaked into
  Agent-visible context instead of acting internally.
- Rebind used whole-view digest equality (too coarse).
- Memory lookup unioned family+anchors (could return the whole store).

## Corrections (src/learning.rs, src/query/adaptive.rs, repodex_learn)
- Production `inventory()->coverage()->generate_questions()` drives the
  curriculum — deterministic, reproducible (CURRICULUM_MATCHES_PRODUCTION_GENERATOR=true).
- Real checkpoint snapshots = state after first-N investigations.
- Memory acts internally via `internal_route()` + `RouteBias` reordering —
  no Agent-visible hint, B0==L1 Agent-visible contract.
- Dependency-aware rebind: anchors resolve uniquely; stored dep identities
  validated, not whole-view equality.
- Bounded MemoryIndex (family+anchor intersect) — bounded at 10k records.
- Real 100/1k/10k scale benchmark.

## Result — honest
Production curriculum generated **6 real gap-driven questions** -> 3 memory
families (config_control/test_verification/ownership; no call_path gap fired).
Checkpoints saturate after ~5 investigations (LEARNING_SATURATES_AT_C5).
On 20 probes, **0 showed a beneficial mechanical memory effect** — adaptive
selection already surfaces the same relevant evidence; internal reordering
changed no emitted packet.

Per §29 the campaign **stopped before Agent spend**:
`LEARNING_MEMORY_HAS_NO_MECHANICAL_NAVIGATION_EFFECT`.

## Verdicts
INTERNAL_CONTINUOUS_LEARNING_ARCHITECTURE_NOT_USEFUL
NO_MEASURED_CONTINUOUS_LEARNING_GAIN
NO_FURTHER_LEARNING_WORK_CURRENTLY_JUSTIFIED
