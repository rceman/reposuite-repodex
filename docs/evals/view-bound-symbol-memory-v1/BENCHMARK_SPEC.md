# RepositoryView-Bound Symbol Memory V1 — benchmark spec

Goal: project-wide historical navigation memory consumed by production
RepositoryView queries, with current-view rebinding as the only route to
current evidence.

## Safety rule

Historical memory may recommend where to investigate; only the current
validated RepositoryView produces current evidence. `HistoricalMemoryHit` has
no direct conversion to current evidence — it must pass `TargetViewRebinder`.

## Architecture

```
MemoryStore.query_scoped(query, known, scope)      (§4 project isolation)
  -> SymbolMemoryEvidence (stable locator + facets)  (§9-§13)
  -> TargetViewRebinder (current snapshot + bytes)   (§15)
  -> TargetViewMemoryAnnotation (state + current path/range)  (§16,§30)
  -> MemoryComposition -> EvidenceProjection.memory  (§29,§31)
```

## Memory modes (§33) — `off` | `file` | `symbol`

Same path in direct `--json`, flag-mode, and `POST /v1/query`
(`memory_mode` request field). Default `off`. Deterministic query always works
when memory is off/unavailable/degraded.

## Cross-view fixture (§51-§53)

`main`/`branch-a`/`branch-b` worktrees of one git project; mutations:
same-file change, body change, signature change, move, duplicate, removal,
dirty edit, not-yet-merged feature. Assertions in
`tests/view_bound_memory.rs`.

## Live derivation (§42-§47)

Derived worker mirrors durable AgentEvents -> `agent-events` store; on the
checkpoint boundary `memory::live::derive_live` runs episodes -> symbol
exposures -> memory. Async, incremental, replay-equivalent, project-isolated.
