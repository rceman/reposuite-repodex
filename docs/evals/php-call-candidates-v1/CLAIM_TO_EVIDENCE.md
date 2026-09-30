# Claim -> evidence

- "PHP candidates emit call_candidate edges" -> tests/php_call_candidates.rs
  php_candidates_flow_through_graph_and_query; traces/*_P2.txt.
- "no FACT promotion" -> all php edges EvidenceClass::Candidate; packet `c`
  flag; STATIC_RESULTS.
- "bounded, no global method scan" -> no_global_method_name_scan test;
  this_method/scoped only touch named class candidates.
- "dynamic/LSB/dispatch honest" -> negative_controls_no_false_candidates,
  construction_out_of_scope, PHP_UNSUPPORTED_CASES.json.
- "PHP name semantics" -> php_case_insensitivity, tiered fallback assertions.
- "transport parity" -> direct==service==auto byte-identical (4 queries).
- "no Rust/Go/Python regression" -> full cargo test suite green.
