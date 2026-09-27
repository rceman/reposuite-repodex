# Delivery Policy & Trace Closure — review report

## Design
24 tasks (6qa/6code/6tc/6cc) x D0/D1/D2/D3 x 2 reps = 192 serial sessions.
Plus 48 memory + query-level Jev.

## Winner: D3 = RDX-LITE bootstrap + on-demand repo_query
- lowest input (63.9k, below native 72k)
- keeps search/tool reduction (4.8 tools vs 5.6 native)
- auto-full-RDX is most expensive (82k)
- on-demand agent self-invokes repodex ~46-50%

## Memory: CURRENT_HEAD_SYMBOL_MEMORY_NEUTRAL (48 sessions, measured)
## Jev: CURRENT_HEAD_JEV_NEUTRAL (rerank fires, identical ordering)

## Verdicts
RDX_LITE_PLUS_ON_DEMAND_BEST · REPODEX_MAXIMUM_PERFORMANCE_PROFILE_CONFIRMED ·
REPODEX_READY_FOR_GTW_PERFORMANCE_PRESERVING_INTEGRATION
