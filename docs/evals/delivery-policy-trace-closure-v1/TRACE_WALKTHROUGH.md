# Trace walkthrough — qa-cache (all 4 delivery arms, real sessions)

T+0 USER: "How does the cache get values and what is the Get signature?"

## D0 Native  (in=92,390, 10 tools)
exec(ls) -> exec(find) -> read cache.go -> read core.go -> read handler.go ->
read resolve.go -> read codec.go -> ... x8 reads -> grep. Broad exploration.

## D1 Auto-RDX  (in=40,614, 2 tools)
packet embedded -> exec x2 (verify) -> ANSWER. Packet pre-answered; near-zero
exploration.

## D2 On-demand  (in=50,450, 4 tools)
exec -> grep -> read cache.go -> grep. Agent searched first; did not call
repo_query this session (answered directly). Still cheaper than native.

## D3 Lite+on-demand  (in=37,543, 3 tools)
RDX-LITE (top anchor) -> read cache.go -> exec -> grep -> ANSWER. Cheapest AND
fewest tools.

## Delta
Auto/full and lite packets collapse broad exploration (10 tools -> 2-3).
D3 keeps the collapse at the lowest token cost.
