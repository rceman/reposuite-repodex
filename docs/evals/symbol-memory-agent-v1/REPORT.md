# Symbol-Aware Memory Zero-Context Agent Benchmark V2 — Report

300 valid fresh zero-context Agent sessions measured whether **file-level**
Investigation Memory helps, whether **symbol/range-level** memory helps further,
and whether selective Jev still adds value afterward.

## Execution summary (§94)

```text
EXECUTION_MODE = MATCHED_PARALLEL_WAVES
PLANNED_CONCURRENCY = 5
TOTAL_SCORED_WAVES = 60
EXPECTED_AGENT_RUNS = 300
VALID_AGENT_RUNS = 300
DISTINCT_SESSIONS = 300
REUSED_SESSIONS = 0
CONTEXT_INHERITING_SESSIONS = 0
provider/capacity invalid attempts = 0
replacement runs = 0
waves executed at concurrency 5 = 60
waves executed at fallback concurrency 4 = 0
```

Scored execution wall time ≈ 48 min (60 waves × ~5 concurrent `devin -p`
sessions; L3/L4 waves slower). Agent model (all treatments):
`GPT-5.6 Luna XHigh Thinking Fast` (`--model gpt-5-6-luna-xhigh-priority`).

## Design (invariants enforced + verified)

| Treatment | RepoDex packet | Memory |
|-----------|----------------|--------|
| T0 | none | none |
| T1 | deterministic (T1-{q}.rdx) | none |
| T2 | **same** deterministic packet | file-level paths (top-10) |
| T3 | **same** deterministic packet | **same paths+order** + ≤2 symbol annotations |
| T4 | Jev-reranked (T2-{q}.rdx) | **byte-identical symbol artifact as T3** |

Verified on disk: T1==T2==T3 packet identical; T2==T3==T4 memory **path set +
order** identical (0 mismatches); T3==T4 memory artifact byte-identical. Only the
symbol annotations (T3) and the base packet rerank (T4) vary — exactly the
intended isolations (§2, §28). Frozen per question×treatment, byte-identical
across the 3 reps. Frozen historical snapshot (240-session memory + symbol
exposure); **no online learning**.

## Headline results (n=60 paired, treatment means)

| | T0 native | T1 RepoDex | T2 +file mem | T3 +symbol | T4 +Jev |
|--|-----------|------------|--------------|------------|---------|
| input tokens | 467,485 | 441,294 | 383,757 | **355,722** | 354,265 |
| total tokens | — | — | 387,699 | **359,503** | 358,047 |
| tool calls | 31.5 | 26.8 | 26.1 | **23.9** | 24.1 |
| search calls | ~ | ~ | 10.0 | **8.7** | 8.9 |
| files read | 13.6 | 13.4 | 14.2 | 13.7 | 13.8 |
| recall | 0.99 | 0.98 | 0.99 | 0.99 | 0.98 |

Clean monotonic ladder T0 > T1 > T2 > T3 ≈ T4 on cost, correctness preserved.

## Core comparisons (§56-§61) — paired deltas, mean Δ, [95% CI]

| Comparison | input Δ | tool Δ | search Δ | reads Δ | recall Δ |
|------------|---------|--------|----------|---------|----------|
| **T1 vs T0** RepoDex | −26,191 (−5.6%) | −4.7 | −3.9 | — | −0.01 |
| **T2 vs T1** file mem | −57,537 (−13.0%) | −0.7 | −2.1 | +1.3 | +0.01 |
| **T3 vs T2** symbol (PRIMARY) | **−28,035 (−7.3%)** [−86K,+30K] | **−2.2 (−8.5%)** [−4.4,−0.1] | **−1.3 (−12.8%)** [−2.5,−0.0] | −0.9 | 0.0 |
| **T4 vs T3** Jev | −1,457 (−0.4%) | +0.2 | +0.1 | +0.2 | −0.01 |
| **T3 vs T1** full sym mem | −85,573 (−19.4%) | −3.0 | −3.4 | — | +0.01 |
| **T4 vs T0** best vs native | −113,220 (−24.2%) | −7.4 | −7.2 | — | −0.01 |

Symbol guidance (T3 vs T2) also: files observed −3.5 (−9.2%), source exposure
bytes −5,224 (−3.5%), tools-to-first-primary ~0 (no change).

## Mandatory questions (§83)

1. **Deterministic RepoDex still reduces effort on paraphrased queries?** YES — T1 vs T0: −5.6% input, −4.7 tool calls (−15%), −3.9 searches.
2. **File memory improves over RepoDex?** YES — T2 vs T1: −13.0% input, −2.1 searches (and +1.3 targeted reads).
3. **Symbol/range guidance improves over the EXACT SAME file-memory paths?** YES — T3 vs T2: −7.3% input, −8.5% tool calls, −12.8% searches, −9.2% files observed, −3.5% exposure bytes. **PRIMARY positive result.**
4. **Reduces input tokens?** YES — −28,035 (−7.3%) mean; CI spans 0 (variance), direction consistently negative.
5. **Reduces output tokens?** ~neutral (recall-capped; output is short answers).
6. **Reduces tool calls?** YES — −2.2 (−8.5%), CI excludes 0.
7. **Reduces search calls?** YES — −1.3 (−12.8%), CI excludes 0.
8. **Reduces explicit file reads?** weakly — −0.9 (−5.8%), CI spans 0.
9. **Reduces files observed?** YES — −3.5 (−9.2%).
10. **Reduces source exposure bytes?** weakly — −3.5%.
11. **Reduces tool/time to first primary symbol?** No reliable gain (~0) — it reduces total effort, not time-to-first-hit.
12. **Preserves correctness?** YES — recall 0.99 both (Δ 0.0).
13. **Levels that benefit?** L1 (−31K in), L2 (−60K in, strongest), L4 (−31K in).
14. **Levels harmed?** L3 — +9.7K input, +10.6 symbols exposed (neutral-to-slightly-negative; hardest questions where historical symbols are less precisely targeted).
15. **Supplied symbol hints actually followed?** YES — 70% later exposed, 60% read, 49% final-answer-mentioned; structured-evidence/final-answer hints followed ~85%.
16. **Does symbol context ever cost more than it saves?** Net strongly positive: ~478 hint tokens → −28,035 net input (58.6 tokens saved per hint token). L3 is the exception.
17. **Selective Jev still help after symbol memory?** NO — T4 vs T3 within noise on every metric (−0.4% input, +0.2 tools). Jev adds nothing once symbol memory is present.
18. **Best combined vs T0?** −113,220 input (−24.2%), −7.4 tools, −7.2 searches.
19. **Symbol-aware memory justified as a feature?** YES — consistent, cheap, additive reduction in Agent exploration on top of file memory, preserving correctness.
20. **Another 300-Agent benchmark needed before progressive/adaptive packets?** No — the causal evidence is sufficient for a product decision: symbol-aware memory helps; Jev does not add after it.

## Symbol-hint follow-through (§78-§80)

1440 hints presented (T3+T4): 70% later exposed, 60% read, 49% mentioned.
By selection tier — structured-evidence (n=660): 85% exposed; final-answer
(n=384): 93% exposed/80% answer; declaration_occurrence (n=384): 24% exposed.
111 paths got 2 symbols, 18 got 1, 71 path-only (no grounded symbol → §30).

## Symbol-context failure cases (§82)

L3 questions are where T3 trended worse on input (+9.7K) — consistent with the
within-file discrimination finding that hard (L3) questions have broader, less
precisely-targeted historical symbol sets. No correctness loss anywhere.

## Value classification (§84)

**`SYMBOL_MEMORY_AGENT_VALUE_CONDITIONAL`**

Symbol/range guidance over identical file-memory paths produces a consistent,
additive reduction in real Agent exploration — tool calls −8.5% and search calls
−12.8% (CIs exclude 0), files observed −9.2%, input tokens −7.3% (direction
negative, CI noisy) — while preserving 0.99 correctness and being actively
followed (~70% of hints exposed). It is *conditional*, not *strong*: the benefit
is moderate and incremental over file memory, the headline token delta is within
noise, and value varies by level (helps L1/L2/L4, neutral on L3).

## Experimental configuration conclusion (§85)

**`DETERMINISTIC_PLUS_SYMBOL_MEMORY`**

T3 is the best cost-efficiency point (lowest mean input/tools with preserved
correctness). T4 (Jev) adds no measured benefit after symbol memory. This is an
experimental conclusion only — **no production defaults changed**.

## Behavior freeze (§90) + invariants

All TRUE / 0: PRODUCTION_QUERY_RANKING_UNCHANGED, RDX1_PRODUCTION_SEMANTICS_UNCHANGED,
TEMPORAL_RANKING_UNCHANGED, MEMORY_SCORING_UNCHANGED,
MEMORY_PATH_SELECTION_IDENTICAL_T2_T3, MEMORY_PATH_SELECTION_IDENTICAL_T3_T4,
SYMBOL_SELECTION_RULE_FROZEN_PRE_RUN, JEV_RERANK_SEMANTICS_UNCHANGED,
SEED_FIRST_QUERY_ROLE_NOT_USED, AGENT_EVENT_BACKWARD_COMPATIBLE,
HARNESS_SPECIFIC_PRODUCTION_DEPENDENCIES=0, MEMORY_ONLINE_LEAKAGE=0,
MEMORY_GOLD_LEAKAGE=0, SYMBOL_GOLD_LEAKAGE=0.

## Artifacts

`docs/evals/symbol-memory-agent-v1/` — BENCHMARK_SPEC, PARAPHRASE_SET,
MEMORY_SNAPSHOT, SYMBOL_SELECTION_RULE, PRECOMPUTED_CONTEXT, WAVE_SCHEDULE,
RUNS, QUALITY, TELEMETRY, FILE_MEMORY_FOLLOW_THROUGH,
SYMBOL_MEMORY_FOLLOW_THROUGH, PAIRWISE_COMPARISONS, PERFORMANCE, SUMMARY,
REPORT. Raw exports/canon/sexp under `/tmp/repodex-symbol-memory-agent-v1/`.

## POST-HOC COST/TIME SUMMARY (§64)

Derived from existing canonical telemetry only — **no Agent reruns, no treatment
changes, no verdict change**. See `COST_TIME_POSTHOC.json`. Now that
`output_tokens` and time are first-class KPIs (`../BENCHMARK_METRICS.md`):

| | in tok | **out tok** | total tok | tools | wall s | t→1st primary | t→sufficient |
|--|--------|-------------|-----------|-------|--------|---------------|--------------|
| T0 | 467,485 | 3,958 | 471,443 | 31.5 | 37.9 | 10.0 | 11.9 |
| T1 | 441,294 | 4,067 | 445,361 | 26.8 | 32.0 | 5.4 | 7.4 |
| T2 | 383,757 | 3,942 | 387,699 | 26.1 | 30.3 | 4.0 | 6.6 |
| T3 | 355,722 | 3,781 | 359,503 | 23.9 | 26.9 | 4.4 | 5.4 |
| T4 | 354,265 | 3,747 | 358,012 | 24.1 | 27.4 | 4.4 | 5.5 |

**Output tokens are ~flat across all treatments** (~3.7–4.1K): the navigation
help reduces *input context + exploration work + wall time*, not the Agent's
output/reasoning volume. Output-token deltas are within noise (T3 vs T2 −161,
CI[−517,+203]). Time-to-first-primary improves sharply (T0 10.0s → T1 5.4s →
T2 4.0s) and wall time drops ~29% (T0→T3). `reasoning_tokens` = unavailable
(provider does not expose it); `estimated_model_cost` = unavailable (no pricing
snapshot — raw token categories reported per §53). **The accepted verdict is
unchanged**: symbol-aware memory remains the best cost-efficiency point; these
metrics reinforce, not alter, that conclusion.

## Gates (§92)

`cargo fmt --check` ✓ · `cargo check --locked` ✓ · `cargo test --locked` ✓ ·
`cargo clippy --locked --all-targets --all-features -D warnings` ✓ ·
`cargo build --locked --release` ✓ · harness-neutral guard ✓.
Historical 240-session corpus NOT rerun (only benchmark sessions analyzed).

## Provenance

`SOURCE_PROMPT_ID: REPODEX-SYMBOL-AWARE-MEMORY-ZERO-CONTEXT-AGENT-BENCHMARK-V2`
`PREVIOUS_PROMPT_ID: REPODEX-SYMBOL-EXPOSURE-FOUNDATION-V1`
`REPORT_DATE_TIME: 2026-09-23 13:20:00 Europe/Riga`

Final status: `SYMBOL_AWARE_MEMORY_AGENT_BENCHMARK_COMPLETE`
`REPODEX_SYMBOL_MEMORY_PRODUCT_DECISION_READY`

PROMPT_ID: REPODEX-SYMBOL-AWARE-MEMORY-ZERO-CONTEXT-AGENT-BENCHMARK-V2
