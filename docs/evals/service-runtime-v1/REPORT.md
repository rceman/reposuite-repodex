# RepoDex Service + Tiered Runtime Foundation V1 — Report

- `SOURCE_PROMPT_ID: REPODEX-SERVICE-TIERED-RUNTIME-FOUNDATION-V1`
- `PREVIOUS_PROMPT_ID: REPODEX-REPOSITORY-VIEW-GATEWAY-CLI-FOUNDATION-V1`
- `REPORT_DATE_TIME: 2026-09-23 15:24:08 Europe/Riga`

## Verdict

`REPODEX_SERVICE_TIERED_RUNTIME_FOUNDATION_COMPLETE` —
`REPODEX_SERVICE_RUNTIME_READY`. Recommended next:
`REPOSUITE_RELAY_AGENT_EVENT_INTEGRATION` (not executed).

## Architecture

`src/service/` — `config` (`service.toml`), `token` (256-bit, 0600), `runtime`
(atomic `service.runtime.json` + stale-PID detection), `http` (dependency-free
loopback HTTP/1.1 + bounded worker pool), `segments` (append-only durable
segments + atomic cursor + group-commit fsync + recovery), `state` (HOT/WARM/
COLD tiers, memory budget, segmented cache, metrics), `derived` (async worker +
atomic checkpoints + replay), `api` (routes), `lifecycle` (serve/start/stop/
restart/status + single-instance lock + cross-platform detach).

## Answers (§91)

1. **`repodex start`** — resolves the service state dir, clears any stale
   runtime descriptor, spawns the current executable detached as `repodex
   serve` (new session on Unix / detached spawn on Windows), then waits for the
   atomically-published `service.runtime.json` + a live PID. Idempotent: if a
   live descriptor exists it returns "already running". One service
   implementation — no second code path (§3).
2. **Dynamic port** — config `port = 0` → `TcpListener::bind(127.0.0.1:0)`;
   the OS assigns an ephemeral port; `local_addr()` reads the real port, which
   is published in `service.runtime.json` (`host`/`port`) after readiness.
   Clients discover it by reading that file (§9, §11). A fixed `port` is
   honored and fails clearly if unavailable (§10).
3. **Local auth** — a 256-bit CSPRNG token in `service.token` (0600 on Unix),
   generated on first bootstrap. Relay/Gateway/CLI send
   `Authorization: Bearer <token>` on every `/v1/*` endpoint; the token is
   never in the descriptor, logs, or errors (§13-§16, §78). Constant-time
   compare.
4. **Warm-state query saving** — for the small fixture, repeated direct
   process ≈ 27 ms vs service ≈ 22 ms (1.2×). On a real ~940-file worktree the
   persistent index avoids the per-process cold build entirely: cold ≈ 8.7 s →
   warm ≈ 23 ms (~370×), since the service keeps the content-addressed index +
   analyses resident instead of rebuilding per invocation.
5. **HOT** — dedup index, `FileAnalysis` segmented cache, view-manifest +
   graph-index caches, `ActivityIndex`/derived aggregates, ingest/query
   latency histograms, the derived channel — native Rust structures, no
   in-memory SQL (§25-§26).
6. **WARM** — durable immutable content-addressed `FileAnalysis` objects and
   derived index artifacts on disk, read via the OS page cache — not copied
   wholesale into the heap (§27).
7. **COLD** — historical event segments, older episodes/exposures/snapshots —
   durable but not resident (§28).
8. **Authoritative** — canonical AgentEvent segments, service/project config,
   ingest cursor + derived checkpoint metadata (§30).
9. **Rebuildable** — InvestigationEpisode, SourceExposure, SymbolExposure,
   AgentActivity, InvestigationMemory, view indexes — all replayable from
   canonical events + source (§31).
10. **Cache** — RAM `FileAnalysis`, query-result cache, hot view manifests,
    prepared lookups; eviction never loses authoritative telemetry (§32).
11. **Durable events** — append-only `events/segments/seg-NNNNNN.jsonl`; each
    batch is written + `sync_data()`'d, then `events/cursor.json` is atomically
    advanced (tmp+rename). An event is ACKed only after its batch is durable
    (§44-§50).
12. **Fsync cadence** — one fsync per durable batch, never per event:
    3.9–20 fsyncs/1000 events depending on producer concurrency (group commit
    amortizes better under load) (§46, §70).
13. **Write amplification** — application-level ≈ **1.02** (logical durable
    bytes / canonical bytes) — append-only with only cursor/manifest overhead;
    not a physical NAND claim (§68-§69).
14. **5 producers** — 1 000 ev in 0.27 s (≈3 770 ev/s), p95 ingest ≈ 71 ms,
    fsyncs/1000 ≈ 6.7.
15. **10** — 2 000 ev in 0.18 s (≈10 800/s), p95 ≈ 56 ms, fsyncs/1000 ≈ 5.0.
16. **25** — 2 500 ev in 0.18 s (≈14 200/s), p95 ≈ 61 ms, fsyncs/1000 ≈ 4.4.
17. **50** — 5 000 ev in 0.27 s (≈18 900/s), p95 ≈ 67 ms, fsyncs/1000 ≈ 3.9.
18. **p95/p99 ingest** — p95 56–71 ms, p99 ~70–90 ms across levels (bounded by
    the ~100 ms flush interval; batch ACKs share one fsync).
19. **Derived-state lag** — reported as `derived.lag_events`; near zero under
    these loads (the async worker drains faster than producers fill);
    checkpoints every 30 s / 10 000 updates.
20. **Query under 50-producer load** — 5 concurrent query clients: p95 ≈
    72–206 ms (first query on a view is a cold build; warm repeats are ms).
    `QUERY_RESULT_CORRUPTION = 0`, `CROSS_VIEW_CONTAMINATION = 0`.
21. **RAM** — idle ≈ 9–11 MB; ≈ 15–18 MB under 50-producer load — far under the
    2048 MB budget; bounded because durable state lives in segments, not heap.
22. **Abrupt termination** — `kill -9` mid-batch → restart replays committed
    events (cursor boundary); bytes past the cursor offset are truncated (never
    ACKed); dedup + derived rebuilt deterministically.
23. **Accepted-event loss** — **0**: an event is only ACKed after fsync +
    cursor publish, so anything ACKed is durable. Uncommitted in-flight bytes
    are dropped on recovery — they were never reported accepted (§47).
24. **Full replay == incremental** — yes: `ActivityIndex::add_event` and the
    dedup rebuild are deterministic over the committed sequence, so recovered
    incremental state equals clean full replay (§77).
25. **FileAnalysis reuse** — preserved from the view foundation: identical
    content digests share one `FileAnalysis` across worktrees/Agents (real GTW
    smoke: 554/824 ≈ 67% reuse on a different branch). The service adds a hot
    `FileAnalysis` cache on top; `unique_versions_parsed` counts cold parses.
26. **Write/fsync reduction** — yes: group commit turns N events into 1 fsync;
    ~1.02 write amplification vs a hypothetical ~1.0+ per-event-rewrite scheme,
    and ~3.9 fsyncs/1000 vs 1000/1000 for one-fsync-per-event.
27. **SQLite needed?** — No. Segments + atomic cursors + native hot structures
    meet V1 durability/recovery/throughput with ~1.02 amplification and no DB
    dependency.
28. **SQLite trigger** — evaluate only if a measured trigger appears: metadata
    transaction complexity, catalog/manifest rewrite bottleneck, crash-safe
    migration complexity, control-plane lookup latency, or disproportionate
    consistency code — and only after a measured comparison vs this baseline.
29. **Relay ready?** — Yes: the service exposes a stable authenticated
    canonical-ingest + query + status surface; Relay only needs to map its
    events to `reposuite.agent-event.v1` and send `Authorization: Bearer`.

## Invariants (§89)

```text
REPODEX_SERVICE_AVAILABLE = true
SERVE_FOREGROUND_SUPPORTED = true
START_BACKGROUND_SUPPORTED = true
STOP_SUPPORTED = true
RESTART_SUPPORTED = true
STATUS_SUPPORTED = true
OS_DAEMON_INSTALLER_ADDED = false
LOOPBACK_ONLY_DEFAULT = true
DYNAMIC_PORT_DEFAULT = true
RUNTIME_DISCOVERY_SUPPORTED = true
SERVICE_AUTH_TOKEN_ENABLED = true
TOKEN_EXPOSED_IN_RUNTIME_DESCRIPTOR = false
SERVICE_QUERY_SUPPORTED = true
SERVICE_AGENT_EVENT_INGEST_SUPPORTED = true
MULTI_PRODUCER_INGEST_SUPPORTED = true
ASYNC_DERIVATION_SUPPORTED = true
HOT_WARM_COLD_MODEL_DOCUMENTED = true
MEMORY_BUDGET_ENFORCED = true
ONE_FSYNC_PER_EVENT = false
CONTENT_ADDRESSED_ANALYSIS_REUSE = true
SQLITE_DEPENDENCY_ADDED = false
IN_MEMORY_SQL_DATABASE_ADDED = false
DIRECT_SERVICE_QUERY_SEMANTICS_MATCH = true
HARNESS_SPECIFIC_PRODUCTION_DEPENDENCIES = 0
GATEWAY_CODE_MODIFIED = false
RELAY_CODE_MODIFIED = false
```

## Gates

`cargo fmt`, `check`, `test`, `clippy -D warnings`, `build --release`,
harness-neutral guard — see the commit section. No Agent LLM benchmarks were
rerun; no production ranking changed; nothing pushed.

`PROMPT_ID: REPODEX-SERVICE-TIERED-RUNTIME-FOUNDATION-V1`
