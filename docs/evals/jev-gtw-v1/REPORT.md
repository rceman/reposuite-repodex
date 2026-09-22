# Live Jev GTW Benchmark V1 — Report

```text
LIVE_JEV_GTW_BENCHMARK_COMPLETE
JEV_VALUE_ASSESSMENT_READY
```

Corpus GTW@f09d083 · resolved model `jev-1.13.0` (configured `jev-latest`) ·
400 RepoDex executions · 600 Jev API calls · **0 fallback / 0 error / 0
rate-limit** · specs sha `01ed9279`/`a721a537`.

## Quality matrix (nDCG@10)

| level | A base | B query | C rerank | D full |
|-------|--------|---------|----------|--------|
| L1    | 0.085  | 0.085   | **0.256**| 0.256  |
| L2    | 0.271  | 0.271   | **0.394**| 0.396  |
| L3    | 0.316  | 0.316   | **0.396**| 0.398  |
| L4    | 0.371  | 0.371   | 0.360    | 0.358  |
| **all** | **0.261** | **0.261** | **0.351** | **0.352** |

Overall: hit@1 0.15→0.26/0.27, hit@5 0.35→0.49/0.50, hit@10 0.40→0.50,
MRR 0.247→0.352/0.358. Primary recall/full-coverage unchanged (membership is
never altered — rerank only reorders).

## Per-question nDCG@10 (A / B / C / D)

| Q | A | B | C | D |
|---|---|---|---|---|
| L1-Q1 | .000 | .000 | .000 | .000 |
| L1-Q2 | .000 | .000 | .000 | .000 |
| L1-Q3 | .000 | .000 | .000 | .000 |
| L1-Q4 | .000 | .000 | **.784** | **.784** |
| L1-Q5 | .425 | .425 | .498 | .498 |
| L2-Q1 | .626 | .626 | **.770** | .764 |
| L2-Q2 | .338 | .338 | .372 | .371 |
| L2-Q3 | .171 | .171 | **.547** | .541 |
| L2-Q4 | .110 | .110 | .160 | .161 |
| L2-Q5 | .109 | .109 | .119 | .144 |
| L3-Q1 | .090 | .090 | **.551** | .545 |
| L3-Q2 | .283 | .283 | .407 | .409 |
| L3-Q3 | .220 | .220 | .082 | .088 |
| L3-Q4 | **.841** | .841 | .666 | .685 |
| L3-Q5 | .145 | .145 | .272 | .263 |
| L4-Q1 | .608 | .608 | .637 | .635 |
| L4-Q2 | .364 | .364 | .348 | .351 |
| L4-Q3 | .032 | .032 | .211 | .211 |
| L4-Q4 | .539 | .539 | .329 | .332 |
| L4-Q5 | .315 | .315 | .273 | .263 |

## Wins / ties / losses vs baseline

- **B vs A: 0 / 20 / 0** — the query role was a complete no-op.
- **C vs A: 12 / 3 / 5** — degraded: L3-Q3, L3-Q4, L4-Q2, L4-Q4, L4-Q5.
- **D vs A: 12 / 3 / 5** — identical to C (query role added nothing).

## Query role — no measurable value here

For every one of the 100 query-role calls, Jev chose `find`. Because free-text
queries have no `--target`, only `find`/`related` are valid bounded options —
and Jev always picked `find`, so B ≡ A byte-for-byte. `callers`/`callees` are
never offered (they need a resolved target). **The query role added zero and
can never reach callers/callees on a free-text query.** intent accuracy vs
acceptable = 95/100 (the 5 misses = L2-Q2 wanted `callers`, unreachable).

## Rerank role — real but mixed value

Rerank lifts nDCG@10 +0.09 and MRR +0.10 overall by promoting gold items that
ranked mid-list. It **helps L1/L2/L3** (biggest: L3-Q1 .09→.55, L2-Q3
.17→.55, L1-Q4 .00→.78) but **hurts where the baseline already led with the
exact symbol** — worst L3-Q4 (.841→.666) and L4-Q4 (.539→.329), and the exact
`TaskExecutionDispatch` control was demoted 1→6. Stability top-10 Jaccard
≈0.94 across reps (near-deterministic, slight model variance).

## Latency (ms)

| mode | e2e p50 | p95 | max | SO wall p50 |
|------|---------|-----|-----|-------------|
| A | 553 | 574 | 616 | 0 |
| B | 1275 | 1341 | 1400 | 718 |
| C | 2000 | 2111 | 4237 | 1440 |
| D | 2762 | 5229 | 5434 | 2200 |

Per-call: query p50 721 / p95 804; rerank p50 725 / p95 810 (max ~3.3s tail).
Fixed ~0.55s of e2e is process+34MB-graph load (identical across modes).
19 samples had a negative-e2e clock artifact (VM clock skew) — excluded from
latency percentiles, correctness unaffected.

## Tokens & cost (list $0.042/1M in, $0 out)

| mode | calls/q | in tok/q | out tok/q | $/q | $/1k | $/1M |
|------|---------|----------|-----------|-----|------|------|
| A | 0 | 0 | 0 | $0 | $0 | $0 |
| B | 1.00 | 430 | 31 | $0.000018 | $0.018 | $18.07 |
| C | 2.00 | 2120 | 298 | $0.000089 | $0.089 | $89.04 |
| D | 3.00 | 2550 | 329 | $0.000107 | $0.107 | $107.11 |

In D, rerank is ~83% of Jev cost ($0.0089 vs query $0.0018 over the run).
No provider-reported cost field; computed at list price only.

## RDX1 / RepoDex-output tokens

A vs D on the same query: **344 F + 327 R records identical**; bytes differ
only by the 33-byte `S so_query/so_rerank` summary lines. System One never
changes emitted facts/relationships for the same limit.

## Safety controls

- **C1** zero-result `zzzxxyy`: `total=0 shown=0 complete=1`, no fabricated
  result. PASS.
- **C2** exact `TaskExecutionDispatch`: still present (membership preserved)
  but rerank demoted it 1→6 — a real ordering regression on exact-symbol
  queries.
- **C3** exhaustive `validateTaskDependencies`: A↔D identical seed+related
  sets, same `total`/`complete`/evidence. PASS.
- **C4** uncertainty (GTW `MultipleCandidates`=0, so used `call_candidate`
  edges): A↔D identical candidate edges + evidence classes; no FACT/CANDIDATE
  or `candidate_set_id` change. PASS.

## Interpretation

- **L1** exact lookup: baseline is weak on verbose natural-language queries;
  rerank helps when it can recognize the gold file (L1-Q4) but can't fix
  zero-overlap (L1-Q1/Q2/Q3 stay 0 — model can't surface absent seeds).
- **L2** relationship: biggest clean win — rerank surfaces the right
  relationship files.
- **L3** multi-hop: net win but two regressions (rerank demoted already-correct
  top hits on L3-Q3/L3-Q4).
- **L4** architecture/invariant: roughly flat, slight regression — baseline
  already ranks the right files and rerank adds noise.

## Recommendation

**Keep System One available but disabled by default; rerank shows real value
on L2/L3 relationship/multi-hop queries but is risky on exact-symbol and
already-correct queries; the query role is not worth enabling as implemented
(it never deviates and can't reach callers/callees without a target).** The
clearest value is `rerank-only` (C) — nearly all of D's gain at ~2/3 the cost
— but a confidence/no-op guard would be needed to protect the cases it hurts.

## Uncertainty

Single corpus, 20 questions, one model version; 5 degraded vs 12 improved is a
small margin; rerank non-determinism (Jaccard .94) means per-query outcomes
vary slightly run-to-run.
