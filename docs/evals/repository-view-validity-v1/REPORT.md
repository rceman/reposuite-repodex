# RepositoryView Dependency Validity Gate V1 — Report

- `SOURCE_PROMPT_ID: REPODEX-REPOSITORY-VIEW-DEPENDENCY-VALIDITY-GATE-V1`
- `PREVIOUS_PROMPT_ID: REPODEX-PREPARE-ASTRA-RESEARCH-SNAPSHOT-V1`
- `REPORT_DATE_TIME: 2026-09-24 12:05:00 Europe/Riga`

## Contracts inspected (§2)

- `view::manifest::compute` / `ViewManifest::fingerprint` — effective source manifest.
- `view::index::ensure_index` / `build_view_snapshot` — index reuse + snapshot.
- `repository::artifact::object_key(_scoped)` — content-addressed FileAnalysis.
- `repository::fingerprint::{AnalyzerFingerprint, SnapshotConfig}` — producer/config identity.
- `links::{model::LinkFingerprint, artifact::{verify, LinkManifest.metadata}}` — topology/link deps.
- `candidates::model::CandidateFingerprint`, `graph::model::GraphFingerprint` — derived policies.
- New: `view::validity::{ValidityInputs, InvalidReason, index_dir}` — the single gate (§17).

## Pre-fix reproductions (§3, §31)

See `PRE_FIX_REPRODUCTIONS.json`. REPRODUCED: metadata-only change, nested-manifest
addition, metadata removal, same-size same-second edit, config mismatch,
analyzer-unscoped object key, mid-build digest/bytes divergence. NOT_APPLICABLE:
branch-keyed partitioning (already content-keyed).

## Answers (§35)

1. **Reproduced pre-fix:** metadata-only go.mod change; nested-manifest addition (pure metadata); metadata removal; same-size same-second edit; snapshot-config mismatch; analyzer-unscoped object key; digest-A/bytes-B capture.
2. **Did not reproduce:** branch-keyed partitioning (index was already content-keyed; cross-worktree reuse already correct and is preserved).
3. **Metadata-only Go/Cargo changes reusing incompatible graph:** YES pre-fix — go.mod/Cargo.toml were never in the manifest, so the index key was blind to them. Fixed: metadata digests are in `ViewManifest.metadata` + the fingerprint.
4. **New nested manifest escaping verification:** YES pre-fix — a source-only fingerprint could not see a metadata-file addition. Fixed: metadata inventory is hashed into the fingerprint.
5. **Same-size rapid dirty edit reusing old digest:** YES pre-fix — cache key was `(mtime_secs, size)`. Fixed: key is `(mtime_ns, size, change_ns)`; a rewrite bumps ctime even with a backdated mtime, so same-size same-second edits rehash.
6. **Authoritative for source-content identity:** the SHA-256 `content_digest` of the bytes read. The `(mtime_ns,size,change_ns)` stat triple is only a skip-rehash *hint*; the hash is truth (§9).
7. **Coherent hash/analyze binding:** `build_view_snapshot` computes `content_digest(read.bytes)` at parse time and records/uses THAT digest (reuse path only trusts the manifest digest when the file's stat is unchanged since manifest). The index is published under the fingerprint of the digests actually used.
8. **Source changes during capture:** the changed file is re-read/re-hashed, flagged `diverged`, and the index is keyed/published on the *actual* captured fingerprint — never "digest A + analysis of B".
9. **Producer/config identities in reuse validation:** `AnalyzerFingerprint`, `SnapshotConfig`, `LinkFingerprint`, `CandidateFingerprint`, `GraphFingerprint` — all folded into `validity_key`.
10. **Identical-input cross-worktree reuse:** YES — identical complete inputs (content+metadata+producers+config) yield the same `validity_key` → shared index; no branch/path partitioning.
11. **Extra hashing on unchanged warm query:** none — the fast path is stat-only; `files_hashed=0` (~4.8 ms for 200 files).
12. **Warm-query latency before vs after:** ~unchanged (~4–5 ms warm); validity check is a stat scan + one hash-free compare.
13. **Work after a one-file dirty edit:** 200 stats, 1 hash, 1 parse, 199 FileAnalysis reused; derived artifacts rebuilt.
14. **Work after metadata-only change:** metadata re-hashed; 0 source reparses; derived artifacts rebuilt.
15. **Service vs direct semantics:** identical — both route through `ensure_index`; the gate lives there (`api.rs` → `run_view_query` → `ensure_index`).
16. **Known remaining correctness issues:** none identified; residual edge is a metadata file changing *during* a single build, which self-invalidates conservatively on next query (the index records what was actually consulted).
17. **Safe for target-view-bound symbol memory:** YES — reuse is now exact-input-bound, so a downstream feature can trust a hit to describe the current view+contract.

## Storage

Identical-input second view shares one index dir (~0 incremental disk, ~KB manifest cache). No per-branch duplicate index (`BRANCH_KEYED_CACHE_PARTITIONING=false`).

## Classification

`REPOSITORY_VIEW_DEPENDENCY_VALIDITY_READY`
Next task: `REPODEX_FAITHFUL_BOUNDED_EVIDENCE_DELIVERY` (not implemented).

## Final status

`REPODEX_REPOSITORY_VIEW_DEPENDENCY_VALIDITY_GATE_COMPLETE`
