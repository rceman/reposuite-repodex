# RepositoryView-Bound Symbol Memory V1 — report

## What was built

Production RepositoryView queries now consume Investigation Memory through a
safe composition layer:

- **`memory/project`** — canonical project scope: `repo:{sha256(common_dir)}`
  for git (shared across worktrees/branches), `project:{alias}` for registered
  projects without git, `root:{sha256(canonical_root)}` for non-git `--root`.
  Never the bare `"root"` constant; branch names are metadata only.
- **`memory/rebind`** — stable declaration locator
  `{lang}:{path}:{kind}:{qualified_name}` (qualified via `scope_path`), name/
  header/body facets, and `TargetViewRebinder` producing an explicit
  `BindingState` with current path+range materialized from the target snapshot.
- **`memory/compose`** — `off|file|symbol` modes; bounded `MemoryComposition`
  attached to the canonical `EvidenceProjection`; trivial single-exact lookups
  stay minimal.
- **`symbol_exposure`** — exposures now capture `symbol_locator` + `DeclFacets`
  (bytes via SourceStore or the live indexed file).
- **`memory/live` + `service/derived`** — incremental `AgentEvent -> episode ->
  SymbolExposure -> memory` on the checkpoint boundary; replay-equivalent.

## §72 — required answers

1. **Authoritative project identity**: `repo:{sha256(canonical git common_dir)}` — shared by all worktrees/branches of the logical repo, distinct for unrelated repos. Fallbacks: `project:{alias}`, `root:{sha256(root)}`.
2. **Two worktrees share memory**: yes — same common-dir digest ⇒ identical scope (verified).
3. **Unrelated projects share accidentally**: no — distinct digests + scoped query filter; identical paths/names never mix.
4. **`--root` without safe identity**: `root:{sha256(canonical_root)}` scope; if unresolvable the contribution is unscoped and never matches a scoped query — not guessed.
5. **Origin provenance retained**: project scope, investigation/session id, repo_head, path, file_content_digest, symbol locator, facets, exposure kind/provenance, freshness, timestamps — immutable; never rewritten to look current.
6. **Symbol IDs stable across versions**: `decl:{path}#{ordinal}` is file-local and position-unstable — NOT used for cross-version identity. The stable `locator` is used for rebinding.
7. **Locator for rebinding**: `{lang}:{path}:{kind}:{qualified_name}`; the path-free `{lang}:{kind}:{qname}` key enables move detection.
8. **Facets**: `name_digest`, `header_digest`, `body_digest` (when a body exists), `policy`.
9. **Whole-file digest invalidate unchanged Foo**: no — intrinsic decl/header/body facets still match ⇒ `ExactFresh` (verified).
10. **Body changes**: `changed_implementation`; old body facts not reused (candidate).
11. **Signature changes**: `changed_interface`; stronger staleness.
12. **Moves files**: `moved_but_same` when qkey+facets match; new path+range materialized.
13. **Renamed**: no name-similarity inference — `ambiguous`/candidate; never auto-FACT.
14. **Duplicated**: multiple distinct-path candidates ⇒ `ambiguous` unless facets disambiguate.
15. **Splits**: not auto-equated — `ambiguous`/candidate navigation only.
16. **Removed**: `absent`; never a stale current fact.
17. **Ambiguity**: explicit `BindingState::Ambiguous`/`Unknown` — never compressed to a bare `fresh=true`.
18. **Memory directly builds current projection data**: no — only via rebinding.
19. **Target-view validation enforced**: rebinder reads only the validated `indexes/{key}/snapshot` + current source bytes.
20. **Current ranges from current view**: always — `SourceRange` of the target decl; old coordinates never returned.
21. **Exact freshness make callers current**: no — entity equality does not validate relationships.
22. **New caller added**: memory never hides it; relationship facts come from the target-view graph, not the historical caller list.
23. **Config/metadata change**: invalidates via the existing dependency-validity gate; memory-derived current evidence is not trusted without a valid index.
24. **Uncertain reference occurrences**: kept as bounded CANDIDATE; only declaration/enclosing-read evidence carries FACT (`certainty` preserved end-to-end).
25. **`/v1/query` supports memory**: yes — `memory_mode` request field.
26. **Modes in direct + service**: yes — `--memory` flag, `--json` `memory_mode`, `POST /v1/query` `memory_mode` — same `MemoryMode` + `compose`.
27. **Deterministic query without memory**: yes — `off`/unavailable/degraded returns an empty composition; seeds unaffected.
28. **Service maintains memory from durable events**: yes — incremental `derive_live` on checkpoint.
29. **p95 derivation lag**: bounded by checkpoint cadence; reported via `derived_lag_events` (events pending). No separate p95 timer in V1 — noted honestly.
30. **After crash/replay**: derived state rebuilt deterministically from the durable log; converges to the same memory (verified).
31. **5/10/25 simultaneous agents**: sessions/investigations stay isolated by scope; foreign-scope events never match a scoped query (tested with 10 in-scope + foreign).
32. **Cross-view memory experiment**: unchanged symbols transfer as `ExactFresh`/`MovedButSame`; changed/absent/ambiguous handled conservatively; 0 stale-fact contamination.
33. **Real agent benchmark ran**: yes — 18 serial `devin -p` sessions (3 q × 3 treatments × 2 reps).
34. **T0/T1/T2 correctness**: 6/6 / 6/6 / 6/6 — non-inferior.
35. **Input tokens**: off 31775, file 31592, symbol 37476 (symbol +18%).
36. **Output tokens**: off 467, file 415, symbol 501.
37. **Tool calls / steps**: steps 9.2 / 9.2 / 9.5 (within noise).
38. **Searches**: not separately exposed by this ATIF schema (work proxy = total_steps).
39. **Reads**: same — bounded by steps.
40. **Time to primary evidence**: not separately instrumented in the ATIF export.
41. **Time to sufficient evidence**: same.
42. **Total wall**: off 13.8s, file 12.8s, symbol 14.2s (within noise).
43. **Trivial exact lookups**: packet bounded — memory block ~1KB, suppressed on single-exact-match.
44. **Overserving regression reappeared**: no — memory is a small annotation, not a second packet.
45. **Symbol memory default**: **opt-in only** — safe but no measured positive work benefit on these lookups; per §66-§67 it stays `PRODUCTION_OPT_IN_ONLY`.
46. **Ready for Gateway task/code after Relay telemetry**: the production query path now safely composes memory; readiness for Gateway integration is gated on Relay telemetry availability, not this work.

## Classification

`REPOSITORY_VIEW_BOUND_SYMBOL_MEMORY_READY` — the safety/identity/freshness
layer is complete, tested, and integrated into the production query path. The
agent benchmark shows it is safe but not yet a measured win, so the production
default remains opt-in.

## Commits

`2099b08` implementation, `9b168ea` cross-view/live tests, eval artifacts.
