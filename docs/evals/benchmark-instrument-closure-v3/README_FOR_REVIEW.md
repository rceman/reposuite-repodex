# Review order
1. REPORT.md 2. SUMMARY.json 3. tools/eval/ (committed harness) 4. TASK_CORPUS +
GOLD_CONTRACT + GOLD_SOURCE_PROOFS 5. FIXTURE_MANIFEST 6. VALIDATION_RESULTS
7. SESSION_METRICS.csv 8. traces/ 9. CLAIM_TO_EVIDENCE.md
Reproduce: python3 tools/eval/run_smoke.py --binary <bin> --workdir <dir>
