# Claim -> evidence

- structured disposition: src/graph/model.rs (schema 3), src/graph/build.rs,
  src/query/projection.rs, src/query/observation.rs (disposition_parts)
- real-query reason codes: tests/gap_telemetry.rs
  production_query_generates_*; PRODUCTION_PATH_GAP_TESTS.json;
  SYNTHETIC_OPERATION_TESTS.json reasons list
- operation field: src/agent_event/model.rs ToolCallStarted.repository_operation
- operation-aware classifier: src/agent_event/fallback.rs
  operation_class/legacy_class + UnclassifiedRepositoryActivity
- precedence + unclassified-not-stopper: fallback.rs + tests
  (legacy_shell_is_unclassified_not_task_action, shell_discovery_op_is_native_fallback,
   other_repository_tool_with_discovery_op_is_fallback,
   task_action_ends_window, source_read_op_is_verification)
- count reconciliation: report top-level fields + COUNT_RECONCILIATION.json +
  tests (report_counts_reconcile)
- replay: tools/telemetry/replay_to_agent_events.py (now emits ops),
  tools/telemetry/observed_stream.py (real-query stream), REPLAY_RESULTS.json
- safety: RDX_ABLATION.json (0 diffs), SCALE_BENCHMARK.json, FAILURE_ISOLATION
  unchanged (emit-failure path untouched)
- previous evidence corrected: ../production-gap-telemetry-v1/ERRATA.md
