# Claim -> evidence
| claim | evidence |
|---|---|
| all gold source-proven | GOLD_SOURCE_PROOFS.json + gold.py |
| direction validated | validator.relation_direction + cc-eng traces + test_relation_direction |
| mutation needs diff+scope+build | validator.worktree_proof + test_mutation_* + VALIDATION_RESULTS |
| 0 rejections/cancellations | TOOL_PREFLIGHT + traces (rejected=0) |
| committed fixtures | tools/eval/fixtures + FIXTURE_MANIFEST.json |
| one-command pipeline | run_smoke.py + README_FOR_REVIEW.md |
| full traces | traces/*.json + TRACE_INDEX.json |
