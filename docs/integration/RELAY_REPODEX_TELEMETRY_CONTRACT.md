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

## Context artifact events (Production Gap Telemetry V1)

Relay SHOULD emit `context_artifact_presented` for evidence delivered to the
Agent — RepoDex RDX packets today; the type is producer-neutral by design.

Required on the payload:

    artifact_id       stable identity (packet digest)
    artifact_kind     e.g. "rdx1_packet"
    producer          e.g. "repodex"
    tool_call_id      the tool call that produced it (correlation, §19)
    content_digest    sha256 of the artifact payload
    content_bytes     payload size
    presentation      e.g. "adaptive"
    metadata          bounded structured fields ONLY, supplied by the
                      producer — see below

`data` must NEVER contain the artifact text itself.

## Structured metadata handoff (Gateway -> Relay, future)

`RELAY_PARSES_RDX_FOR_TELEMETRY = false` — Relay must never parse the
Agent-visible RDX string to build telemetry. The minimum metadata a future
Gateway integration hands to Relay is the RepoDex `RepoQueryObservation`
(`reposuite.repodex.query-observation.v1`), produced by the same execution
that rendered the packet — today via `query --emit-observation <dir>`:

    query_digest        sha256 of normalized query text (not the text)
    intent              classified navigation intent
    nav_profile         rendered profile
    seed/related/candidate counts, dispositions histogram
    gap_signatures[]    {family, reason_code} — normalized, bounded
    packet_bytes, packet_digest, duration_ms

Relay copies the allowlisted keys into `metadata`; no other derivation needed.

## What Relay does NOT do

    - run RepoDex queries or choose RepositoryViews (Gateway owns that)
    - interpret task query authority
    - modify RepoDex evidence
    - persist full query text or arbitrary tool arguments

## Invariants (unchanged + extended)

    RELAY_REPODEX_RESPONSIBILITY            = TELEMETRY_ONLY
    RELAY_REPODEX_QUERY_SUPPORTED           = false
    RELAY_AGENT_FACING_REPODEX_HELPER       = false
    RELAY_RESOLVES_QUERY_REPOSITORY_VIEW    = false
    RELAY_RESOLVES_TASK_QUERY_AUTHORITY     = false
    GATEWAY_OWNS_REPODEX_QUERY_AUTHORITY    = true
    RELAY_PARSES_RDX_FOR_TELEMETRY          = false
    AGENT_VISIBLE_TELEMETRY_BYTES           = 0
    TELEMETRY_UPDATES_QUERY_BEHAVIOR        = false
    TELEMETRY_UPDATES_MEMORY                = false
    TELEMETRY_UPDATES_ROUTE_PRIORITY        = false
    TELEMETRY_UPDATES_JEV                   = false
    TELEMETRY_UPDATES_ADAPTIVE_SELECTION    = false
