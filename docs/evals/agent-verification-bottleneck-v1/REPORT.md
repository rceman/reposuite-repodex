# Agent Verification Bottleneck V1 — report

## §51 — required answers

1. **What caused the verification read**: RepoDex supplied path/locator/range/relations but not the exact source bytes — the Agent then read the file to confirm.
2. **CurrentSourceWitness**: typed record {scope, view, path, role, evidence_class, entity, current_file_digest, byte/line range, exact source bytes, complete_for_range, truncated, provenance}.
3. **Current**: bytes sliced from the live file under the validated view's `canonical_root` only when `snapshot_id(current)==snapshot_id(index)`.
4. **Exact binding**: digest computed from the same bytes the range slices — never hash-A/read-B.
5. **Old source → witness**: never — historical/stale digest mismatch yields `unavailable`, not replayed bytes.
6. **Roles**: declaration, declaration_header, body, relation_occurrence, test_declaration, config_occurrence (decl/body implemented).
7. **No safe range**: navigation evidence only; no invented range.
8. **Limits**: ≤4 witnesses, ≤2048 B each, ≤8192 B total.
9. **Truncation**: `truncated=true`, `complete_for_range=false` on a bounded partial.
10. **Truncated body as complete**: no.
11. **Witness bytes counted as exposure**: yes (actual content).
12. **Path/range as exposure**: no — navigation metadata.
13. **FACT/CANDIDATE preserved**: yes — exact bytes never upgrade candidate.
14. **Ambiguous → guessed witness**: no.
15. **Relation witnesses**: occurrence/site range where the fact provides one (call edges with ranges); not manufactured.
16. **Test/config witnesses**: bounded current source for the selected test decl/config range.
17. **Reparsing for witness**: 0 — ranges come from the existing index snapshot.
18. **Witness p50/p95**: ms-scale bounded file reads + digest check.
19. **Total query overhead**: bounded reads on top of ~45 ms warm query.
20. **96-session benchmark**: complete — all 96 ran.
21. **N/A** (ran fully).
22. **W0/W1 correctness**: 47/48 vs 48/48 — non-inferior.
23. **Overlapping verification reads**: W0 83%, W1 83% — unchanged.
24. **Total reads**: ~equal (steps 10.4 vs 10.3).
25. **Searches**: not separately exposed (steps proxy).
26. **Tool calls**: 10.4 / 10.3.
27. **Input tokens**: 96603 / 97555 (+1%).
28. **Output tokens**: 991 / 935.
29. **Initial packet/witness bytes**: W1 adds ~31B–6KB witnesses.
30. **Later unique/cumulative source bytes**: not separately metered (agent-side reads unchanged).
31. **Time to primary evidence**: not separately instrumented.
32. **Time to sufficient evidence**: same.
33. **Agent wall**: W0 19.8 s, W1 18.2 s.
34. **Combined wall**: + ~45 ms RepoDex.
35. **% W1 witnesses reread**: 83%.
36. **Exact same evidence reread**: 83% (agent re-reads the files the witness already supplied).
37. **Median delay before reread**: first read occurs early (~first agent step) in both arms.
38. **Break-even**: no — witness bytes added input cost without removing the read.
39. **Exact/simple cohort**: W0 65.5K in/9.9s vs W1 80.3K/10.5s — W1 cost more, no work saved.
40. **Implementation/body**: ~equal (75.0K vs 75.9K).
41. **Relationship**: ~equal (114.5K both; wall 26.7 vs 29.4).
42. **Connector**: W1 lower wall (17.8 vs 28.0) — driven by a W0 outlier; tokens ~equal.
43. **Test/config**: ~equal.
44. **Any cohort regressed**: none materially; ambiguous/exact cost slightly more on W1.
45. **W2 triggered**: no — the reread threshold was met but W2 would only test provenance persuasion, which §38 forbids as the mechanism.
46. **W2 provenance effect**: not run.
47. **Verification is the bottleneck**: yes — the Agent re-reads supplied evidence regardless (83%).
48. **Exact source delivery reduces it**: no — Case C.
49. **Worth retaining**: yes as a safe opt-in capability (`source_witness=bounded`) — witnesses are exact + bounded + digest-verified — but it does not reduce agent work.
50. **Next bottleneck**: the Agent's own verification policy — it re-reads even exact current source. Not evidence selection, not missing bytes.

## Verdict

`CURRENT_SOURCE_WITNESS_DOES_NOT_REDUCE_REVERIFY`. The witness machinery is
correct, digest-bound, bounded, and current-view-safe — but the Agent verifies
independently regardless. Fifth consecutive finding that evidence delivery
cannot remove the Agent's verification work.

## Commits

`dabbc44` implementation; eval artifacts.
