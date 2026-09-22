# AgentEvent v1 + Telemetry Ingest — spec

Establish the canonical **RepoSuite-level** `AgentEvent v1` contract, a durable
Agent Event Store, a generic ingest boundary, and the first producer adapter
(Devin ATIF). Validate by replaying all 240 benchmark ATIF traces.

## Components

- `src/agent_event/model.rs` — harness-neutral envelope + typed payloads;
  `reposuite.agent-event.v1`. Unknown future types preserved via opaque `data`.
- `src/agent_event/store.rs` — `AgentEventStore`: `manifest.json` +
  `sessions/<sid>.jsonl`, append-oriented; **per-session** dedup so memory is
  bounded by one session, not the store.
- `src/agent_event/ingest.rs` — `AgentEventSink` boundary + envelope
  validation + `IngestReport{accepted,duplicates,rejected,errors}`.
- `src/agent_event/devin_atif.rs` — Devin ATIF adapter (isolated ATIF names).
- CLI `agent-events ingest|verify|stats|session|export`.

## Semantics

- change/idempotency authority = `event_id = {session}:{sequence}`; sequence
  monotonic; RFC3339 UTC for wall-time. Same id + different content = conflict.
- `source_observed` = content actually delivered to the model (read file-view,
  grep content); name-only discovery excluded. External/truncated paths →
  `path` absent.
- tokens = actual per-model-call usage summing exactly to ATIF final totals.

## Validation

- Contract: 8 synthetic tests (minimal, idempotent, conflict, unknown-type,
  malformed, out-of-order, path-normalization, large-batch).
- Real: replay 240/240 ATIF exports; reconcile tokens/tool-calls/sequence/
  final-answer vs ATIF authoritative + prior benchmark telemetry; manual
  source-observation inspection of 16 sessions (E0-E3 × L1-L4).
- Perf: conversion+ingest throughput, peak RSS, store bytes/event, duplication.
