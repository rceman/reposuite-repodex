# Future Gateway/Relay emitter handoff

Minimum handoff so Relay can emit `context_artifact_presented` WITHOUT
parsing RDX — the `RepoQueryObservation` JSON emitted by
`query --emit-observation <dir>` (or an equivalent Gateway-side projection
of the same struct):

    packet text          -> Agent (unchanged)
    observation JSON     -> Relay: copy allowlisted keys into
                            context_artifact_presented.metadata,
                            set artifact_id=packet_digest,
                            content_digest=packet_digest,
                            content_bytes=packet_bytes,
                            presentation=nav_profile,
                            tool_call_id=<the repo_query tool call>

Relay then emits its ordinary tool_call_started/completed and
source_observed events. RepoDex consumes events via existing
`agent-events ingest` / `POST /v1/events`. Nothing else is required — no
RDX parsing, no query authority, no RepositoryView resolution.

RepoDex side must guarantee: packet+observation come from ONE execution;
observation metadata is a strict subset of what the query already computed.
