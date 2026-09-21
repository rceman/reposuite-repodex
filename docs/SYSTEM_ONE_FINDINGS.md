# System One integration findings

`SO-F###` — stable IDs. Categories: CONFIG/PROTOCOL/AUTH/QUERY_ROLE/
RERANK_ROLE/FALLBACK/COMPLETENESS/RDX1/PERFORMANCE/LIVE_READINESS.

## SO-F001 — CONFIG — canonical path + named models

`~/reposuite/repodex/config.toml`, `[system_one]` only. Named models +
per-role routing (`query`,`rerank`); the same model may serve both. Absent
file/absent table/`enabled=false` => disabled. `config` CLI does key get/set
on the raw TOML doc so partial edits are allowed; typed validation runs at
`status`/query time.

## SO-F002 — PROTOCOL — system-one-v1 is decision-shaped, not generative

`{model,state,questions}` with `choice`/`score`/`noul`. RepoDex builds the
option set; the model returns typed decisions. It is never asked to generate
a `QueryPlan` or repository facts.

## SO-F003 — PROTOCOL — bounded synchronous HTTP, no async runtime

`HttpSystemOneModel` uses `ureq` 2.x (sync, rustls). Adds ureq+rustls+ring+webpki
transitively — a deliberate, documented choice; no tokio/reqwest. Response
capped at 4 MiB; connect+read timeouts from `timeout_ms`.

## SO-F004 — AUTH — none/bearer/header, never logged

Tokens are set on the request only. They never appear in logs, errors,
diagnostics or RDX1/query provenance.

## SO-F005 — PROTOCOL — error taxonomy feeds fallback

Timeout/Connection/Auth/RateLimited/Server/InvalidJson/InvalidResponse/
UnsupportedProtocol/Config — each maps to deterministic fallback, no panic.

## SO-F006 — QUERY_ROLE — bounded advisor over RepoDex alternatives

The model `choice`s among intents RepoDex already supports and that are valid
for the plan (`find`/`related`/`callers`/`callees` when a target exists). An
explicit `--intent`/`--target`/`--exhaustive` always wins — the role is never
asked. An unknown/malformed choice falls back to the baseline intent.

## SO-F007 — RERANK_ROLE — scores logical results, reorders only

`score` per logical seed, batched (16/request). Reorder-only: same result
multiset, deterministic tie-break on equal scores, transaction-like — if any
batch/answer fails the whole rerank falls back (never a half-corrupted order).
Candidate sets are scored as logical results, keeping them atomic.

## SO-F008 — COMPLETENESS — model cannot shrink truth

For `exhaustive`, rerank only reorders — `total`/`complete`/NoCandidate/
OutOfScope are computed by deterministic RepoDex before the model runs and are
unchangeable by it.

## SO-F009 — FALLBACK — disabled ≡ forced-failure

`System One disabled` and `enabled-but-every-call-fails` produce identical
result identities and identical deterministic ordering; only `so_*`
provenance differs (used→fallback).

## SO-F010 — RDX1 — additive summary keys only

`S so_query=…`/`S so_rerank=…` are forward-compatible additive `S` fields —
no RDX1 version bump, no change to `F`/`R` evidence records, nothing emitted
when disabled.

## SO-F011 — PERFORMANCE — negligible overhead, none when disabled

Disabled/absent config: no model object is built — identical baseline cost.
Enabled against a local mock: query+rerank add ~ms-scale HTTP calls bounded by
seed count (expansion caps at 64 seeds → ≤4 rerank batches + 1 query call).
Hugo query stayed ~0.65 s end-to-end in both modes.

## SO-F012 — LIVE_READINESS — local & hosted are config-interchangeable

`http://127.0.0.1:<port>/custom/path` (no auth) and an HTTPS-shaped bearer
endpoint both work — the configured full URL is authoritative, `/v1/systemone`
is not hardcoded, `provider`/`jev` is not in the contract. `system-one probe`
validates transport+auth+schema only; live Jev evaluation is deferred to
`docs/SYSTEM_ONE_LIVE_EVAL.md` pending real credentials.
