# Performance Ceiling & Trace V1 — report

## Sessions

| study | sessions |
|---|---|
| final F0/F1/F2 | 192 |
| memory ablation | 24 |
| Jev | 0 (not reproduced, neutral) |
| output screen | 0 (RDX already the compact format — measured statically) |
| **TOTAL** | **216** (<=480) |

## §98 output audit

1. Production-v1 agent format: `--json` machine response was the packet used in benchmarks; canonical agent-facing = RDX.
2. Payload: JSON mean 79KB / RDX mean 22KB.
3-10. 92% of JSON is `related` edges; 31% duplicated strings (rule ids, paths); field-name/syntax large.
11. Fields never needed: repeated provenance/rule-id/digest metadata.
13-14. Candidates: O0 JSON 79KB, O1 RDX 22KB.
16. RDX entered as the compact arm — it is the existing canonical agent format.
23-24. Winning format: **RDX** — removes ~57KB (~14k est tokens) per packet.
25. Downstream extra work: none — same tool-call reduction as JSON.

## §99 memory

26-28. File memory on cold tasks: +8k tokens, corr 3->4 (noise). Memory OFF fastest-safe.
34. Max-perf memory: off.

## §100 Jev

35-48. Not reproduced at current head; historical marginal; `JEV_NOT_IN_MAX_PERFORMANCE_PROFILE`, `CURRENT_HEAD_JEV_NEUTRAL`.

## §101 Native vs RepoDex

49-52. Correctness F0 31 / F1 27 / F2 28 (n=64 each) — RepoDex does not improve correctness.
53-56. Input F0 64k / F1 108k / F2 72k.
57-58. F1 +69% input vs native; F2 +12% (packet cost).
60-63. Tool calls F0 5.5 / F1 4.0 / F2 3.8 — RepoDex -31%.
65-68. Searches F0 1.5 -> 0.6 (-60%); reads F0 2.6 -> 2.3.
75. RepoDex own latency ~28ms — negligible share of wall.

## §102 task class

76-79. Q&A/tc flat; cc (cross-cutting) marginal gain; code cohort all-poor (packet slightly hurts — must edit/verify anyway).
80. Code cohort regressed slightly under packet arms.
81. RepoDex improves investigation cost (searches), not patch/test success.
83. Largest benefit: cross-cutting. 84. Smallest/negative: code-change.

## §105 final

112. Fastest safe config: **MAX_PERFORMANCE_PROFILE_V1** — RDX packet, memory off, vocab/recipes/witness off, manifest on.
113-114. vs Native: -31% tools, -60% searches, +12% tokens, correctness neutral.
115. vs JSON production: -34% tokens, same tool benefit.
116. Largest remaining Agent cost: verification + semantic interpretation (model reads/reasons regardless).
117-118. Retrieval and output representation are NOT the bottleneck.
119-121. Dominant: model semantic reasoning/verification (and patch/test on code tasks).
122. More RepoDex R&D justified? No — ceiling reached for current architecture.
123. Gateway should preserve: RDX agent packet (not JSON), explicit-root authority, task-neutral query.
124. Gateway gets compact RDX for the agent; rich JSON stays machine/debug only.
125. Delivery-mode recommendation: inject the first RDX packet only on tasks where localization is the bottleneck (cross-cutting); for pure single-hop lookups the packet costs more than it saves — test Agent-invoked query tool vs auto-inject during GTW integration.

## Verdicts

- `REPODEX_AGENT_OUTPUT_COMPACTION_CONFIRMED` (RDX 72% smaller, same benefit)
- `CURRENT_HEAD_MEMORY_NEUTRAL`
- `CURRENT_HEAD_JEV_NEUTRAL`
- `REPODEX_PERFORMANCE_CEILING_REACHED_FOR_CURRENT_ARCHITECTURE`
- `REPODEX_READY_FOR_GTW_PERFORMANCE_PRESERVING_INTEGRATION`
