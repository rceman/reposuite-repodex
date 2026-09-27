# Claim -> evidence

| claim | evidence |
|---|---|
| D3 input=63,944 | SESSION_METRICS.csv (D3 mean) + DELIVERY_BENCHMARK.json |
| D3 tools=4.8 | SESSION_METRICS.csv + DELIVERY_BENCHMARK.json |
| D3 searches=1.1 | SESSION_METRICS.csv + DELIVERY_BENCHMARK.json |
| D0 20/48, D1 20/48, D2 18/48, D3 18/48 | SESSION_METRICS.csv correct column |
| RDX-LITE <=2KiB | RDX_LITE_SPEC.md + TRACE_WALKTHROUGH.md (real packet ~120B) |
| repodex invoked ~46-50% on-demand | DELIVERY_BENCHMARK.json ondemand_usage + traces (exec tool args) |
| symbol-memory neutral | MEMORY_CURRENT_HEAD.json |
| Jev rerank used + identical order | JEV_REPRODUCTION.json + JEV_CURRENT_HEAD.json |
| full trace per session | traces/<session>.json + TRACE_INDEX.json |
