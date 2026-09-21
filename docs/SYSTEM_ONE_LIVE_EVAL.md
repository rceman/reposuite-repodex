# System One Live Evaluation Plan

Do **not** run this until a real `TYPE_SAFE_API_KEY` is provisioned. This plan
evaluates System One for **added value**, not merely connectivity. All four
modes run over the **same** query corpus and corpora (tokio + Hugo).

## Comparison modes

| mode | query role | rerank role |
|------|-----------|-------------|
| baseline      | off      | off       |
| query-only    | jev      | off       |
| rerank-only   | off      | jev       |
| query+rerank  | jev      | jev       |

## Query corpus

Cover (not one anecdote): exact symbol queries, ambiguous names, multi-term
repository concepts, callers/callees intents, "all"/exhaustive requests,
queries with weak lexical overlap, and zero-result queries.

## Measurements per mode × query

- **result-set identity** — the seed/related key multiset must equal baseline
  (System One may not add/remove repository results).
- **ordering changes** — Kendall-style rank displacement vs baseline.
- **top-N usefulness** — labelled relevance of the top-N (a human/secondary
  judgement, recorded per query).
- **query latency** — end-to-end.
- **System One latency** — request/response time per role.
- **fallback rate** — fraction of calls that fell back to deterministic baseline.
- **request count** — # protocol requests per query (batching).
- **input usage** — provider-reported usage where available.

## Procedure

1. `repodex system-one probe jev` — confirms transport+auth+schema only.
2. Freeze a fixed query list; run baseline → record RDX1.
3. Re-run each mode; diff result identities + order + `so_*` provenance.
4. Verify invariants still hold (exhaustive totals, candidate sets, fallback).
5. Score added value: does rerank/query advice improve top-N usefulness
   without breaking any deterministic contract?

## Acceptance

System One adds value iff it measurably improves ordering/selection while
**result identities, totals, completeness and fallback remain deterministic**.
A provider that merely answers is not a win — improvement must be demonstrated.
