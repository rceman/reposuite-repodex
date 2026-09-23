# Investigation Memory Foundation — Report

```text
INVESTIGATION_MEMORY_FOUNDATION_COMPLETE
MEMORY_AWARE_AGENT_BENCHMARK_READY
INVESTIGATION_MEMORY_SIGNAL_CONDITIONAL
```

Base `bb1b9ca` → addendum boundary restoration `a003dc7` → memory layer.

## Harness-neutral boundary (addendum, mandatory)

```text
HARNESS_SPECIFIC_PRODUCTION_DEPENDENCIES = 0     (tests/harness_neutral.rs)
DEVIN_ATIF_PRODUCTION_MODULE_REMOVED = true      (src/agent_event/devin_atif.rs deleted)
PROTOTYPE_ADAPTER_PATH = scripts/adapters/devin-atif.py
REPODEX_CANONICAL_INGEST_ONLY = true             (--format devin-atif removed)
RUNTIME_PROVENANCE_OPAQUE = true                 (source.runtime is opaque metadata)
REPOSUITE_RELAY_FUTURE_BOUNDARY_DOCUMENTED = true (scripts/adapters/README.md)
```

240-session replay re-validated through `scripts/adapters/devin-atif.py` →
canonical ingest: **input=92,450,895 output=939,108 tools=6,134
source_observed=15,256 final=240** — identical to the accepted canonical
store, plus **180 `context_artifact_presented`** events (E1/E2/E3 packets).

## Architecture delivered

- `context_artifact_presented` (generic; §5 fields, RDX1 packet refs with
  rank). Surfaced ≠ observed ≠ read — three facts separate.
- `src/memory/`: `QuerySignature` (digest/terms/identifiers/paths), bounded
  postings index, `MemoryStore` (entries + postings + checkpoint), explainable
  similarity, `MemoryEvidence` + `EvidenceStrength` classes, `Freshness`.
- AgentActivity scalability fix (§11-§13): hot `PathActivity` now holds
  **counts only** — the unbounded session/investigation-id sets moved to a
  disk-backed contribution index (`activity/contrib.jsonl`) used only at
  build. Hot record verified bounded (test `hot_path_bounded`, 200 sessions →
  record <8KB).

## Real replay (240 sessions → 240 memory entries)

All canonical totals reconciled (0 mismatches). `derive` now folds 31,994
events incl. 180 context artifacts.

## Context-feedback (§79): what RepoDex showed vs Agent used

```text
E1: surfaced=1251  observed 33%  read 20%  mentioned  8%
E2: surfaced=1251  observed 33%  read 22%  mentioned  9%
E3: surfaced= 456  observed 37%  read 23%  mentioned 11%
non-surfaced discovered/read/mentioned: E1 473 (211 men) E2 491 (211) E3 491 (218)
```

Most surfaced paths go unused (neutral, §24). Agents independently discovered
~470-490 non-surfaced paths per treatment — ~210 of which reached the answer.

## Memory signal (§80) — reported separately, NOT aggregated

| track | setup | P@1 | P@5 | P@10 | R@10 | MRR |
|-------|-------|-----|-----|------|------|-----|
| **A exact-repeat** | earlier reps, same q | .225 | .875 | **.975** | .900 | .468 |
| **B leave-q-out** | other 19 questions | .050 | .250 | .300 | .233 | .126 |
| **C paraphrase** | original-q memory | .150 | .750 | **.850** | .783 | .377 |
| baseline popularity | query-independent | .0 | .200 | .500 | .325 | .098 |

**§54 memory opportunity** (the core result): the RepoDex packet missed the
gold primary path in 135 E1-E3 runs; where an earlier repetition existed (90),
memory returned that missing gold path in the top-10 for **82 runs (91%)**.

## Product answers (§82)

1. **Reusable navigation signal?** Yes — clearly for repeat/paraphrase.
2. **Exact-repeat strength?** Strong: P@10 0.975, MRR 0.468.
3. **Survives excluding the question?** Partially: P@10 0.30 — real but weak
   cross-question transfer (shared entities).
4. **Survives paraphrase?** Yes: P@10 0.85 — the identifier/term signature is
   robust to rewording.
5. **Strongest evidence?** `explicitly_read` + `final_answer_mention`
   (surfaced-only is weakest; structured-EVIDENCE strongest available).
6. **RepoDex surfaces what agents use?** ~33% observed, ~20% read, ~9-11%
   mentioned — most surfaced paths unused (neutral).
7. **Agents find non-surfaced paths?** Yes — ~470-490/treatment discovered,
   ~210 reaching the answer.
8. **Could those help later runs?** Yes — 91% of packet-missed gold recovered
   by prior-rep memory.
9. **Memory beyond deterministic RepoDex?** Union shows MRR 0.146→0.169 and
   gold recovery on packet misses; conditioned memory >> popularity baseline.
10. **Cheap enough per session?** Yes — ~28ms/lookup (process-spawn bound;
    in-process query sub-ms), no raw-scan/git/Jev.
11. **Stale-aware?** Yes — fresh/removed/unknown classified; `changed` wired
    for GitActivity/digest evidence (conservative default `unknown`).
12. **Ready for real A/B?** Yes — substrate is sound; a memory-aware
    zero-context benchmark is the right causal test.

## Scalability (§81)

- No unbounded session-id sets in hot AgentActivity records — moved to
  disk-backed contributions. ✓
- Memory lookup scans postings candidates only, never all episodes. ✓
- Lookup reads no raw AgentEvents. ✓
- Memory update processes only changed investigations (per-investigation
  digest checkpoint). ✓
- 1M-session storage: raw events ~126GB, episodes ~81GB (embedded exposures),
  AgentActivity ~2.8GB, memory ~27GB — all disk-backed. Dominant = episode
  exposure embedding.

## Classification & next step

```text
INVESTIGATION_MEMORY_SIGNAL_CONDITIONAL
```
Strong exact-repeat + paraphrase reuse; conditional cross-question
generalization. Recommend exactly one next task (§84):

```text
MEMORY_AWARE_ZERO_CONTEXT_AGENT_BENCHMARK
```

## Artifacts

`docs/evals/investigation-memory-v1/` + `docs/INVESTIGATION_MEMORY_V1.md` +
`scripts/adapters/`. Memory store (outside Git): `/tmp/memory`; derived:
`/tmp/derived2`; canonical: `/tmp/aes2/store`.

```text
SOURCE_PROMPT_ID: REPODEX-INVESTIGATION-MEMORY-FOUNDATION-V1
PREVIOUS_PROMPT_ID: REPODEX-INVESTIGATION-EPISODE-AGENT-ACTIVITY-FOUNDATION-V1
REPORT_DATE_TIME: 2026-09-23 08:47:35 Europe/Riga
```
