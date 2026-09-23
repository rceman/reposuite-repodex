# RepositoryView + Gateway-Ready CLI + Benchmark KPI — Report

RepoDex now correctly serves **multiple repository checkouts/worktrees/views**
via a stable human + machine CLI, content-addressed FileAnalysis reuse, and a
canonical Correctness/Cost/Work/Time benchmark KPI contract.

## What was built

- **`src/view/`** — `model` (RepositoryView, locator, request/response schemas,
  error codes), `git` (HEAD/branch/dirty/common-dir via `git`, graceful),
  `registry` (atomic `~/reposuite/repodex/projects.json`), `resolve` (locator →
  canonical view, traversal+symlink safety), `manifest` (effective path→digest
  fingerprint), `index` (content-addressed shared objects + digest-keyed derived
  index + atomic publish + build lock), `service` (locator→index→query).
- **CLI** — `query --root | --project [--view] --query` (flag/RDX/human),
  `query --json` (stdin request → stdout JSON), `project register|show|list|
  remove|resolve`. `--graph` path unchanged.
- **`docs/evals/BENCHMARK_METRICS.md`** — the KPI contract.
- **`docs/REPOSITORY_VIEW.md`** — concepts + contract + Gateway boundary.
- **Post-hoc** `docs/evals/symbol-memory-agent-v1/COST_TIME_POSTHOC.json` + a
  POST-HOC section in its REPORT (existing telemetry only — §62-§67).

## Correctness (§68, §73) — all assertions pass

`tests/view_cli.rs` 7/7 + manual runs on a multi-branch git fixture
(main + work-a/b/c/d):

- **CWD-independent** — `cd /tmp` + `--root` queries the named view (§5).
- **Per-view symbols, zero contamination** — main→SharedMain, a→SharedA,
  b→SharedB, c→SharedMain; A's symbols never appear in B (§21, §73).
- **Dirty edit** — uncommitted `DirtyNew` becomes query-visible on the next
  query (new fingerprint + incremental rebuild; §22).
- **Added/deleted** — `a.go`/`OnlyInA` visible only in A; deleted `serve.go`
  absent in B, present in main — no stale leakage (§25).
- **Explicit-root ≡ project/view** — identical seeds when resolving the same
  path (§35); `--project` default → registered root (§7).
- **Traversal/symlink** — `../main` rejected `VIEW_PATH_ESCAPE`; containment
  enforced post-canonicalization (§17-§18).
- **Interleaved + concurrent** — A/B/A/C/B correct; 4 concurrent cold builds on
  different views + repeated same-view, no corruption, registry intact (§36-§37).
- **SymbolExposure exact-version** — FileAnalysis is `(path,digest)`-keyed, so a
  view never serves another view's symbol ranges (§38).

## Cost / Work / Time (§69-§71)

Small-fixture timings (cold includes full snapshot→links→candidates→graph):
cold ≈ 36 ms, warm ≈ 28 ms, A→B→A ≈ 28 ms, dirty/add refresh ≈ 32 ms. Agent
token metrics N/A.

**Real GTW smoke test** (§74) on real task-worktrees
(`task-worktrees/gpt-tunnel-gateway/`):
- `GTW-TSK384` cold: 942 files parsed, `ensure_ms` 8,712 → correct seeds.
- `GTW-TSK587` (different branch): **554 FileAnalysis reused + 270 parsed**
  (~67% reuse across different task branches), `ensure_ms` 6,443.
- Warm repeat GTW-TSK384: `idx_reused=true`, `ensure_ms` **23 ms** (~380× vs cold).
- State: 68 MB objects + 122 MB indexes for two ~900-file worktrees.

## Storage / reuse (§75-§76)

Fixture: 9 FileAnalysis objects for ~16 file-instances across 5 views; 3 index
dirs (main+work-c+work-d share one fingerprint). **A new identical-content
worktree costs ~KB (manifest cache only) — `idx_reused=true`, 0 new objects, 0
new index.** `N` identical worktrees do **not** multiply disk by N; different
content reuses shared `(path,digest)` objects (67% on real GTW).

## Machine contract (§30-§35, §44-§45)

`repodex query --json` reads one `reposuite.repodex.query.request.v1` from
stdin → resolves view → `reposuite.repodex.query.response.v1` on stdout only
(`repository_view` + canonical structured `result`). Deterministic error codes
+ non-zero exit. Gateway only ever needs to send `{"query": "..."}` once it maps
Task→view.

## Mandatory questions (§83)

1. **Query arbitrary worktree independent of cwd?** YES.
2. **project+view ≡ explicit root?** YES (same canonical view → identical result).
3. **Same relative path on multiple branches coexists safely?** YES — separate fingerprints/indexes, 0 contamination.
4. **Dirty/uncommitted source queried correctly?** YES — recomputed each query.
5. **How fast does RepoDex observe an edit?** One manifest recompute (stat-only for unchanged) + incremental rebuild — ~ms on small repos, seconds to rebuild only changed files on large ones.
6. **Added/deleted reflected without stale leakage?** YES.
7. **FileAnalysis reuse between worktrees?** ~67% on real GTW branches; 100% for identical content.
8. **Incremental disk/RAM for a mostly-identical view?** ~KB manifest cache + shared objects/index; only changed files' analysis is new.
9. **Does another worktree need a full reparse?** NO — identical → whole-index reuse; shared files → object reuse.
10. **SymbolExposure tied to exact content version?** YES — `(path,digest)`-keyed.
11. **Machine CLI stable enough for Gateway `task/code`?** YES — versioned request/response, deterministic errors, cwd-independent, machine-only stdout.
12. **Gateway Task→RepoDex mapping?** `Task → project alias + view/worktree name → {"project","view","query"}` (or `{"root",...}`). RepoDex stays Task-agnostic.
13. **Additional foundation needed before Gateway integration?** None blocking. Optional hardening: index GC/eviction for many dirty fingerprints, `reasoning_tokens` passthrough if the provider ever exposes it, and a view-index TTL — none required for `task/code`.

## KPI questions (§84)

1. `output_tokens` first-class? YES (BENCHMARK_METRICS.md + posthoc).
2. Reasoning tokens separately only when authoritative? YES — `unavailable` here.
3. Monetary cost from explicit snapshot w/o embedding pricing? YES — documented; `unavailable` (no snapshot for `gpt-5-6-luna-xhigh`).
4. Correctness/Cost/Work/Time documented as the 4 dimensions? YES.
5. time-to-first-primary-evidence defined? YES (wall + tool-call forms).
6. time-to-sufficient-evidence defined conservatively? YES — only when objectively determinable.
7. 300-run benchmark enriched post-hoc with output/time? YES — `COST_TIME_POSTHOC.json` + REPORT section, existing telemetry only.
8. Unavailable retrospective metrics? `reasoning_tokens` (not exposed by provider) and `estimated_model_cost` (no pricing snapshot) — reported as `unavailable`, not estimated.
9. Did post-hoc change the accepted conclusion? **NO** — output tokens ~flat, time improves; verdict unchanged.

## Classification (§85)

**`REPOSITORY_VIEW_GATEWAY_CLI_READY`**

Recommended next task (§86): **`GTW_REPODEX_TASK_CODE_INTEGRATION`** — *not*
implemented here.

## Invariants (§80)

`CWD_INDEPENDENT_QUERY=true` `EXPLICIT_ROOT_SUPPORTED=true`
`PROJECT_VIEW_SUPPORTED=true` `MACHINE_JSON_QUERY_SUPPORTED=true`
`ROOT_PROJECT_LOCATORS_MUTUALLY_EXCLUSIVE=true` `DIRTY_WORKTREE_SUPPORTED=true`
`DIFFERENT_BRANCH_VIEWS_SUPPORTED=true` `CROSS_VIEW_CONTAMINATION=0`
`CONTENT_ADDRESSED_ANALYSIS_REUSE=true`
`SYMBOL_EXPOSURE_EXACT_VERSION_PRESERVED=true`
`PRODUCTION_QUERY_RANKING_UNCHANGED=true` `MEMORY_SCORING_UNCHANGED=true`
`JEV_BEHAVIOR_UNCHANGED=true` `HARNESS_SPECIFIC_PRODUCTION_DEPENDENCIES=0`
`REPODEX_MCP_SERVER_ADDED=false` `GATEWAY_CODE_MODIFIED=false`
`BENCHMARK_KPI_CONTRACT_DOCUMENTED=true`

## Gates (§81)

`cargo fmt --check` ✓ `cargo check --locked` ✓ `cargo test --locked` ✓
`cargo clippy --locked --all-targets --all-features -D warnings` ✓
`cargo build --locked --release` ✓ harness-neutral guard ✓ view_cli 7/7 ✓.
No Agent benchmarks rerun; no Gateway/Relay changes; no push.

## Provenance

`SOURCE_PROMPT_ID: REPODEX-REPOSITORY-VIEW-GATEWAY-CLI-FOUNDATION-V1`
`PREVIOUS_PROMPT_ID: REPODEX-SYMBOL-AWARE-MEMORY-ZERO-CONTEXT-AGENT-BENCHMARK-V2`
`REPORT_DATE_TIME: 2026-09-23 14:30:00 Europe/Riga`

Final status: `REPOSITORY_VIEW_GATEWAY_CLI_FOUNDATION_COMPLETE`

PROMPT_ID: REPODEX-REPOSITORY-VIEW-GATEWAY-CLI-FOUNDATION-V1
