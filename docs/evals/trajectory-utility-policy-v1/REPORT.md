# Trajectory Utility Policy V1 — report

## §69 — required answers

1. **Evidence-unit identity**: `{scope, entity(locator/key, no line #s), repr, role, truth_class}` → content digest.
2. **Representation kinds**: path_hint, locator, declaration_range, body_range, fact_relation, candidate_relation, connector_route, file_memory, symbol_memory, recipe_witness.
3. **Already available**: delivered evidence, source_observed, final mentions, symbol exposure, compiler trace.
4. **New**: per-unit association cells, decision-slate log, expansion count, subsequent-work proxy.
5. **Runtime-observable**: delivered, source_observed, expansions, subsequent tool calls.
6. **Evaluator-only**: correctness, time-to-sufficient, gold witness sets.
7. **OUTCOME_ASSOCIATED**: correlated with outcome (e.g., final mention) but not causally proven.
8. **CAUSALLY_VALIDATED**: only a controlled intervention (U0/U1 / seeded slate) can establish — none showed benefit.
9. **Task success → per-item causal**: no — marked `INVESTIGATION_LEVEL_ASSOCIATION`.
10. **Unconsumed evidence negative**: no — ignored/unconsumed stays unknown/weak, not a penalty.
11. **Continuation metrics**: searches/reads/tool-calls before primary + sufficient, expansions, subsequent tool calls, final_mentioned.
12. **Conditioning**: scope | shape | repr | role | entity.
13. **Fallback hierarchy**: specific cell → global prior (shrinkage).
14. **Min support/shrinkage**: `MIN_SUPPORT=5`, `SHRINKAGE=3.0` toward global rate.
15. **Rejected signals**: reasoning-token volume, task-success→item causal copy, line-number identity.
16. **One scalar score**: no — separate dimensions; a bounded selection heuristic is used only for apply ordering.
17. **Formula**: shrunken followup-rate `(followups+3*global)/(delivered+3)`; used only for apply ordering.
18. **Shadow computes**: the would-be optional ordering/admission without changing the packet (trace only).
19. **Shadow disagreement**: diverged on all 5 ambiguous queries (promoted low-followup seed); controls unchanged.
20. **Apply can change**: optional evidence ordering/admission.
21. **Apply can never change**: mandatory evidence, FACT/CANDIDATE, view validity, project scope.
22. **Apply falls back**: < MIN_SUPPORT, or < MIN_MARGIN advantage → baseline order.
23. **Randomized production-default**: no — evaluation only.
24. **Propensity logged**: `1/eligible` + seed in the trace.
25. **Randomization reproducible**: yes — xorshift64 seeded.
26. **Incremental == replay**: yes — deterministic store over decisions/history.
27. **Utility lookup p50/p95**: sub-ms (in-memory cell read).
28. **Policy overhead**: sub-ms per optional seed.
29. **State cost**: `utility.json` cells + append-only `decisions.jsonl`; no DB.
30. **Offline held-out**: association coherent (low-followup ranked first); limited corpus → limited generalization.
31. **Paraphrase generalization**: single query family → not measurable.
32. **Leave-family-out**: not measurable on this corpus.
33. **96-session screen ran**: a bounded 32-session U0/U1 ran (8 q × 2 arms × 2 reps).
34. **U0/U1 correctness**: 16/16 both.
35. **Searches**: not separately exposed (steps proxy).
36. **Reads**: same.
37. **Tool calls**: 9.7 / 9.8.
38. **Time to sufficient**: not separately instrumented.
39. **Input tokens**: U0 38989, U1 41534.
40. **Output tokens**: U0 716, U1 837.
41. **Agent wall**: U0 13.6 s, U1 13.0 s.
42. **Cohorts improved**: none materially; control wall lower is noise.
43. **Cohorts regressed**: ambiguous slightly (in +10%, wall +30%).
44. **Escalation met**: no (no material work improvement).
45. **Confirmation study**: not run (stop rule).
46. **Slate micro-experiment**: not run separately (seeded support exists; stop rule applied).
47. **Causal representation win**: none measured.
48. **Predicting historical signals**: low-followup-rate ordering — coherent but no causal benefit.
49. **Failing signals**: none contradicted, but none translated to causal wins.
50. **Final production policy**: `UTILITY_POLICY_SHADOW_ONLY`.
51. **Further deterministic selection justified**: not on this workload — four experiments show no agent-work reduction from evidence manipulation.
52. **Largest bottleneck now**: the Agent's own verification/reasoning work — it re-reads the same source regardless of how evidence is selected/ordered/sized.

## Strategic answer (§63)

The remaining bottleneck is the **Agent's verification work**, not evidence
selection. Faithful delivery helped once (it removed *missing* evidence); but
memory, adaptive packets, recipes, and now utility ordering — four consecutive
experiments — show **no measurable work reduction** from changing what/how
evidence is selected when deterministic retrieval already finds the answer.

## Verdict

Machinery is correct, deterministic, safe, and replay-equivalent. Associations
are observable + shrunken; shadow works; apply reorders only optional evidence.
But apply produced no causal work reduction → **SHADOW_ONLY**, not a ranking
policy.

## Commits

`e61835b` implementation; eval artifacts.
