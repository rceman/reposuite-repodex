# Interactive RepoDex-First Navigation Correction V1 — report

## Corrections (commit 9be271a)
- `Ambiguous` now reachable (no cue + not structural) -> explicit gap.
- Fake-regex cues removed; explicit intent precedence (Test first so a "call"
  word can't steal it; manifest before callees).
- Callers/Callees require `call_candidate`/`call` kind + correct direction —
  `contains`/`member_of`/`owned_by_manifest` can no longer be emitted as
  caller/callee (e.g. "who calls Get?" now honestly returns related=0 rather
  than a fake containment caller).
- ManifestConfig filters the real canonical relation kinds.
- Irrelevant-filtered edges no longer emit a false TRUNCATED_CONTINUATION.
- Real serialized byte budget (2KiB lookup / 4KiB rel / 8KiB path / 12KiB cap),
  whole-packet line-boundary cut + truthful continuation gap.
- Fixed `TestCandidatesOf` retained; `is_test_shaped` honest.
- Benchmark validator: exclude the `repo_query` shim from mutation changed_paths.

## Interactive campaign — 24 tasks x 3 arms x 2 reps = 144 sessions (0 INFRA)

| arm | success | input | tools | model | repodex | search | read | wall |
|-----|---------|-------|-------|-------|---------|--------|------|------|
| N0 native      | 36/48 | 77.2k | 7.5 | 5.6 | 0  | 0.9 | 1.2 | 33.8s |
| P1 pre-inject  | 43/48 | 57.9k | 5.4 | 4.2 | 0  | 0.5 | 1.4 | 24.7s |
| R2 interactive | 42/48 | 78.3k | 6.8 | 5.7 | **59** | 0.5 | 1.3 | 30.0s |

- **Real interactive usage: 45/48 R2 sessions issued >=1 `repo_query` (94%
  first-query adherence); 59 total calls.** Prior V1 had repodex_calls=0.
- **R2 preserves correctness** (42 >= N0 36; within 1 of P1 43).
- **cd cohort: N0 5/12 -> P1 12/12 -> R2 12/12** — interactive repodex matches
  perfect pre-injection on navigation-dependent code tasks.
- R2 vs P1 overhead (the real interactive cost): +1.5 model calls, +1.4 tools,
  +5.3s wall, +20.4k input — the price of tool selection vs perfect delivery.
- R2 vs N0: tools -9%, search -44%, correctness +6 — reduces broad discovery.
- Packet sizes: median 747B, max 2057B — all within budgets.
- Intent audit: canonical task phrasings classify correctly; R2 mismatches are
  agent free-form paraphrases (e.g. "where do I add X" -> Ambiguous), not
  classifier errors on the frozen bounded corpus.

## Verdicts
INTERACTIVE_REPODEX_FIRST_NAVIGATION_CONFIRMED
ADAPTIVE_RELATIONSHIP_EVIDENCE_CONFIRMED
READY_FOR_GTW_REPODEX_QUERY_INTEGRATION_V1
