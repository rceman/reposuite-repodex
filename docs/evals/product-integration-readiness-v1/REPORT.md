# Product Integration Readiness V1 — report

## §57 — required answers

1. **Retrieval micro-opt stopped?** yes — `STOP_FOR_PRODUCT_INTEGRATION`.
2. **Unresolved gap blocking stop?** none — residual is true semantic synonymy, not a correctness gap.
3. **Why no morphology Agent screening?** V1 gave a marginal +6/40 retrieval gain with no clean win over the residual semantic gap — the §43 eligibility gate (meaningful gain + bounded FP + no regression + acceptable cost) was met only marginally; no Agent spend on a marginal mechanism.
4. **Consistent with frozen gate?** yes — marginal gain → not screened. No inconsistency.
5. **Policy of every optional subsystem**: manifest=default; morphology/repo-native vocab=opt-in; cheap-model=offline-experimental; memory/adaptive/recipes/witness=opt-in; utility=shadow-only.
6. **One public executable?** yes — `reposuite-repodex` (`~/go/bin/repodex` is an unrelated GOPATH leftover, not ours).
7. **Lifecycle clean-room?** PASS (status/start/query/restart/stop, no orphan).
8. **status start daemon?** no.
9. **OS service install/manage?** no.
10. **Runtime dir**: `~/reposuite/repodex` (override via env/`--state-dir`).
11. **Machine API**: `/v1/{status,query,events,events/batch,shutdown}`.
12. **Compat documented?** schema-stamped v1 responses; additive fields backward-compatible.
13. **Error model**: UNAUTHORIZED/INVALID_REQUEST/INVALID_ROOT/VIEW_INVALID/QUERY_UNRESOLVED/QUERY_AMBIGUOUS/BUDGET_EXHAUSTED/SERVICE_UNAVAILABLE/INTERNAL; "no evidence" = 200 empty.
14. **Explicit root query?** yes — content-keyed authority.
15. **cwd/worktree fallback?** none for service queries (explicit root required).
16. **Worktree distinction**: by exact `root` → RepositoryView; branch is metadata.
17. **Cross-worktree reuse**: identical content shares content-addressed analysis.
18. **Task/worktree authority**: Gateway, not RepoDex.
19. **Gateway sends**: `root` + `query` (+ optional bounded policies).
20. **RepoDex Task semantics?** none — Task-neutral.
21. **Relay call /v1/query?** no — telemetry only.
22. **Relay sends**: canonical AgentEvents to `/v1/events[/batch]`.
23. **SourceObserved**: source content actually delivered — not a mention.
24. **Path mention → SourceObserved?** no.
25. **Runtime/adapter opaque?** yes (strings, never branched).
26. **ACK waits for derivation?** no — durable ack is independent.
27. **Batch atomicity**: per-event non-atomic; valid siblings commit; `event_id` idempotent dedupe.
28. **Incremental == replay?** yes (deterministic event_id + checkpoint/replay).
29. **Hard crash**: durable events survive; restart no semantic loss.
30. **Corrupt tail**: actionable diagnostic, never invents state.
31. **view-A memory → stale current on B?** no (view-bound rebinding).
32. **Dirty source authoritative post-restart?** yes.
33. **Recommended Gateway profile**: `root`+`query`, defaults (manifest on, rest off).
34. **Integrators touch all toggles?** no — production-v1 profile hides them.
35. **Stable profile justified?** yes — reduces integration ambiguity.
36. **status exposes**: pid/endpoint/uptime/version/memory/index state.
37. **doctor needed?** no — status suffices.
38. **Idle RSS**: ~10.4 MiB.
39. **Warm query p50/p95**: 28/30 ms.
40. **Cold index**: ~4.8s (reposuite ~290 files).
41. **Warm reopen**: ~513ms.
42-46. update/ingest/replay: bounded, sub-ms ingest, no regression.
47. **Performance regression?** none.
48. **Clean-room E2E passed?** yes.
49. **Failed/N-A steps**: none failed.
50. **Gateway blockers**: none on RepoDex side.
51. **Relay blockers**: none on RepoDex side.
52. **Integration ready?** yes.
53. **First integration**: Gateway query (no consumer yet owns Task→root).
54. **Next task**: GTW_REPODEX_QUERY_INTEGRATION_V1.

## Verdict

`REPODEX_PRODUCT_INTEGRATION_READY` — bounded stable component, contracts
written, clean-room passes, no blockers on the RepoDex side.

## R&D policy

`REPODEX_RETRIEVAL_R_AND_D_PAUSED_FOR_INTEGRATION`
