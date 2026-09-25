# Agent Evidence Protocol Campaign V1 — report

## §90 — session accounting

| experiment | sessions |
|---|---|
| A (delivery/provenance/channel) | 160 |
| B (breadth) | 0 — not triggered (containing-reread 7-14% < 25%) |
| C (structure) | 72 |
| D (read-protocol) | 0 — not triggered (A3 reread low) |
| confirmation | 0 — no winner |
| secondary replication | 0 — no second authorized harness |
| **TOTAL** | **232** (<= 480) |

## §91 — required answers

1. **What the 83% reread contained**: mostly *legitimate* reads of the file RepoDex identified (the agent uses the locator, then reads the file — expected). Reclassified: redundant exact rereads are actually **infrequent** (5-17%); the earlier figure was inflated by loose filename matching.
2. **Exact redundant fraction**: ~5-17% depending on arm.
3. **Containing/broader**: ~5-14%.
4. **Insufficient-witness fraction**: insufficient cases read more (10-25%) — justified.
5. **Immediate next action**: 35-45% answer with no tool (packet suffices); ~40% read the referenced file; <10% search.
6. **Median time to exact reread**: file-level only (no range telemetry).
7. **Median time to containing reread**: same limitation.
8. **A1 replicated exact-source result**: yes — equivalent packet, equivalent null.
9. **Provenance >=20pp**: no (A2 12% vs A1 12% — no improvement).
10. **Tool-result >=20pp**: no (A3 12%).
11. **Source bytes identical A1/A2/A3**: yes.
12. **Retrieval identical**: yes.
13. **Correctness change**: no (38-40/40 across arms; A1 marginally best).
14. **Most-changed A cohort**: none materially.
15. **B triggered**: no — containing < 25%.
16. **Why not**: broader-context demand below threshold.
17. **Enclosing context reduced containing reads**: n/a.
18. **Structural local context**: n/a.
19. **Breadth break-even**: n/a.
20. **Extra bytes from broader context**: n/a.
21. **Later bytes avoided**: n/a.
22. **C triggered**: yes (no winner + no breadth break-even).
23. **Structured decl altered reread**: no work reduction (C1 rr=0% but steps/tokens ~equal — noise).
24. **Relation/source structure**: no.
25. **D possible**: not triggered.
26. **Read-like framing**: n/a.
27. **Lowest sufficient-witness reread**: A0 (3%) — trivially, no witness to reread; among witness arms ~10-17%.
28. **Lowest tool calls**: A3 (9.9).
29. **Lowest source reads**: A3/A2 (fewest narrated reads, marginal).
30. **Lowest searches**: all ~equal.
31. **Lowest time-to-sufficient**: not separately instrumented.
32. **Lowest combined wall**: A2 (18.0s).
33. **Lowest input tokens**: A0 (79.5K) — witnesses add bytes.
34. **Lowest-reread = lowest cost**: no — A0's low reread didn't lower steps.
35. **Any treatment hurt correctness**: no (all >=38/40).
36. **Negative-control diligence failure**: none — insufficient-witness cases correctly read more.
37. **Screening winner**: no.
38. **Confirmation run**: no.
39. **Winner replicated**: n/a.
40. **Replication run**: no — no second harness.
41. **Agent-specific vs shared**: single agent (INCONCLUSIVE for generalization).
42. **Dominant mechanism**: AGENT_VERIFICATION_POLICY — the Agent's own read-to-ground workflow drives reads; delivery protocol/provenance/channel/structure do not alter it.
43. **Missing provenance the bottleneck**: no.
44. **Delivery channel**: no.
45. **Context breadth**: no.
46. **Source representation**: no.
47. **Intrinsic Agent verification dominates**: yes — but note the reads are largely *legitimate*, not redundant; agents often answer directly from the packet (35-45% no-tool-first).
48. **New production protocol justified**: no.
49. **Production behavior added**: none.
50. **SourceWitness opt-in**: yes, keep.
51. **Adaptive Context opt-in**: yes.
52. **Guarded Recipes opt-in**: yes.
53. **Utility Policy shadow-only**: yes.
54. **Continue optimizing evidence delivery**: no — six experiments now show the Agent's verification workflow is presentation-invariant; the marginal value is exhausted.
55. **Largest remaining bottleneck**: the Agent's own verification/grounding policy — it reads source it deems necessary regardless of how well RepoDex pre-delivers it. This is an agent-side, not RepoDex, property.
56. **Next step**: stop optimizing evidence delivery; either study the agent's verification policy boundary or consolidate the validated faithful-delivery into product integration.

## Mechanism verdict

`AGENT_VERIFICATION_POLICY_DOMINATES` — refined: not that the agent performs
*redundant* rereads (those are rare), but that its read-to-ground workflow is
inherent and presentation-invariant. No evidence-protocol variable — source
bytes, provenance, tool-result channel, breadth, or structure — materially
reduces investigation work because the reads are driven by the Agent's own
verification policy, not by what the packet lacks.

## Production decision

`REPODEX_EVIDENCE_PROTOCOL_CHANGE_NOT_JUSTIFIED` — no production evidence-
delivery change; all opt-in mechanisms stay as-is.

## Sessions: 232 / 480 budget
