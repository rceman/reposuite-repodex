# Claim -> evidence

- canonical context_artifact_presented + tool_call_id/metadata: src/agent_event/model.rs, schemas/examples/context-artifact-presented.jsonl, tests/gap_telemetry.rs
- no RDX in events: context.rs/model.rs — only digest/bytes/metadata fields exist
- Relay never parses RDX: PRODUCTION_EMITTER_HANDOFF.md; observation emitted by RepoDex itself via --emit-observation
- one execution/one truth: src/query/observation.rs consumes EvidenceProjection + AdaptiveRendered; adaptive_rdx_observed returns renderer accounting
- agent-facing output unchanged: RDX_ABLATION.json (5/5 byte-identical)
- transport parity of observation: same render_adaptive_packet called on direct AND service paths (cli.rs)
- gap normalization: GAP_SIGNATURE_CONTRACT.md + observe() mapping
- windows/session isolation/idempotency: tests/gap_telemetry.rs (14 tests)
- replay validation: REPLAY_RESULTS.json (929 sessions, 415 presentations, 615 controls, false_repodex_associations=0)
- no learning: telemetry is ingest+report only; nothing reads it in query path (NORMAL_QUERY_MUST_NOT_SCAN_TELEMETRY by construction)
- scale: SCALE_BENCHMARK.json 10k sessions / 33k events — ingest 868ms, analysis 175ms
- hygiene: PREVIOUS_EVIDENCE_REPAIR.json
