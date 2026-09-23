# Service Runtime V1 — Benchmark Spec

Non-LLM benchmark of the persistent RepoDex service. Synthetic canonical
AgentEvents only (§62) — no Agent sessions. KPIs follow
`../BENCHMARK_METRICS.md` (Correctness / Cost / Work / Time).

## Workload (§64-§65)

- **Normal**: mixed `source_observed` / `tool_call_completed` /
  `model_call_completed` canonical events, sent in 50-event batches per
  producer — a plausible per-Agent ingest granularity (events are produced in
  bursts around tool calls, not uniformly).
- **Burst**: sharp bursts well above average (≥100 ev/s-equivalent), labelled
  stress — not claimed sustained real load.
- **Producer levels** (§63): 1, 5, 10, 25, 50 concurrent producers.

## Measured

- **Ingest** (`/v1/events/batch`): throughput, p50/p95/p99 latency, durable
  ACK latency, fsyncs/1000 events, durable batches/1000, application write
  amplification (`logical bytes / canonical bytes`), segments.
- **Query under load** (§66): 5 concurrent `/v1/query` clients against a
  git-worktree fixture while 10/25/50 producers ingest; correctness
  (`QUERY_RESULT_CORRUPTION`, `CROSS_VIEW_CONTAMINATION`).
- **Crash recovery** (§75-§77): `kill -9` mid-batch → restart → committed
  events recovered, derived replayed, replay-equivalence.
- **Direct vs service** (§82): repeated `query --direct` vs `/v1/query`.
- **Cost**: RSS idle + under load, disk footprint.

## Isolation

`REPODEX_STATE_DIR=/tmp/rv-*`; fixture worktrees under `/tmp/rv-fixture`. The
real user registry/service is never touched.
