# Navigation Contract — corrected architecture (V1)

Pipeline (defect A fixed):
  question + explicit options
    -> adaptive::plan()    [classify -> NavIntent; anchors -> target/to]
    -> typed QueryIntent   [callers|callees|paths|related|find]
    -> run_view_query (typed engine op)
    -> EvidenceProjection (canonical)
    -> adaptive_rdx (record-aware emit, UTF-8-safe budget, honest trailer)
    -> RDX packet

Hard invariants preserved: candidate != FACT; bounded absence != global;
explicit ambiguity > guessed identity; current view > history.
