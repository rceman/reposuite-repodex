# TASK 3E — Rust Structural Qualified-Path Call Candidate Expansion — Results

`REPODEX-T3E-RUST-STRUCTURAL-QUALIFIED-PATH-CANDIDATES-V1` adds one bounded
candidate family: a Rust `qualified_path` call whose `::`-separated callee path
is structurally proven to traverse repository modules and terminate at a
source-written `function` produces zero/one/many candidates under the new rule
`rust.call.structural_path_function_candidate`. It remains candidate generation
— never Rust name resolution, never `Exact`/`Resolved`.

## 1–2. Base and implementation SHAs

Resolved base HEAD: `cd635b0ee4cc62bdcd836919c22336c2c04cf1d8` — working tree
clean, `e7ab143` (TASK 3D) is an ancestor.

TASK 3E implementation commit: recorded below.

## 3. Candidate rule ABI / version

`CANDIDATE_RULE_ABI_VERSION` 3 → 4; `POLICY_VERSION_RUST_CALL` 3 → 4;
`candidate_fingerprint` regenerated (`rule_abi=4 rust=4`). TASK 3A analyzer and
TASK 3B link ABIs are unchanged — no normalized extraction or structural link
semantics changed.

## 4. New rule id

`rust.call.structural_path_function_candidate` — distinct from the local and
imported plain-name rules; qualified-path records carry it.

## 5–6. Call-fact sufficiency (§28)

The `qualified_path` fact already stores `callee_written` (the `::`-joined
path) and `type_arguments` separately. `::` never occurs inside a segment, so
`split("::")` deterministically recovers root/segments/terminal — the same
`split` normalization TASK 3B uses for `use` targets. **No fact-model extension
needed; no source reparsing.**

## 7–11. Root policy

* `crate::` → the crate-root module (index 0) of every crate whose `mod` tree
  reaches the call's file.
* `self::` → the call's containing module (file's own module path plus inline
  `mod` scope chain).
* `super::` → containing module's parent; `super` may repeat; walking above the
  crate root is a bounded `out_of_scope` (`super_above_crate`).
* Relative `name::` → a structurally-proven `mod` child of the containing
  module only; never lowercase/capitalization inference.

## 12–16. Shadowing / exclusion policy

* A relative root shadowed by a covering `let`/`param` binding or an applicable
  `use` import → `no_candidate` (`path_root=shadowed:<name>`).
* `Type::`/external/`unproven` roots → `out_of_scope` (`path_root=unproven`).
* Imported module aliases (`use x::util as u`) are not followed — the import
  owns the name → `no_candidate`.
* Re-export traversal, glob, prelude, cfg evaluation, external crates,
  turbofish: all out of scope.

## 17–19. Terminal and traversal

Only `DeclarationKind::Function` terminal declarations are candidates (verified
against the real declaration, not only the module listing). Intermediate
segments descend the persisted `children` module edges; ambiguous module names
keep the whole index set (never pick-first). Module files and inline `mod`s are
both supported via the TASK 3B module tree.

## 20–26. Real-corpus result (tokio, 9,319 `qualified_path` calls)

Pre-conversion root census:

```text
no_crate                    8,458
turbofish                     299
root_shadowed                 289
external_or_unproven_root     247
relative_structural_module     12
crate                          10
super                           2
self                            2
```

Outcome conversion:

```text
single_candidate     24   (crate 9, self 2, super 2, relative module 11)
no_candidate        291
out_of_scope      9,004
```

No `multiple_candidates` on this corpus.

## 27–33. Audit

`scripts/task3e_audit.py` rebuilds the crate module tree independently from
snapshot facts (no production candidate code) and re-derives the expected
candidate set for **all 9,319** `qualified_path` calls:

```text
CORRECT_SINGLE_STRUCTURAL_PATH      24
CORRECT_NO_CANDIDATE               291
CORRECT_OUT_OF_SCOPE             9,004
FALSE_STRUCTURAL_PATH_CANDIDATE      0
WRONG_STRUCTURAL_PATH_TARGET         0
ROOT_SHADOWING_ERROR                 0
MISSING_STRUCTURAL_PATH_CANDIDATE    0
```

## 34–38. Regression / determinism / invalidation

* TASK 3D audit (`--all`, 799 files) on the 3E artifact: `STRUCTURAL_PATH
  handled/missing 9,319/0`, all plain-name/import verdicts unchanged,
  TP/FP/FN 1396/0/0.
* Repeated build → byte-identical; identical across output dirs and checkout
  roots (`/tmp/t3e-xr` and `/tmp/t3e-xr2` produced the same
  `candidate sha256:177e48b9…`).
* Stale TASK 3D artifact under the 3E binary → `RuleFingerprintMismatch`.
* Update-vs-fresh: 9 new structural cases (module fn add/remove, module rename,
  call-path change, module file add/remove, inline module add, shadowing
  binding add/remove) all produce identical candidate digests/bytes.

## 39–41. Size / perf / RSS

```text
artifact: 18,131,386 → 19,024,783 B  (+893,397, +4.93%)
build ~1.4–1.7 s, RSS ~182 MB
```

## 42–45. Findings

`docs/TASK3E_FINDINGS.md` — T3E-F001…F010. No new BLOCKER/HIGH; T3B-F004
re-export and module-tree reachability bounds are carried forward.

`cargo fmt --all -- --check`, `cargo check --locked`, `cargo test --locked`
(400), `cargo clippy --locked --all-targets --all-features -- -D warnings`,
`cargo build --locked --release` — all PASS.

## 46. Completion

`TASK3E_COMPLETE`.
