# Guarded Evidence Recipes V1 — report

## §75 — required answers

1. **Recipe**: a bounded, version-safe plan (typed selector steps) reconstructing *current* evidence for a recurring investigation shape.
2. **Not a recipe**: cached answer, final conclusion, historical source dump, shell/tool command sequence.
3. **Selectors**: resolve_anchor, resolve_memory_anchor, callers_of, callees_of, related_of, path_to, test_candidates_of, owning_module_of.
4. **Arbitrary shell/code**: no — typed enum only, read-only, bounded.
5. **Project scope**: recipe store partitioned by canonical scope digest; scoped query filter.
6. **Applicability**: conservative — query-family term coverage + intent/family consistency + resolvable anchor.
7. **AMBIGUOUS**: >1 matching recipe → `RecipeMatch::Ambiguous` → no execution.
8. **NO_MATCH**: `RecipeMatch::NoMatch` → deterministic query unaffected.
9. **Min support**: `MIN_SUPPORT=2` episodes sharing a motif.
10. **Mining signals**: query_text identifier terms, symbol-exposure evidence roles, evidence_path_mentions — not final-answer text.
11. **Final answers → truth**: never; only which evidence was referenced.
12. **Path generalization**: concrete entities → parameterized `ResolveAnchor` + selector family, not memorized paths.
13. **Target-view binding**: execution resolves the anchor from the current query seeds and runs selectors on the current engine.
14. **Anchor moved**: re-resolved at current location (locator/rebind).
15. **Changed implementation**: recipe still runs — it describes *how* to get evidence, not the bytes.
16. **Changed interface**: current signature materialized fresh.
17. **Removed anchor**: `fell_back=true`, produces nothing, query unaffected.
18. **Ambiguous anchor**: candidate-only / fallback, never a guessed identity.
19. **New caller added**: `CallersOf` re-derives the current caller set — new caller included (not replayed).
20. **New test**: `TestCandidatesOf` re-derives current test candidates.
21. **Config/metadata change**: inherits the current validated view + dependency-validity gate.
22. **Old caller/test replay**: never — selectors always run the current query.
23. **Families**: DefinitionToCallers, DefinitionToCallees, DefinitionToTestCandidates, TwoAnchorConnector, DefinitionToPackageModule.
24. **Recipes mined (eval)**: 1 authored `definition_to_callers` recipe in the bench store.
25. **Match rate**: matched on 5 recipe-family+paraphrase questions.
26. **Ambiguous rate**: 0 in the eval corpus.
27. **No-match rate**: 3 controls correctly produced no recipe.
28. **Guard-failure rate**: 0 in the eval corpus.
29. **Recipe overhead**: sub-ms match + one sub-query per selector step; ≤ a normal query.
30. **New parsing by recipe**: 0.
31. **Replay-equivalent derivation**: recipe store is deterministic over mined history.
32. **Real Agent benchmark**: yes — 32 serial `devin -p` sessions.
33. **R0/R1 correctness**: 16/16 both.
34. **Input tokens**: R0 34061, R1 37152.
35. **Output tokens**: R0 483, R1 533.
36. **Tool calls/steps**: R0 9.5, R1 9.7.
37. **Searches**: not separately exposed (steps proxy).
38. **Reads**: same.
39. **Time to primary evidence**: not separately instrumented.
40. **Time to sufficient evidence**: same.
41. **Agent wall**: R0 13.7 s, R1 15.7 s.
42. **Combined wall**: same + ~45 ms RepoDex query.
43. **Recipe-family cohort**: R1 ≈ R0 (in 38933 vs 38266, wall 17.5 vs 15.1) — no benefit.
44. **Paraphrase cohort**: R1 worse on this small repo (tokens +23%).
45. **No-recipe control**: R1 ≈ R0 (within noise) — cheap no-match lookup.
46. **Wrong-family increased work**: no wrong-family recipe fired (controls unaffected).
47. **Worth retaining**: yes — the guarded machinery is correct + safe; it simply didn't pay off on a small repo where deterministic retrieval already sufficed.
48. **Production policy**: **opt-in** — correct + safe, no measured agent-work benefit in this cohort.

## Verdict

The recipe engine is deterministic, read-only, project-scoped, target-view
bound, and never replays stale evidence. It did NOT reduce agent work in this
screening cohort — third consecutive finding that deterministic retrieval
already suffices on a small repo. OPT_IN, not default.

## Commits

`2742025` (Phase A CLI), `1942060` (Phase B recipes), eval artifacts.
