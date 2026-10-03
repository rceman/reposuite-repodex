# Claim -> evidence

- 19/19 inheritance-targeted cases gain candidates: MECHANICAL_GATE.json +
  MECHANICAL_PACKET_DIFFS.json (H0/H1 call_candidates.jsonl diffs).
- Nearest-level + override precedence: tests/php_inheritance.rs
  `inherited_method_lookup_uses_nearest_level`; phpinh corpus D/A/B `over`.
- Private ancestor unreachable: `visibility_and_missing_boundaries`
  (hidden => no_candidate both $this and typed).
- Cycle safety: `duplicate_and_external_and_cycle_parents_are_safe` +
  fixtures/phpinh/src/Cycle.php.
- Ambiguous parent arms: HIERARCHY_METRICS.json (SharedBase Ambiguous);
  DupChild calls each hit their own arm.
- static::/new static/trait/interface unsupported: tests +
  late_static_and_trait_dispatch_stay_out_of_scope.
- Transport parity: TRANSPORT_PARITY.json (4 probes, direct==service==auto).
- Pilot ceiling: PER_ARM.json (12/12 all arms), SESSION_METRICS.csv,
  traces/.
- Perf: PERFORMANCE.json (4MiB +5.3% full, under 20% flag).
- No agent gain: PILOT_RESULTS.json (headroom-stop decision).
