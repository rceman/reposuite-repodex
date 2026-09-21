# System One integration — results

```text
MISSION  : REPODEX-AUTONOMOUS-SYSTEM-ONE-INTEGRATION-FOUNDATION-V1
BASE     : fed1291
STATUS   : SYSTEM_ONE_INTEGRATION_FOUNDATION_COMPLETE
           LIVE_JEV_EVALUATION_READY
```

## Operating modes validated against local compatible mock (§48)

| mode | result |
|------|--------|
| A. baseline (disabled)        | deterministic output, no `so_*` provenance |
| B. query role only            | bounded `choice` advice applied / fallback on failure |
| C. rerank role only           | seeds reordered, identical result multiset |
| D. query + rerank             | both roles exercised; `so_query=used so_rerank=used` |

## Multi-model routing (§49)

`roles.query=local-a` + `roles.rerank=local-b` → query `choice` requests hit
only `local-a`'s URL, rerank `score` requests hit only `local-b`'s URL
(verified by captured request paths on two independent servers). Same-model-
both-roles also validated.

## Endpoints (§50/§51)

- custom local path `http://127.0.0.1:<port>/custom/systemone`, `auth=none`: PASS
- HTTPS-shaped config + `bearer` + `model` field: header verified on a mock
- `header` auth (`X-API-Key`): verified on a mock

## Error/fallback (§40-§42)

401→Auth, 429→RateLimited, 5xx→Server, malformed JSON→InvalidJson,
malformed typed answer→InvalidResponse, dropped connection→Connection —
all → deterministic baseline; result identities/order unchanged.

## Performance (§52)

```text
disabled (no config)      : ~0.65 s cold query on Hugo (graph load+index+run)
enabled vs local mock     : ~0.65 s — negligible added overhead
rerank batching           : bounded (16/question batch; ≤64 expanded seeds)
```

## Tests

```text
system_one tests : 12 (config, protocol, auth, errors, roles, routing, fallback)
total suite      : 612
regressions      : Rust/Go/graph/query/RDX1 audits unchanged, all pass
```

## What remains

- Only real-credential live Jev evaluation (see `docs/SYSTEM_ONE_LIVE_EVAL.md`).
- No live TypeSafe call was made; no API key was added; nothing was pushed.
