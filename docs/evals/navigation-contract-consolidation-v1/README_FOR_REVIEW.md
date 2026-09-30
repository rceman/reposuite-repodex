# navigation-contract-consolidation-v1 — review map

Four production navigation-contract defects corrected (see REPRODUCED_ASTRA_FINDINGS):

- A: intent planned before retrieval — adaptive::plan() maps a question to a
  typed QueryIntent + anchors BEFORE run_view_query; explicit flags win.
  Evidence: traces/*_OLD.txt vs *_NEW.txt; tests adaptive_nav.
- B: service parity — all normalized fields forwarded; adaptive renders from
  the canonical projection through the identical code path; direct==service
  ==auto byte-identical. Evidence: traces/service_*.txt, SERVICE_PARITY_RESULTS.
- C: UTF-8-safe record-aware budget — no byte slicing, whole-record emission,
  trailer inside the bound. Evidence: UTF8_BUDGET_RESULTS, adaptive_nav tests.
- D: truthful trailer — S counts emitted state; bytes= equals final size
  (validate_packet fixpoint); RESULT_LIMIT != AMBIGUOUS_RESULT; no dangling
  local refs.

Campaign: 144 sessions (24 tasks x N0/F1/A2 x 2), 0 infra-invalid.
Aggregate: PER_ARM/PER_COHORT/PAIRED_DELTAS/DISCOVERY_ACCOUNTING/
FIRST_DISCOVERY_ADHERENCE/TOKEN_ACCOUNTING/TIMING/PACKET_METRICS.
