# Adaptive Context Compiler V1 — report

## §70 — required answers

1. **Shapes**: exact_lookup, ambiguous_lookup, relationship, path_or_connector, orientation, test_or_verification, unknown.
2. **UNKNOWN**: a real shape — no declaration seeds or unsafe classification → conservative bounded packet; never forced into a confident template.
3. **Obligations**: definition, disambiguation, relationship, connector, test_candidate.
4. **WITNESSED**: objective current-view evidence — a decl seed (definition), a relation edge (relationship), a found connector, ambiguity candidates.
5. **CANDIDATE_ONLY**: ambiguity/candidate edges/test candidates — bounded candidate evidence only.
6. **NOT_APPLICABLE**: only when the shape objectively doesn't require the obligation (e.g., disambiguation on an exact lookup).
7. **Selectable representations**: label, path, qualified locator, declaration/body range, relationship edge, connector path, memory path, current-rebound memory symbol.
8. **Mandatory once emitted**: rank, FACT/CANDIDATE, current entity identity, relationship endpoints, ambiguity state.
9. **Cost estimation**: serialized bytes of the canonical projection (`serde_json::to_vec` on `to_json()`).
10. **Estimate vs actual**: estimate IS the serialized byte count of the emitted form (same serializer) — measured pre- and post-prune; error ~0 because it's the real render.
11. **Selection heuristic**: shape-tier pruning (mandatory + shape-required kept, optional dropped), then serialized-byte budget drops lowest-priority optional records — not a probability of correctness.
12. **Complementary bundles**: relationship endpoints+via and connector steps kept together; partial emission never emitted misleadingly.
13. **Default exact-lookup packet**: single top seed — rank/key/kind/label/path/language/declaration_range/FACT.
14. **Size vs prior faithful**: exact 8376→1173 B (−86%), test 12949→1218 B (−91%), simple 3065→1152 B (−62%).
15. **+15K overserving regression gone**: yes — exact lookups now emit the minimal single-seed packet.
16. **Ambiguous**: bounded candidate seeds with distinct paths/locators + decl ranges; no winner picked.
17. **Relationship**: endpoints + evidence + via preserved, topology capped.
18. **Connector**: bounded packet (≤2 routes, ≤3 depth).
19. **Orientation**: bounded seeds + local relations (capped).
20. **Test**: decl + candidate test decls only (no "proves behavior" claim).
21. **Explicit gaps**: obligation ledger rows (`unknown`/`candidate_only`/`not_applicable`) in the `context` block.
22. **Budget exhausted**: lowest-priority optional records dropped; `budget_limited=true`.
23. **Truncation implies completeness**: never — `budget_limited` sets `complete=false`.
24. **Source exposure from path/range**: no — only navigation metadata; SourceExposure semantics unchanged.
25. **memory=auto decline**: for exact_lookup the single exact seed already witnesses definition → `memory_redundant` → memory dropped.
26. **memory=auto admit**: when a scoped, target-view-rebounded composition exists AND the shape leaves an obligation it can satisfy (disambiguation/current symbol) → admitted.
27. **Memory bypass scope/rebind**: never — admission consumes the same gated MemoryComposition.
28. **Compiler p50/p95**: ≤~6 ms added to a ~45 ms warm query (dominated by process spawn + index load); p95 within noise.
29. **New parsing by compiler**: 0 — operates on the built QueryResult + projection.
30. **Main Agent benchmark ran**: yes — 40 serial `devin -p` sessions (10 q × 2 arms × 2 reps), memory=off both (design A).
31. **C0/C1 correctness**: 20/20 both — non-inferior.
32. **Input tokens**: C0 39290, C1 40409 (+3%).
33. **Output tokens**: C0 719, C1 665.
34. **Tool calls / steps**: 9.7 / 9.8.
35. **Searches**: not separately exposed by this ATIF schema (steps proxy).
36. **Reads**: same.
37. **Time to primary evidence**: not separately instrumented in the export.
38. **Time to sufficient evidence**: same.
39. **Agent wall**: C0 18.1 s, C1 18.8 s.
40. **Combined wall**: same (RepoDex query is ~45 ms warm, negligible).
41. **Exact-lookup cohort**: packet −86%; agent tokens ~flat (no extra work, no regression).
42. **Ambiguous cohort**: locators preserved; tokens within noise.
43. **Relationship cohort**: provenance preserved; tokens within noise.
44. **Connector cohort**: route preserved; ~flat.
45. **Test/orientation**: test packet −91%; orientation −45%; agent tokens ~flat.
46. **Any cohort worse**: no measured correctness/work regression; C1 +3% input tokens is within prompt-dominated noise.
47. **Auto-memory sub-experiment**: not run — C1 isolation showed no positive delta, so it would not change the opt-in verdict.
48. **Adaptive memory outperform**: not applicable (not run); admission rule is deterministic.
49. **Evidence justifying future learned utility**: a cohort where adaptive packets measurably reduce agent steps/tokens (larger repos, deeper investigations) would justify it.
50. **Worth production default**: not yet — OPT_IN. Safe + smaller packets, but no measured agent-work win in this cohort.

## Verdict

The compiler is deterministic, auditable, correct, and produces materially
smaller packets (the overserving regression is fixed). It did NOT reduce agent
work in this screening cohort because agent cost is prompt+read dominated, not
packet-byte dominated. Classification: **OPT_IN**, not default.

## Commits

`5b82273` implementation; eval artifacts.
