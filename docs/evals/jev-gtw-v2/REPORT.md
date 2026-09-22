# Jev V2 + Native Agent GTW Benchmark — Report

```text
JEV_V2_NATIVE_AGENT_GTW_BENCHMARK_COMPLETE
```

RepoDex base `ab9038f` → final `ddf4520` (clean). GTW pinned `f09d083` worktree.
Resolved model `jev-1.13.0` on every call (no drift). 400 retrieval executions,
403 Jev calls, 0 fallback/error/429. Spec sha `01ed9279`/gold `a721a537`.

## PART I — Retrieval V2 (full 400-run benchmark)

### Quality matrix (nDCG@10)

| level | A base | B query | C rerank | D full |
|-------|--------|---------|----------|--------|
| L1    | 0.085  | 0.077   | **0.256**| 0.233  |
| L2    | 0.271  | 0.153   | **0.394**| 0.178  |
| L3    | 0.316  | 0.086   | **0.396**| 0.064  |
| L4    | 0.371  | 0.097   | 0.362    | 0.090  |
| **all** | **0.261** | **0.103** | **0.352** | **0.141** |

Overall: hit@1 A .15 / B .24 / C .28 / D .28; hit@10 A .40 / B .25 / C .50 / D .28;
MRR A .247 / B .245 / C .364 / D .280; PathRecall@20 A .358 / C .358.

### Wins/ties/losses vs A
- **B: 0/6/14** — the seed-first query role is a net REGRESSION.
- **C: 12/3/5** — rerank-only remains the clear winner.
- **D: 2/3/15** — full mode is worse than C (query-role damage not recovered).

### V2 query role — now reachable but harmful
V2 made non-find plans reachable: intents = {find 31, related 62, callers 7}
(in B). **L2-Q2 correctly chose `callers(ensureWorkerActionableSlot)` conf .91**
(the critical §17 test passes). But binding plans to lexical seeds collapses
coverage: the top-5 seeds are often *test functions* (`TestTSK…`,
`TestProjectOperationalStatus…`), so `related/callers(test_fn)` narrows results
to the test's neighborhood instead of the implementation. Net nDCG B 0.103.

### V2 rerank — guard works
`so_rerank`: 131 used, 69 `not_needed_single_result`/`exact_match`/`exhaustive`.
**C2 `TaskExecutionDispatch` stays rank #1** (`not_needed_exact_match`) — the V1
1→6 regression is fixed. Rerank calls dropped 200→131. top-10 Jaccard C .941.

### Batching experiment (R1=1×20 vs R2=2×10, 8q×3r)
Identical quality (nDCG .591 vs .590) but **R1 uses ~18% fewer input tokens
(1725 vs 2093) and half the calls** → 1×20 is strictly better; confirmed as the
primary strategy. Both far under 28k soft budget.

### Latency / cost (retrieval)
E2E p50: A 590 / B 1353 / C 1346 / D 1369 ms. Per-call ~726ms query, ~743ms rerank.
$/q: B $0.000045, C $0.000074, D $0.000068 → /1M ≈ $45/$74/$68.

## PART II — Real-Agent (reduced-scale zero-context)

24 fresh `run_subagent` agents (subagent_explore; cannot run RepoDex). All
produced correct, source-cited answers. **All 24 correct (RequiredFactRecall
~1.0 across every treatment).**

Mean self-reported tool calls:

| treatment | mean calls | vs E0 |
|-----------|-----------|-------|
| E0 native | **13.7** | — |
| E1 RepoDex(A) | 21.0 | +53% WORSE |
| E2 +rerank(C) | 14.7 | +7% worse |
| E3 +full(D) | 18.3 | +34% worse |

Per-question tool calls (E0/E1/E2/E3): L1-Q4 16/12/12/13 · L1-Q5 15/30/9/13 ·
L2-Q1 12/26/17/18 · L2-Q3 16/13/10/10 · L3-Q2 6/9/16/29 · L4-Q1 17/36/24/27.

**Headline: the ~33KB noisy RDX1 packet did NOT reduce real Agent work on this
corpus — it often increased it.** RepoDex helps on focused-symbol questions
(L2-Q3, L1-Q4) but hurts broad architectural questions (L1-Q5, L2-Q1, L3-Q2,
L4-Q1) where test-heavy seeds send the agent down more paths. Correctness never
degraded, but efficiency did. The retrieval nDCG gain does NOT translate into
end-to-end Agent savings here.

## Safety / invariants
- C1 zero-result `total=0` preserved (roles `not_needed`). PASS.
- C2 `TaskExecutionDispatch` rank #1 preserved via exact-match guard. PASS.
- C3 exhaustive A==C identical (seeds+related+total+complete+evidence). PASS.
  (In D the query role legitimately picks a different plan → different set; that
  is plan-selection, not corruption.)
- C4 candidate-evidence edges preserved identical A↔D. PASS.
- All §110 invariant counters = 0. Source-scanning invariant intact (queries use
  persisted artifacts, no reparse).

## Verdict
- **Deterministic RepoDex (E1) does not currently pay for itself end-to-end**:
  it raised mean Agent tool calls (+53%) without hurting correctness.
- **Jev adds no agent value over RepoDex** (E2/E3 also ≥ E0).
- **Selective rerank (C) is the only net-positive Jev mode for *retrieval***
  (nDCG +0.09, exact-match regression fixed) — but that retrieval gain did not
  reduce real Agent effort in this sample.
- **Seed-first query role (B/D) is harmful** — keep it off; it needs
  implementation-kind-aware seed filtering (downweight test files) before it's
  viable.

## Caveats
Agent phase is reduced-scale (24/240) and self-reported-tool-metrics only —
`run_subagent` exposes no tool-trace or token usage, so token/byte metrics and
repetition variance are unavailable. Full product-decision status is withheld.
Raw traces `/tmp/repodex-jev-gtw-v2/raw/`; docs `docs/evals/jev-gtw-v2/`.
