# Symbol Exposure Foundation V1 — Report

Extends the historical Agent evidence chain from file-level to symbol-level:

```text
AgentEvent → SourceObserved → InvestigationEpisode → SourceExposure
                                ↓ (new)
                            SymbolExposure   ← + RepoDex parsed intelligence
                                ↓
                     symbol AgentActivity / Investigation Memory symbols
```

`SymbolExposure` is a **derived, rebuildable** fact: RepoDex can prove that
source bytes/ranges associated with a symbol were contained in content exposed
to an Agent. It never claims the model attended to, reasoned about, or found a
symbol useful.

## What was built

| Component | File | Role |
|-----------|------|------|
| Model | `src/symbol_exposure/model.rs` | `SymbolExposure`, `ExposureKind`, `OverlapClass`, `ResolutionState`, `Certainty` |
| Provider | `src/symbol_exposure/provider.rs` | content-addressed symbol-map provider: indexed-digest fast path, digest-keyed parse cache, missing-version detection |
| Resolver | `src/symbol_exposure/resolve.rs` | observed-range → symbol-overlap classification (enclosing / declaration / reference) |
| Store/derive | `src/symbol_exposure/store.rs` | per-investigation docs, resolution diagnostics, incremental fold, final-answer symbol mention join |
| Symbol activity | `src/symbol_exposure/activity.rs` | bounded hot symbol records + disk-backed `contrib.jsonl` (§34/§35/§77) |
| AgentEvent | `src/agent_event/model.rs` | `SourceObserved` + optional `file_content_digest`, `byte_start`, `byte_end` (backward-compatible) |
| Memory | `src/memory/model.rs` + `store.rs` | `SymbolMemoryEvidence` + `MemoryEntry.symbols`; `enrich_symbols`; `memory query`/`show` surface symbol hints |
| Adapter | `scripts/adapters/devin-atif.py` | emits whole-file `file_content_digest` (pinned corpus only) |
| CLI | `src/cli.rs` | `symbol-exposure derive|investigation|source|activity|stats`; `memory build --symbol-exposure` |

## Mandatory questions (§89)

**1. Can RepoDex determine which exact symbols were contained in source exposed to an Agent?**
Yes. For every `source_observed` event with a resolvable content version, the
resolver maps the observed line range onto the `FileAnalysis` for that exact
version and emits one `SymbolExposure` per overlapping declaration/reference/
call. 672,556 exposures were derived across 240 investigations bound to 675
exact content digests.

**2. Can it distinguish enclosing / declaration / reference occurrence?**
Yes — three explicit `ExposureKind`s with deterministic overlap semantics:
`declaration_occurrence` (the decl name row was shown), `enclosing_declaration`
(observed window inside the decl body), `reference_occurrence` (a parsed
reference/call occurrence in the window). Nested declarations are all reported
with `depth` + an `innermost` flag. Replay: 84,260 declaration + 8,823
enclosing + 579,473 reference occurrences.

**3. Can it preserve FACT/CANDIDATE uncertainty?**
Yes. Resolved references carry `certainty=fact` + a `decl:path#id` target;
call-like callees and unresolved references stay `certainty=candidate` with the
written name — no candidate is silently promoted to semantic truth.

**4. Can it bind exposure to the exact file content version seen?**
Yes. `SourceObserved.file_content_digest` (whole-file sha256) is matched to the
indexed `IndexedFile.content_digest`. An unseen digest with available bytes
parses and caches a new version; an unseen digest without bytes yields
`unresolved_missing_source_version` — never a stale-map guess. `repo_head` is
kept as provenance, not identity.

**5. How often can indexed symbol maps be reused without reparsing?**
95.46% of resolvable observations (14,532 / 15,223) hit the indexed digest —
the dominant cheap path, exactly as intended for read-only investigation.

**6. How often is a new source-version parse required?**
0 on this pinned corpus (all observed digests matched the index). The path is
implemented and tested (contract tests cover unseen-version parse + cache).

**7. Cache-hit symbol-resolution latency?**
mean **0.019 ms**, p50 0.011 ms, p95 0.016 ms, p99 0.026 ms (240-decl Go file).

**8. Cache-miss / new-version parse latency?**
~**16 ms** per unseen version (240-decl Go file, tree-sitter parse + extract).
Parsed once per digest, then cached.

**9. How much source snapshot storage?**
Indexed versions are **not** re-snapshotted (the manifest already holds them) —
only the digest is referenced. Unseen versions persist their source bytes once
per digest in the `SourceStore`. The dominant cost is the **normalized exposure
provenance records**: ~783 MB for 240 invs (~3.3 MB/inv, ~1.16 KB/record) —
verbose JSON; see STORAGE.json for the projection and the compaction path.

**10. Does dedup prevent repeated reads multiplying storage?**
Yes. Repeated observations of one digest reuse the cached symbol map; the file
is parsed/snapshotted once per unique version, not per read (§11, §13).

**11. Symbols exposed per historical investigation?**
mean **634 unique symbols** / investigation across ~40 files (≈16 symbols/file);
270 with a literal `declaration_occurrence`. Per-treatment means are in
SYMBOL_EXPOSURE_REPLAY.json.

**12. How often do final-answer symbols correspond to earlier exposure?**
**100%** of investigations had ≥1 exposed symbol whose exact name appears as a
whole token in the final answer (bounded match, §36); mean ~29 such symbols.

**13. Does symbol-level memory outperform/sharpen file-level memory offline?**
It **sharpens** rather than replaces. `PrimarySymbolHit@K` is a stricter target
than `PrimaryPathHit@K`, so absolute numbers are lower (e.g. exact-repeat S@10
0.225 vs P@10 0.70) — but when file memory recalls the gold file, the symbol
evidence isolates the gold symbol within it **100%** of the time, and enables
`path + symbol + range` hints instead of bare paths (§54).

**14. Does it recover useful distinctions inside large/hot files?**
Yes — the clearest result. The hottest file (`tsk585_..._regression_test.go`)
was observed by 156/240 investigations exposing 225 distinct symbols, but each
investigation exposed a different subset (mean 121, range 3–225) — symbol
evidence discriminates *which region* of a shared file mattered where file-level
memory can only say "the file". Example: `task_execution_review.go` was seen by
98 invs, but the L3-Q5 gold symbols (`TaskExecutionReviewDecide`, …) were
exposed in only 48 — exactly the within-file discrimination file memory lacks.

**15. Is enrichment cheap enough to run asynchronously per SourceObserved?**
Yes — cache-hit resolution is ~0.02 ms/event (~80k events/sec single-threaded),
so a background worker comfortably outruns real Agent event rates. New-version
parses (~16 ms) occur once per unique digest, not per read.

**16. Ready to rerun the memory-aware zero-context benchmark with symbol-aware memory?**
Yes — the symbol evidence is correct, version-pinned, cheap, and exposes the
precise (path + symbol + range) navigation hints a symbol-aware treatment needs.

## Classification

**`SYMBOL_EXPOSURE_SIGNAL_CONDITIONAL`**

The foundation is solid and the reach signal is strong — every investigation's
exposure captured the gold symbols (100% via `declaration_occurrence`), and the
evidence cleanly discriminates within hot files. It is *conditional*, not
*strong*, because raw "symbol was exposed" is broad (≈634 symbols/inv — nearly
every symbol in every read file), so standalone symbol ranking is noisy; the
value emerges when exposure is **weighted by kind/provenance** and **composed
with file-level memory** (file picks the file, symbol picks the region). No
production weights were tuned (§39) — this is an offline signal measurement.

## Recommendation

`MEMORY_AWARE_ZERO_CONTEXT_AGENT_BENCHMARK` — with symbol-aware memory/context
(`path + symbol + bounded range` hints) as an explicit treatment dimension.
**Not executed** (§82, §91).

## Existing behavior invariants (§87)

`QUERY_RANKING_UNCHANGED=true`, `RDX1_UNCHANGED=true`,
`TEMPORAL_RANKING_UNCHANGED=true`, `SYSTEM_ONE_BEHAVIOR_UNCHANGED=true`,
`MEMORY_SCORING_UNCHANGED=true`, `AGENT_EVENT_BACKWARD_COMPATIBLE=true`,
`HARNESS_SPECIFIC_PRODUCTION_DEPENDENCIES=0`.

Memory `query`/`show` return identical path rankings with and without symbol
enrichment (verified); symbol evidence is additive display data only.

## Gates (§88)

`cargo fmt --check` ✓ · `cargo check --locked` ✓ · `cargo test --locked` ✓
(all suites incl. 10 new symbol-exposure contract tests) ·
`cargo clippy --locked --all-targets --all-features -D warnings` ✓ ·
`cargo build --locked --release` ✓ · harness-neutral guard ✓.

## Provenance

`SOURCE_PROMPT_ID: REPODEX-SYMBOL-EXPOSURE-FOUNDATION-V1`
`PREVIOUS_PROMPT_ID: REPODEX-INVESTIGATION-MEMORY-HARNESS-NEUTRAL-ADAPTER-ADDENDUM-V1`
`REPORT_DATE_TIME: 2026-09-23 10:55:00 Europe/Riga`

Final status: `SYMBOL_EXPOSURE_FOUNDATION_COMPLETE` · `SYMBOL_AWARE_MEMORY_BENCHMARK_READY`

PROMPT_ID: REPODEX-SYMBOL-EXPOSURE-FOUNDATION-V1
