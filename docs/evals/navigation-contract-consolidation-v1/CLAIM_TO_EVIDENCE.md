# Claim -> evidence

- "callers/callees/paths now execute typed machinery before retrieval"
  -> tests/adaptive_nav.rs plan_maps_natural_language_to_typed_operation;
     traces/What_does_ResolveKey_call__{OLD,NEW}.txt; live queries.
- "direct/service/auto are semantically identical"
  -> traces/service_NEW.txt == auto_NEW.txt == direct output (byte-identical);
     SERVICE_PARITY_RESULTS.json.
- "no UTF-8 panic, no mid-record cut, whole packet in budget"
  -> tests utf8_budget_never_panics_never_splits_scalar,
     whole_packet_budget_includes_trailer, no_dangling_local_references;
     PACKET_METRICS.json (77 live packets, 0 violations).
- "summary describes emitted state"
  -> validate_packet bytes= fixpoint test; S counts = emitted records.
- "first-discovery adherence measurable"
  -> FIRST_DISCOVERY_ADHERENCE.json (F1 48/48, A2 48/48 repodex-first);
     metrics::discovery_flags.
- "dex/shell-sequence discovery correctly classified"
  -> tools/eval/classify.py segment decomposition + dex pattern.
