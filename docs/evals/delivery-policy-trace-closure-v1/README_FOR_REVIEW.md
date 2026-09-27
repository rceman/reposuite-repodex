# Review entry point

Open in order:
1. REPORT.md — findings + verdicts
2. SUMMARY.json — invariants + totals
3. DELIVERY_BENCHMARK.json — D0-D3 aggregate
4. SESSION_METRICS.csv — one row per scored session
5. TRACE_WALKTHROUGH.md — real matched task across all 4 arms
6. traces/<session>.json — per-session observable trace (see TRACE_INDEX.json)
7. RDX_LITE_SPEC.md — the winning delivery format
8. MEMORY_CURRENT_HEAD.json, JEV_CURRENT_HEAD.json — ablation results
9. CLAIM_TO_EVIDENCE.md — claim->artifact map

Machine JSON = machine/debug only. RDX / RDX-LITE = agent-facing. AGENT_FACING_JSON_OUTPUT=false.
