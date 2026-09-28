# Review order
1. REPORT.md 2. SUMMARY.json 3. tools/eval/* (runner/tracer/validator/gold/classify)
4. TASK_CORPUS + GOLD_CONTRACT + GOLD_SOURCE_PROOFS 5. SESSION_METRICS.csv
6. VALIDATION_RESULTS.json 7. traces/<session>.json 8. CLAIM_TO_EVIDENCE.md
Reproduce: python3 tools/eval/run_smoke.py --binary <bin> --workdir <dir>
