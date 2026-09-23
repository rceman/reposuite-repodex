# RepositoryView + Gateway-Ready CLI — Eval Spec

Non-Agent correctness/performance benchmark for the RepositoryView + machine
CLI foundation. Agent token metrics are N/A (§67); the KPI framework
(`../BENCHMARK_METRICS.md`) is applied as Correctness / Cost / Work / Time.

## Fixture (§72)

Isolated git repo with:

```text
main            internal/auth/check.go(CheckAuthMain) api/serve.go(ServeMain) shared.go(SharedMain)
work-a  task/a  shared.go(SharedA) check.go(CheckAuthA) + a.go(OnlyInA)
work-b  task/b  shared.go(SharedB) check.go(CheckAuthB) - serve.go  + b.go(OnlyInB)
work-c  detach  identical content to main
work-d  detach  identical content to main
```

`REPODEX_STATE_DIR=/tmp/rv-*` isolates the registry/objects/indexes — tests
never touch `~/reposuite/repodex/projects.json` (§11).

## Assertions (§73)

- per-view symbols: main→SharedMain, a→SharedA, b→SharedB, c→SharedMain
- same relative path does not cross-contaminate
- deleted `serve.go` absent in B, present in main
- added `a.go`/`b.go` visible only in their view
- uncommitted edit becomes query-visible (new fingerprint)
- `--root` == `--project --view` results
- unrelated cwd (`cd /tmp`) does not affect locator authority
- interleaved A/B/A/C/B queries correct; concurrent processes no corruption
- view traversal (`../main`) rejected; `root XOR project`; `--view` requires `--project`

## Timing / storage (§71, §75-§76)

Cold first query (full build), warm repeat, A→B→A switch, dirty/add/delete
refresh; FileAnalysis reuse %, incremental disk for an identical worktree.
