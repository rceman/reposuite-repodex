# Claim -> evidence
| claim | evidence |
|---|---|
| checkpoints now derive from first-N investigations | CHECKPOINT_* + CHECKPOINT_DIFFS (saturate at C5) |
| curriculum is production-generated | CURRICULUM_REPRODUCIBILITY + repodex_learn |
| memory acts internally, not Agent-visible | RouteBias in adaptive_rdx_biased; AGENT_VISIBLE_PARITY |
| dep-aware rebind | learning_gates dep_aware_* tests |
| bounded lookup | MEMORY_SCALE_BENCHMARK + memory_index_bounded_lookup test |
| NO mechanical effect | PROBE_RESULTS mechanical_effect_probes=0 |
