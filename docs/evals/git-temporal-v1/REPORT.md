# Git Temporal Activity Foundation V1 — Report

```text
GIT_TEMPORAL_ACTIVITY_FOUNDATION_COMPLETE
TEMPORAL_SIGNAL_ASSESSMENT_READY
signal_category: TEMPORAL_SIGNAL_CONDITIONAL
```

RepoDex base `9762380` → impl `49efbc1`. Ranking untouched; no Jev; no Agent
rerun; no co-change graph.

## Implementation (§63)

- **Schema**: `FileTemporal{path,first_seen_commit/at,last_changed_commit/at,
  change_commit_count,change_ts_tail[<=128]}` + `TemporalManifest`
  (schema/policy/repo-identity/root_commit/indexed_head/indexed_head_ts/
  semantics/content_digest/artifact_bytes/build stats).
- **Git semantics**: change event = non-merge `name-status` diff vs first
  parent (merges contribute nothing → no double-count); committer `%ct`; ages
  anchored to indexed HEAD ts (deterministic). Rename = `D old`+`A new`,
  path-centric, history not carried. Delete/recreate = one path history.
- **Build**: one streaming `git log` subprocess, line-parsed, O(files) memory.
- **Update**: `OLD..NEW` iff old head is ancestor, else `history_diverged`;
  repo-identity check rejects wrong-repo artifacts.
- **Persist**: staging → records → digest → manifest → atomic rename.
- **Lookup**: `TemporalIndex` (BTreeMap) over the artifact; zero Git.

## Performance (§64)

| corpus | commits | files | events | build ms | peak RSS | artifact |
|--------|---------|-------|--------|----------|----------|----------|
| GTW `f09d083` | 969 | 1,486 | 7,025 | 875 | 15.2 MB | 494 KB |
| 3dpixels | 455 | 3,550 | 7,390 | 919 | 8.3 MB | 1.14 MB |
| bugbounty | 237 | 4,409 | 7,184 | 628 | 9.0 MB | 1.45 MB |

- ~7–8k change events/sec, ~333 B/tracked file, ~70 B/change event.
- **Incremental** (bugbounty 188→237 commits): processed only **49** commits /
  828 events, 62 ms, 9.5 MB — scales with new history, not total.
- **Lookup**: 1000 lookups — mean **1.10 µs**, p50 0.99 µs, p95 1.65 µs,
  max 30 µs (index load 97 ms once). ~1 µs/op.
- **Git subprocesses**: build = 1 `git log` (+ ~4 tiny rev-parse/config);
  incremental = 1 `git log OLD..NEW`; **lookup/query = 0** (strace-verified).
- §59 review: per-file git subprocesses 0; full-history-in-RAM 0 (streaming);
  unbounded commit×file maps 0 (O(files)); duplicate history storage 0;
  query-time git 0.

## Signal (§65) — graded against frozen 20-question gold

Spearman / ROC-AUC of each temporal feature vs frozen relevance (grade ≥2):

| feature | Spearman | ROC-AUC |
|---------|----------|---------|
| **change_commit_count** | **0.229** | **0.826** |
| changes_90d / _365d | 0.229 | 0.826 |
| change_frequency/yr | 0.078 | 0.695 |
| **recency (last-changed age)** | **0.066** | **0.531** |

Per-level (changes→grade AUC): **L2 0.93**, **L4 0.84**, L3 0.84, L1 0.68.
Recency is ~random everywhere (AUC 0.50–0.61).

### Reading
- **Historical activity/churn is a real signal**: frequently-changed files are
  much more likely to contain the answer (AUC 0.83; strongest on multi-hop L2
  and deep L4). Effect is real but moderate — not a clean separator.
- **Recency is not a signal**: last-changed age is near-random vs relevance.
  GTW is a recently-active repo; relevant and irrelevant files are similarly
  "recent", so recency cannot discriminate.

## Naïve ranking simulations (§66) — offline only

Re-sorted each frozen deterministic candidate list by one feature, re-graded:

| ordering | nDCG@10 | MRR | prec@10 |
|----------|---------|-----|---------|
| deterministic base | 0.337 | 0.247 | 0.140 |
| **by recency** | 0.336 | **0.130** | 0.140 |
| **by frequency** | 0.399 | 0.190 | 0.215 |
| **by change count** | 0.411 | 0.160 | 0.235 |

- **Sort-by-recency is harmful**: nDCG flat, **MRR collapses .247→.130** —
  recently-touched test files get pushed above the answer. The original
  "sort by last-changed" hypothesis is refuted.
- **Sort-by-activity lifts top-10 recall** (prec@10 +54–68%, nDCG +18–22%) but
  **hurts MRR** — the hottest files are often irrelevant tests/generated/
  plumbing. Good at widening coverage, bad as a primary key.

## Counterexamples (§38/§39)

- **Stable primary vs newer irrelevant** — L1-Q5 answer `internal/hub/ensure.go`
  (31 days old, 5 changes). The 8 most-recent candidates are all irrelevant
  test files (0–5 days). Recency ordering buries the answer.
- **Hot irrelevant** — `internal/mcp/agent_session_test.go` (35 changes, the
  repo's hottest) is irrelevant across many questions; `config/config.go` (19,
  central plumbing), `generic_task_authoring_*_outputs.go` (21, generated) —
  high churn ≠ high relevance.

## Verdict (§67)

**TEMPORAL_SIGNAL_CONDITIONAL.** A real activity signal exists —
`change_commit_count` discriminates relevant files (AUC 0.83) especially on
L2/L4 — but (a) it is moderate, not dominant; (b) recency is noise and naive
recency sorting is actively harmful; (c) the highest-activity files are often
irrelevant test/generated/plumbing noise. Temporal metadata is useful as a
*bounded feature*, not as a primary ordering.

## Next-stage recommendation (§68)

One follow-up: a **bounded deterministic activity prior** — e.g. use
`change_commit_count` (and/or `changes_90d`) only as a *secondary* signal
(tie-break / small bounded bonus within a relevance tier), evaluated offline on
the frozen set first. Cheap, deterministic, no Jev dependency, and directly
tests whether activity can sharpen ordering without dominating it. Do NOT
pursue recency-based sorting. (Alternatively, feed the same bounded features to
the proven-positive Jev rerank role — but the deterministic prior is the
cheaper, lower-risk first experiment.)

## Artifacts

`docs/evals/git-temporal-v1/`: `BENCHMARK_SPEC.md`, `PERFORMANCE.json`,
`GTW_TEMPORAL_SIGNAL.json`, `OFFLINE_RANKING_SIMULATION.json`,
`candidate_features.json` (per-candidate temporal features), `SUMMARY.json`,
`REPORT.md`. Temporal artifacts live outside Git (`/tmp/gtw-temporal`,
`/tmp/temp-3dpixels`, `/tmp/temp-bugbounty`).
