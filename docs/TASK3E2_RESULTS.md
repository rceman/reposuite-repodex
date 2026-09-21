# TASK 3E V2 — Target-Aware Rust Structural Qualified-Path Candidates — Results

`REPODEX-T3E2-RUST-TARGET-AWARE-QUALIFIED-PATH-CANDIDATES-V1` makes the existing
`rust.call.structural_path_function_candidate` rule consume the TASK 3F
crate/target topology. No new candidate family; same rule, now crate-target
aware.

## 1–3. Base / SHAs / ABI

* Base: `f3af106736c3592705506840f44d12003ccd91cf`.
* `CANDIDATE_RULE_ABI_VERSION` 4→5, `POLICY_VERSION_RUST_CALL` 4→5, candidate
  fingerprint regenerated. TASK 3A analyzer ABI and TASK 3B link ABI unchanged.

## 4–16. What changed

* `candidates` now reads the persisted `rust_crate_target` entities and builds
  one `CrateIndex` per target root through the shared `build_rust_crate_tree`
  — the single module-tree implementation (§4, §16). The old
  `structure.rust_crates` (lib.rs/main.rs) index is removed as the candidate's
  crate source; it remains only as a fallback when no topology entities exist
  (no-manifest repos / in-memory tests).
* **One membership**: `crate`/`self`/`super`/relative roots resolve inside that
  target's tree.
* **Multiple memberships**: each target's tree is evaluated and the
  eligible-function set unioned + deduplicated (existing `owners` loop +
  dedup). `SingleCandidate` still does not mean resolved.
* **Cross-target isolation** (§9–§12): `crate::`/`self::`/`super::` never cross
  a target boundary; verified by fixtures + `CROSS_TARGET_LEAK = 0`.
* **Shadowing** (§18): covering `let`/`param` or `use` still suppresses a
  relative root, per target.
* **External/type/re-export/cfg/import-alias** remain out of scope (§19–§23).
* **T3F-F011** (structured `path_segments`) remains open design debt — V2 still
  uses `callee_written.split("::")`; no fact-model change needed (§24).

## 17–18. Fixtures

`tests/target_path_v5.rs` — 12 target-aware cases (integration-test `crate::`
self-resolution, lib-isolation for test/bin/example/bench, two-test isolation,
workspace non-crossing, explicit `[[test]] path`, target-local
self/super/relative, local-binding and import shadowing, multi-membership) + 5
update-vs-fresh cases. All pass. `structural_path_v4`/`import_aware_v3`/
`local_shadow_v2`/`call_candidates` unchanged in behavior via the lib/main
fallback.

## 19–21. Tokio reconciliation

```text
prior no_crate:        8,458
├── still NoKnownCrate: 3,418  → all out_of_scope
└── now attached:        5,040
    ├── single_candidate       3
    ├── no_candidate        3,017   (root shadowed / terminal not a function)
    └── out_of_scope        2,020   (external / type / unproven root)
```

* §30 — the 1,234 projected "relative module" calls: ~3 became
  `single_candidate`; the rest are import-shadowed (`no_candidate`) or
  external/unproven lowercase roots (`out_of_scope`) — the projection
  overcounted structural-relative paths.
* §31 — `crate`/`self`/`super`: projection 1/0/0; actual new `crate` +1, self
  +0, super +0 — matches.

## 22–27. Qualified-path census (9,319)

```text
                     V1      V2
single_candidate      24  →   27
multiple_candidates    0  →    0
no_candidate         291  → 3,308
out_of_scope       9,004  → 5,984
```

The `no_crate → no_candidate` shift reflects files gaining a crate context
without a proven path; `out_of_scope` now means genuinely external/unproven.

## 28–33. Independent audit — `scripts/task3e2_audit.py`

Re-derives a module tree per target root and resolves every call per
membership, with a `CROSS_TARGET_LEAK` check:

```text
qualified_path calls audited:        9,319
CORRECT_SINGLE_STRUCTURAL_PATH          27
CORRECT_NO_CANDIDATE                 3,308
CORRECT_OUT_OF_SCOPE                 5,984
FALSE_STRUCTURAL_PATH_CANDIDATE          0
WRONG_STRUCTURAL_PATH_TARGET             0
CROSS_TARGET_LEAK                        0
ROOT_SHADOWING_ERROR                     0
MISSING_STRUCTURAL_PATH_CANDIDATE        0
```

## 34–36. Multi-membership + still-NoKnownCrate

* Six `tests/support/*.rs` files each belong to several test targets; their
  qualified calls resolve per-membership, no leaks.
* 392 Rust files / 3,418 calls remain `NoKnownCrate` — `tokio/` 328,
  `tokio-util/` 38, `tokio-stream/` 15, `tests-build/` 11 — files not reached
  by any target's `mod` tree. Not forced into a crate.

## 36–38. Plain-name regression / determinism / cross-root

TASK 3D audit `--all`: `FALSE_CANDIDATE 0, WRONG_SCOPE 0, MISSING 0,
FALSE_IMPORTED 0, WRONG_IMPORTED 0, LEXICAL_PRECEDENCE 0`; TP/FP/FN 1396/0/0
unchanged. Cross-root (`xr`/`xr2`, same snapshot+links) → identical candidate
digest `e100932c`. Update-vs-fresh: 5 target-change cases equal.

## 39–40. Size / perf / RSS

```text
candidate artifact: 19,024,783 → 19,082,227 B (+57,444, +0.30%)
build ~1.2 s; RSS ~184 MB
```

## 41–45. Findings / checkpoint

`docs/TASK3E_FINDINGS.md` T3E2-F011…F015; T3F-F011 kept open. No remaining
BLOCKER/HIGH. Gates: fmt/check/test(437)/clippy/release PASS.
`TASK3E2_COMPLETE`. **Rust checkpoint: `RUST_BOUNDED_FOUNDATION_CHECKPOINT_READY`**
— the bounded structural/candidate foundation (target topology + lexical +
import + structural-path candidates) is internally consistent, audited, and
deterministic; remaining gaps are documented scope, not correctness defects.
