# Claim -> evidence
| claim | evidence |
|---|---|
| path-substring false-positive fixed | classify.py `_is_repodex_invocation` + test_classifier_no_path_false_positive + CLASSIFICATION_CORRECTION.json |
| corrected counts (7 -> 0) | SESSION_METRICS_CORRECTED.csv + TOOL_EXECUTION_SUMMARY_CORRECTED.json |
| S0 native false repodex = 0 | CLASSIFICATION_CORRECTION.json |
| --from-traces reclassifies raw requests | run_smoke.py `_reclassified_from_raw` |
| allowed_changed_paths + relative paths | validator.py + test_allowed_changed_paths / test_changed_paths_repo_relative |
| preflight build must exit 0 | run_smoke.py tool_preflight `returncode == 0` |
| repodex-prepare failure blocks | run_smoke.py run_one `prepare_ok` -> INFRA_INVALID |
