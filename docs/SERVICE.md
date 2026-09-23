# RepoDex Service — persistent local runtime

RepoDex runs as a persistent localhost service with a foreground/background
lifecycle, bearer-token auth, dynamic-port discovery, a HOT/WARM/COLD tiered
runtime, batched durable AgentEvent ingest, and asynchronous derived
processing. **No SQLite**, no OS daemon installer, harness-neutral.

## Lifecycle (§1-§5)

```bash
repodex serve      # run in the foreground
repodex start      # spawn detached `repodex serve`, wait ready, return
repodex stop       # graceful authenticated shutdown
repodex restart    # stop + start
repodex status     # human / --json machine status
```

`start` reuses `serve` — there is exactly one service implementation. There is
**no** daemon/systemd/launchd/Windows-Service installer; boot/login
registration belongs to the OS service manager / a future RepoSuite supervisor
/ Gateway orchestration (§2). A single-instance `service.lock` plus a live-PID
check makes `start` idempotent (§5).

## Discovery + config (§6-§12)

State dir `~/reposuite/repodex/` (`REPODEX_STATE_DIR` for tests/isolation).

- `service.toml` — versioned config: `host` (default `127.0.0.1`, loopback
  only), `port` (default `0` = OS-assigned ephemeral; a fixed port is allowed
  but fails clearly if unavailable), `memory.max_mb`, `ingest.*`,
  `derived.*`.
- `service.runtime.json` — atomic runtime discovery descriptor published after
  bind: `pid host port instance_id started_at version protocol_version`.
  Removed on graceful stop; `status`/`start` validate the PID is alive rather
  than trusting a stale file.
- `service.token` — 256-bit CSPRNG token (0600 on Unix), generated on first
  bootstrap. **Never** printed, logged, or placed in the runtime descriptor.

All `/v1/*` endpoints require `Authorization: Bearer <token>`; no meaningful
endpoint is unauthenticated (§79). The token is the local-machine credential —
TLS is unnecessary for V1 loopback.

## API (§17-§19)

```text
GET  /v1/status           authenticated service status/metrics
POST /v1/query            reposuite.repodex.query.request.v1 -> response.v1
POST /v1/events           one canonical reposuite.agent-event.v1
POST /v1/events/batch     array of canonical events
POST /v1/shutdown         graceful stop
```

`POST /v1/query` accepts the same locator forms as the CLI (`root` OR
`project` + optional `view`) with identical correctness semantics — no second
query model. `POST /v1/events*` accept **only** canonical
`reposuite.agent-event.v1`; Relay/future adapters convert external runtimes to
canonical form first (RepoDex stays harness-neutral, §41-§42).

The CLI `query` uses the running service when one is live, with `--service` to
require it and `--direct` to force the standalone process path (§20). Future
Gateway/Relay integrations should depend on the service explicitly rather than
silently falling back to direct behavior (§21).

## Tiers (§24-§28)

```text
HOT   explicit process memory — dedup index, FileAnalysis segmented cache,
      view-manifest/graph-index caches, ActivityIndex & derived aggregates,
      ingest/query latency histograms. Native Rust structures; no in-memory SQL.
WARM  durable immutable content-addressed FileAnalysis objects + derived
      indexes, read via OS page cache — not copied wholesale into the heap.
COLD  historical event segments / episodes / exposures / snapshots — no
      permanent hot RAM.
```

## Authoritative / rebuildable / cache (§29-§32)

- **Authoritative**: canonical AgentEvent segments, service/project config,
  ingest cursor + derived checkpoint metadata.
- **Rebuildable**: InvestigationEpisode, SourceExposure, SymbolExposure,
  AgentActivity, InvestigationMemory, view indexes.
- **Cache**: RAM FileAnalysis, query results, hot view maps, lookup tables —
  eviction never loses authoritative telemetry.

## Ingest + derived (§44-§53)

The ACK path is small: validate → dedup → enqueue → durable group-commit →
ACK. A batch of events is written/fsynced together (default policy: every
100 ms OR 256 events OR 128 KB), then `events/cursor.json` is atomically
advanced — an event is ACKed only after its batch is durable (§47). Append-only
`seg-NNNNNN.jsonl` segments rotate by size; partial tails are truncated on
recovery (§76).

Durable events feed a background derived worker (episode/activity/exposure/
memory aggregates) that never blocks the producer; it checkpoints atomically
every 30 s or 10 000 dirty updates. On restart the service replays committed
events to rebuild dedup + derived state — recovered incremental state ==
clean full replay (§77). Queries read the latest fully published state and are
never blocked by ingest (§54-§56).

## Memory budget + cache policy (§35-§40)

`memory.max_mb` (default 2048) bounds the hot tier. Under pressure, eviction is
deterministic: probationary FileAnalysis → view manifests → graph indexes →
older derived hot state — never unpersisted authoritative evidence. Caches use
a 2-segment (probation + protected) LRU so a broad one-time scan can't flush
the protected working set.

## Performance (measured, see eval/)

- ~19 000 events/s at 50 producers; p95 ingest ~60–70 ms (group-commit latency).
- fsyncs per 1000 events: 3.9 (50 producers) — one fsync per durable batch.
- Application write amplification ≈ 1.02 (append-only; ≈ canonical bytes in).
- RSS 11–18 MB across load levels (budget 2048 MB).
- Query correctness under ingest load: `QUERY_RESULT_CORRUPTION = 0`.

## No-SQLite baseline (§83-§85)

V1 uses append-only segments + atomic cursors + native hot structures — no
SQLite, embedded SQL, or in-memory SQL. A future SQLite experiment is justified
only if a measured trigger appears (metadata transaction complexity, catalog
rewrite bottleneck, crash-safe migration complexity, control-plane lookup
latency, disproportionate consistency code) — and must be compared against this
baseline on writes/fsyncs/latency/CPU/RAM/recovery before adoption.
