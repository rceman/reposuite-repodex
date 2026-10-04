# RepoQueryObservation (reposuite.repodex.query-observation.v1)

Built in `src/query/observation.rs` from the `EvidenceProjection` +
`AdaptiveRendered` of the same execution.

Fields: `query_digest` (sha256; raw text not stored), `intent`,
`nav_profile`, `repository_id`, `repo_head`, `emitted_seed_count`,
`eligible_seed_count`, `seed_selection_complete`,
`relationship_limit_reached`, `seed_count`, `relation_count`,
`candidate_count`, `dispositions` histogram, `gap_signatures[]`,
`packet_bytes`, `packet_digest`, `duration_ms`.

Emission: `query --emit-observation <dir>` → `obs-<packet_digest>.json`,
written after render on BOTH direct and service paths (the service path
deserializes the identical projection). Best-effort: emit failure warns on
stderr and never changes stdout. Machine JSON — `AGENT_FACING_JSON_OUTPUT`
remains false for query output itself.
