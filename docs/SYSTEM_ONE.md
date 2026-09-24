# Optional System One Integration

RepoDex is fully usable with **no System One** — that is the canonical product.
System One is a replaceable, **bounded decision accelerator**: it may influence
ordering/advice but never repository truth. It is a decision model, **not** a
generative LLM.

```text
user query ──[System One: query advice]──> deterministic RepoDex
                                              │
                                          logical results
                                              │
                                [System One: rerank scoring]
```

## The boundary (what System One may NOT do)

It must never create/modify repository facts, graph edges, candidate targets,
`FACT`/`CANDIDATE` evidence, `candidate_set_id`, the total enumeration,
completeness, or NoCandidate/OutOfScope state. It may only influence
priority / ordering / bounded strategy selection.

## `system-one-v1` protocol

Request `{protocol, model, state, questions}` over synchronous HTTP(S) JSON.
`state` is JSON (RepoDex emits objects). Questions are keyed by application id
and typed:

- `choice` — `{instructions, criteria:{option:desc}}` → `{choice,probabilities,confidence}`
- `score`  — `{instructions, criteria:[ordered levels]}` → `{score,legend,probabilities,confidence}`
- `noul`   — `{instructions[, criteria]}` → `{noul}` (the probability is the answer)

Answers are validated (finite numbers, sane ranges, choice∈offered options);
harmless unknown provider fields are tolerated. `provider`/`jev` is **not**
part of the contract — `protocol`/`url`/`model`/`auth` are.

## Roles

- **`query`** — a bounded *advisor* over RepoDex-built alternatives (intent
  options valid for the plan). It picks one; it never emits a `QueryPlan`.
  Only applies when the intent is *default* — explicit `--intent`/`--target`/
  `--exhaustive` always win.
- **`rerank`** — `score`s logical query results in bounded batches
  (`RERANK_BATCH=16`) and reorders them only: same result set, deterministic
  tie-break, transaction-like all-or-nothing per run, candidate sets stay
  atomic.

## Fallback (deterministic, always)

Any failure — disabled, bad config, bad key, timeout, connection, 429, 5xx,
invalid JSON, invalid/unknown answer — falls back to the deterministic
baseline. Ordering after fallback equals the deterministic order; result
identities are unchanged. `system-one status` never touches the network;
`system-one probe <model>` sends one minimal `noul` question.

## Provenance

`QueryResult.so_query`/`so_rerank` ∈ `{disabled, used, fallback}` (+ model
name/protocol in diagnostics). Exposed in JSON (`so_query`/`so_rerank`) and as
additive RDX1 `S so_query=… so_rerank=…` keys (omitted when disabled — no
version bump, no evidence change).

## Determinism contract

- Disabled: byte-stable deterministic query output.
- Enabled: repository truth/result *identities* stay bounded by deterministic
  RepoDex; only ordering/advice may vary with provider decisions.

## CLI

```bash
reposuite-repodex config system-one.enabled true
reposuite-repodex config system-one.models.jev.url https://api.typesafe.ai/v1/systemone
reposuite-repodex config system-one.roles.query jev
reposuite-repodex system-one status          # offline: validate config + role bindings
reposuite-repodex system-one probe jev       # one protocol request (not a quality test)
```

See `docs/CONFIGURATION.md` for the schema and `docs/SYSTEM_ONE_LIVE_EVAL.md`
for the live-evaluation plan. No live call is made by tests or default builds.
