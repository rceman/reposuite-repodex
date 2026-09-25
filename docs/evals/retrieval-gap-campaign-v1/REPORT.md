# Retrieval Gap Campaign V1 — report

## §57 session accounting

| study | sessions |
|---|---|
| vocabulary Agent screening | 0 (no retrieval-level mechanism cleared the gate) |
| join screening | 0 |
| main Agent benchmark | 0 |
| combination | 0 |
| **TOTAL_AGENT_SESSIONS** | **0** (<=480) |

The campaign's agent sessions were not spent because no retrieval mechanism
materially improved evidence at the retrieval level — per §38 do not run Agent
sessions for mechanisms that fail retrieval-level evaluation.

## §58 — required answers

1. **Already sufficient**: 31/40.
2. **Partially sufficient**: 4.
3. **Insufficient**: 5.
4. **Largest failure class**: MISSING_ENTITY_INDEX (manifest/config files not indexed).
5. **Vocabulary mismatch share**: 3/40 (8%).
6. **Missing relationship share**: 0 on this corpus (relations resolved).
7. **Missing test join share**: 0 (test candidates already resolved).
8. **Missing config/module join share**: 6/40 (15%) — manifests not indexed.
9. **Ranking-error share**: 0.
10. **Query-reachability share**: 0.
11. **Semantic-reasoning-required share**: ~2/40 (true synonymy like shutdown->main).
12. **Vocab Experiment A ran**: yes (deterministic morphological bridge).
13. **Expansion rules**: suffix-stem (-er/-or/-ing/-ed/-s/-tion/-ment/-ies) + dropped-e restore; bounded <=4 variants.
14. **Candidate count growth**: <=3 extra term lookups/term.
15. **Recall@k delta**: +1/40 gold (lx-encoder fixed).
16. **MRR/nDCG delta**: +1 corrected miss; marginal.
17. **False-positive cost**: none observed.
18. **Latency delta**: sub-ms.
19. **Vocab Agent screening**: not run (1/40 delta too small to isolate).
20. **Correctness delta**: retrieval +1 gold.
21. **Searches/reads/tool delta**: n/a (Agent not run).
22. **Cross-artifact join B ran**: no — the gap is missing INDEX content (manifests), not a missing join between existing facts.
23. **Generic join contract**: not reached — joins need facts that don't exist yet.
24. **FACT joins**: n/a.
25. **CANDIDATE joins**: n/a.
26. **Impl->test recall**: already sufficient.
27. **Impl->module/config recall**: fails — manifests not indexed.
28. **Go pilot ran**: no.
29. **What it improved**: n/a.
30. **Noise added**: n/a.
31. **Reachability fixes needed**: none.
32. **Existing capability merely unreachable**: no — misses are absent index content.
33. **Best retrieval mechanism**: vocab bridge (the only implemented one).
34. **Best at Agent level**: n/a.
35-42. **Agent endpoints**: not run (no mechanism cleared the retrieval gate).
43. **Control regression**: none observed.
44. **Combination arm**: not run.
45. **Combined outperform**: n/a.
46. **Remaining failures**: manifest/config indexing (deterministic) + true semantic synonymy (shutdown->main).
47. **Plausibly semantic fraction**: ~5% (2/40).
48. **Cheap semantic model justified**: no — dominant gap is deterministic indexing, not semantics.
49. **Which class for a model**: only the ~2/40 synonymy cases; too small to justify a model.
50. **Further deterministic retrieval work justified**: yes — manifest/config indexing is the dominant measured gap.
51. **Single largest retrieval gap**: manifests/config (go.mod, Cargo.toml) not indexed → config/module/package queries fail.
52. **Next task**: implement manifest indexing (REPODEX_GO_TEST_CONFIG_JOIN_V1 direction).

## Primary gap verdict

`CROSS_ARTIFACT_JOIN_GAP_DOMINATES` — but precisely: the gap is
**manifest/config content not indexed** (implementation→module/config cannot
join because the manifest facts are absent from the index), not a join-rule
deficiency between existing facts. Vocabulary morphology is a minor fixable
gap; true semantic synonymy is rare.

## Production decision

`REPODEX_RETRIEVAL_CHANGE_NOT_JUSTIFIED` — vocab bridge is opt-in (`vocab_bridge=on`), marginal; manifest indexing is the dominant measured gap and warrants a dedicated feature task.
