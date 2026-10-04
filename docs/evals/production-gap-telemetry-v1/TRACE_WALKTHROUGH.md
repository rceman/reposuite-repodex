# Replay walkthrough

- `inh-audit-handle.P2.r1` (fixture): `tool_call_started(exec)` -> `context_artifact_presented` (producer=repodex, correlated via tool_call_id, intent=definition, gap `result_limit`) -> `source_observed` (explicit_read) -> `another_context_artifact` -> `file_read`. Report: second query -> `another_context_artifact` then verification.
- `inh-*.N0.r1` controls: session contains native search/read events only -> 0 artifacts -> `sessions_without_artifacts` +1, `false_repodex_associations` stays 0.
- `nc-legacy-handle.P2.r1`: RepoDex result then `grep`-classified `search` -> `native_discovery_after_artifact` — the exact signal the production gate measures.
