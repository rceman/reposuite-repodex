# Continuous Project Learning Foundation V1 — report

## Precursor audit (§4)
- **A rebinding overstates freshness**: `NOT_REPRODUCIBLE` — `BindingState` is
  granular (ExactFresh/MovedButSame/ChangedInterface/Absent/Ambiguous); only
  byte-identical content is emitted as `fact`. Never a bare `fresh` bool.
- **B unique-anchor not enforced**: `CONFIRMED_CURRENT_DEFECT` — recipe
  `execute()` took the first declaration seed with no ambiguity check despite
  the advertised "unique" guard. FIXED: multiple distinct same-label anchors now
  yield `Ambiguous`/fallback, never an arbitrary first-match execution.
- **C memory loading scans all**: `DESIGN_LIMITATION` — `store.load()` eagerly
  reads every entry file at open; a term index exists for lookup but full load
  occurs at open. Flagged for the hardening milestone (bounded index).

## Foundation (src/learning.rs)
Deterministic inventory → view-bound coverage ledger (5 explicit states,
NA requires a reason) → gap-driven question curriculum (64 candidates,
priority = si*4+ur*3+dm*2-cost-churn, no Jev) → 50 investigations → 4 reusable
memory families (family+anchors+route+deps — HOW to investigate, never a
cached answer) → conservative promote/demote + rebind states.

## Native safety gates — 10 pass
missing→Missing, ambiguous→Ambiguous, changed-view→Stale, no candidate→FACT
upgrade, conservative promotion, demotion, bounded NA reason, gap-only
questions, deterministic priority, coverage view-binding.

## Learning-curve campaign — 240 sessions (12 held-out × B0/L1 × 2 × 5 ck), 0 INFRA
| ck | B0 succ | L1 succ | L1 input | L1 rep | L1 memhits |
|----|---------|---------|----------|--------|------------|
| C0  | 23/24 | 23/24 | 78.8k | 1.0 | 0 |
| C5  | 22/24 | 22/24 | 82.3k | 1.2 | 18 |
| C10 | 22/24 | 22/24 | 86.1k | 1.7 | 18 |
| C25 | 23/24 | 21/24 | 83.8k | 1.5 | 18 |
| C50 | 22/24 | 21/24 | 81.5k | 1.3 | 18 |

**Honest result — continuous learning produced NO measured benefit.** L1
success equals or trails B0 (C25 -2, C50 -1); L1 uses MORE input (+3.5k..+10k)
and MORE repodex calls (the learned route drives extra queries). Memory was
reused (18 hits/checkpoint) but supplied no downstream task-cost saving.
Conclusion (§61): **deterministic adaptive navigation already captures the
reusable value**; learned route hints add context without improving outcome.
Negative but valid — not tuned to win.

## Safety
FALSE_MEMORY_TRUTH_UPGRADES=0 (memory is route hints, not facts);
STALE_MEMORY_ACCIDENTALLY_USED=0; HELD_OUT_ANSWER_LEAKAGE=0; no CoT; JSON off;
Jev off; rebind states honest; unique-anchor now enforced.
