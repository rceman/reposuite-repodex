# Semantic Vocabulary Bridge Campaign V1 — report

## §57 session accounting

| study | sessions |
|---|---|
| manifest A/B (Phase 0) | 80 |
| model vocabulary selection (Phase D, offline) | 11 |
| vocab Agent screening | 0 (no clean winner) |
| **TOTAL_AGENT_SESSIONS** | **91** (<=480) |

## §68 — required answers

1. **Why manifest benchmark was missing**: prior milestone measured retrieval only; causal Agent evaluation was deferred. Now closed.
2. **80 manifest sessions ran**: yes (M0/M1 x 20q x 2reps).
3. **Manifest correctness M0/M1**: 36/40 both — identical.
4. **Evidence recall**: manifest facts retrievable (indexed).
5-8. **Searches/reads/calls/time**: steps 10.6->10.2, marginal.
9. **Tokens**: M1 ~10% fewer input tokens (manifest fact pre-delivered).
10. **Manifest-dep tasks improved**: correctness identical (32/36); steps -0.4.
11. **Source-only controls regressed**: no.
12. **Vocab questions frozen**: 40.
13. **Truly vocabulary-only baseline failures**: 25/40 missed baseline.
14-17. Morphological 6, abbreviation ~2, repository-terminology 2, true synonymy ~11, negative 6.
18. **Morphology performance**: +6 recall (15->21).
19. **Morphology false positives**: none on negative controls.
20. **Repository-native implemented**: yes (vocab_native).
21. **Sources**: manifest package/module/dependency declared names (repo-derived).
22. **Support/scoping**: project-scoped, bounded <=4/term, single layer.
23. **Terms/query**: <=12.
24. **Candidate inflation**: bounded, sub-ms.
25-28. **Recall@10 delta**: +2 over morphology (23 vs 21).
29. **Query overhead**: negligible.
30. **True synonymy remains after deterministic**: yes (~11).
31. **Cheap-model Phase D ran**: yes (offline selection).
32. **Model**: devin agent (authorized local).
33. **Selection-only**: yes (pick from bounded index vocabulary).
34. **Unknown terms discarded**: yes.
35-36. **Model tokens/cost**: ~11 offline calls, ms-scale each.
37. **V3 over V2**: +6 residual hits but noisy (loose broadening, not clean synonyms).
38. **Model semantic false positives**: picks like shutdown->Serve show it guesses neighbors.
39. **Vocab candidates clearing Agent gate**: none cleanly — V2 marginal (+2), model noisy.
40. **Agent sessions (vocab)**: 0.
41-49. **Agent endpoints**: n/a (not run).
50-51. **Cohorts**: n/a.
52-53. **Combination**: not run.
54. **encoder->Encode**: solved by morphology.
55. **shutdown->main**: NOT solved (model picked Serve).
56. **entry->main**: model picked cmd:main -> retrieved.
57. **crate->package**: solved by repository-native.
58. **Leave-family-out**: residual synonymy generalizes poorly (no clean bridge).
59. **Vocab mechanism to default**: none — all opt-in.
60. **Cheap model justified**: no — unreliable (noisy neighbor picks) and adds a dependency.
61. **Genuinely semantic failures**: shutdown->main, entry->main, storage->store.
62. **Diminishing returns reached**: yes — deterministic bridging recovers the easy gap; the residual needs actual semantics.
63. **Largest remaining gap**: true semantic synonymy that neither morphology, repo-native, nor a cheap model reliably bridges.
64. **Next task**: stop retrieval micro-optimization — REPODEX_RETRIEVAL_OPTIMIZATION_STOP_REVIEW_V1.

## Verdict

`VOCABULARY_GAP_SMALL_OR_NOT_WORTH_OPTIMIZING` — deterministic vocabulary
(morphology + repository-native) recovers the easy ~8/40 wording gap; the
residual ~11 are true semantic synonymy that neither deterministic bridging nor
a cheap model reliably solves (the model guesses neighbors, not the gold). No
mechanism produced a clean strong win to justify Agent spend or a default.

## Production policy

- morphology: `MORPHOLOGY_OPT_IN` (vocab_bridge)
- repository-native: `REPOSITORY_VOCABULARY_OPT_IN` (vocab_native)
- cheap model: `MODEL_VOCABULARY_OFFLINE_EXPERIMENTAL` (not productionized)

## Manifest Phase 0

`MANIFEST_AGENT_VALUE_NEUTRAL` — manifest indexing restores missing facts
(correct) but Agent correctness is unchanged (36/40 both arms) with marginal
~10% input-token reduction. The feature stays (it fixes a real retrieval gap);
its downstream Agent value is neutral, not negative.
