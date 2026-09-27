# TRACE_WALKTHROUGH — matched task `qa-cache` (real scored sessions)

Task: `How does the cache get values and what is the Get signature?`
Root: `/tmp/mempilot/repo`. All four delivery arms below are REAL scored runs.

---

## D0 — Native (input 92,390 tok; 10 tools)

T+0000 USER task.
T+.. MODEL -> exec `ls -la /tmp/mempilot/repo && find ... -name "*.md"`
T+.. exec `find ... -type f -not -path "*/.git/*"` (enumerate repo)
T+.. read internal/engine/cache.go
T+.. read internal/engine/core.go
T+.. read internal/engine/handler.go
T+.. read internal/engine/registry.go
T+.. read internal/util/resolve.go
T+.. read internal/util/codec.go
T+.. read core_test.go -> read resolve.go -> grep `.Get`
T+.. FINAL ANSWER: "Cache struct stores resolved keys in `map[string]int` m
(cache.go:4); Get does `c.m[k]` and returns the int" — CORRECT.

Broad exploration: enumerated then read ~8 files to locate Get.

## D1 — Auto full RDX (input 40,614; 2 tools)

T+0  packet injected (real RDX received by the agent):
```
#RDX1 v2
Q How\ does\ the\ cache\ get\ values\ and\ what\ is\ the\ Get\ signature?
F 1 declaration decl:internal/engine/cache.go#3 Get
F 2 declaration decl:internal/engine/cache.go#1 Cache
F 3 file file:internal/engine/cache.go internal/engine/cache.go
F 4 node call:internal/engine/cache.go#0 c.Get
F 5 node decl:internal/engine/cache.go#0 engine
F 1 node decl:internal/engine/cache.go#3 Get
F 7 node decl:internal/engine/cache.go#4 Resolve
F 8 node ent:ent-9c1f9d6e98d52ae0 internal/engine:engine
F 9 node file:go.mod go.mod
D 1 rank=1 path=internal/engine/cache.go decl=6:86-137 body=6:120-137
D 2 rank=2 path=internal/engine/cache.go decl=4:52-84 body=4:58-84
D 3 rank=3 path=internal/engine/cache.go
R contains 3 4 f rule=repodex.containment
R member_of 3 8 f rule=repodex.package_membership
R owned_by_manifest 3 9 f rule=repodex.manifest_ownership
S shown=3 total=3 complete=1 seeds=3 related=9 candidates=0
```
T+.. exec `cat -n internal/engine/cache.go; grep -rn "\.Get("`  (verify only)
T+.. FINAL ANSWER — CORRECT. The packet named cache.go + Get directly;
the agent verified with 2 commands instead of exploring 8 files.

## D2 — On-demand tool (input 50,450; 4 tools)

T+0 no packet; `repo_query` available.
T+.. exec `ls + find` (orient) -> grep `class .*Cache|def get|func .*Get` ->
read cache.go -> grep `Cache|\.Get`.  Agent answered WITHOUT calling
repo_query — self-selected native tools for this direct lookup. CORRECT.

## D3 — RDX-LITE + on-demand (input 37,543; 3 tools)

T+0 RDX-LITE bootstrap received:
```
#RDX1 v2
F 1 declaration decl:internal/engine/cache.go#3 Get
F 1 node decl:internal/engine/cache.go#3 Get
```
(`repo_query` tool available for expansion.)
T+.. read internal/engine/cache.go -> exec `ls + find *.go` -> grep.
T+.. FINAL ANSWER — CORRECT. The lite anchor pointed at cache.go; the agent
verified + one broaden search. Cheapest input AND fewest tools of all arms.

## Delta (qa-cache)

| arm | input | tools | reads | search | repodex |
|---|---|---|---|---|---|
| D0 | 92,390 | 10 | 8 | 1 | - |
| D1 | 40,614 | 2 | via exec | 0 | auto |
| D2 | 50,450 | 4 | 1 | 3 | not called |
| D3 | 37,543 | 3 | 1 | 1 | not needed |

D3 gives the localization win at the lowest token cost.
