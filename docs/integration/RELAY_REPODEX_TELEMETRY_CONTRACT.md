# Relay <-> RepoDex Telemetry-Only Contract (v1)

## Ownership

RepoSuite Relay sends telemetry to RepoDex. It does NOT own RepoDex query
authority and does NOT call `/v1/query`. Agent -> Relay -> /v1/events.

Invariants:
    RELAY_REPODEX_RESPONSIBILITY            = TELEMETRY_ONLY
    RELAY_REPODEX_QUERY_SUPPORTED           = false
    RELAY_AGENT_FACING_REPODEX_HELPER       = false
    RELAY_RESOLVES_QUERY_REPOSITORY_VIEW    = false
    RELAY_RESOLVES_TASK_QUERY_AUTHORITY     = false
    GATEWAY_OWNS_REPODEX_QUERY_AUTHORITY    = true

## Transport

`POST /v1/events` (single) and `POST /v1/events/batch` (array of AgentEvent),
bearer auth. AgentEvent envelope: `schema` (`reposuite.agent-event.v1`),
`event_id`, `session_id`, `sequence`, `timestamp`, `source{runtime,adapter,
adapter_version}` (opaque strings — RepoDex never branches on them), `type`,
`data` (opaque, round-trips).

## ACK boundary

A `200` ack means the event was durably accepted. Derived episode/memory
processing is asynchronous — ingest latency is independent of derivation.

## Batch semantics

Per-event, non-atomic: a malformed element yields a per-element error while
valid siblings still commit. `event_id` dedupes — re-ingestion is idempotent
(`duplicates` incremented, not re-committed).

## SourceObserved

`source_observed` means the source CONTENT was actually delivered to the agent —
not a filename mention, search result, locator, or displayed path. A bare path
reference must NOT produce `source_observed`.
