# RepoDex-First Evidence Navigation V1 — report

## Product work
- `src/query/adaptive.rs`: deterministic NavIntent -> obligations ->
  relevance-filtered deduped selection -> compact adaptive RDX + gap lines.
- `query --nav adaptive` flag (full RDX still default); same engine for CLI/service.
- Fixed `TestCandidatesOf`: was `QueryIntent::Callees` labeling callees tests ->
  now `Related` + `is_test_shaped` filter (native regression gate).
- Native gates `tests/adaptive_nav.rs`: classification, packet contents, dedup,
  budget, FACT/CANDIDATE, gaps — all pass; `cargo test` green.

## Causal benchmark (32 tasks x 3 arms x 2 reps = 192 sessions, dangerous mode)

| arm | success | input | tools | model | search | read | packet |
|-----|---------|-------|-------|-------|--------|------|--------|
| N0 native | 46/63 | 69.5k | 6.1 | 5.0 | 1.2 | 1.4 | 0 |
| F1 full-RDX | 52/62 | 69.6k | 4.2 | 3.9 | 0.2 | 1.3 | 13.9kB |
| A2 adaptive | 52/63 | 69.4k | 4.9 | 4.4 | 0.5 | 1.6 | **1.4kB** |

- **A2 preserves correctness** (52 >= N0-2; = F1's 52 with rep-1s).
- **A2 packet is ~90% smaller than F1** (1.4kB vs 13.9kB) — the adaptive
  packet keeps relationship usefulness at near-RDX-LITE cost.
- **A2 vs N0**: tools -20%, searches -58%, model calls -12%, edits +16 done.
- **Code cohort** is the repodex win: N0 5/15 vs A2/F1 12/15 — locator evidence
  turns navigation-dependent edits from failure to success.
- INFRA_INVALID 4 total (excluded); 0 real tool rejections.
- Intents, obligations, gaps, dedup verified natively; FACT/CANDIDATE intact.

## Verdicts
REPODEX_FIRST_NAVIGATION_CONFIRMED · ADAPTIVE_EVIDENCE_RDX_CONFIRMED

## Recommended next milestone
GTW_REPODEX_QUERY_INTEGRATION_V1 — wire the adaptive RepoDex-first evidence
delivery into the Gateway (the navigation layer is now validated end-to-end;
continuous learning is a later, separate concern).
