# Claim -> evidence
| claim | evidence |
|---|---|
| tc-ver gold corrected (no go directive) | TASK_CORPUS.json tc-ver absent_claim + GOLD_SOURCE_PROOFS.json + tc-ver.S0 traces -> TASK_SUCCESS |
| execution vs validation split | traces (execution_status + validation_status) + validator.py |
| 0 rejections/cancellations | TOOL_PREFLIGHT + VALIDATION_RESULTS (rejected=0) |
| code tasks worktree-validated | VALIDATION_RESULTS worktree proof + code-* traces |
| exit code vs transport | traces TOOL_RESULT tool_transport_status + process_exit_code |
| repodex wall | traces repodex_prepare_wall_ms + TIMING.json |
| harness source committed | tools/eval/* (this dir) |
| negative+equiv validator tests | VALIDATOR_TESTS + test_validator.py |
