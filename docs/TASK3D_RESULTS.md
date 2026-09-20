# TASK 3D — Rust Import-Aware Plain-Name Call Candidate Expansion — Results

`REPODEX-T3D-RUST-IMPORT-AWARE-CALL-CANDIDATES-V1` adds a single bounded recall
expansion: a plain-name Rust call lexically blocked by an explicit `use` is now
resolved through that exact import occurrence's persisted TASK 3B `use_path`
relationship into zero/one/many `function` candidates under the new rule
`rust.call.imported_function_candidate`. It remains candidate generation —
never Rust name resolution, never `Exact`/`Resolved`.

## 1–3. Base and history preflight

Resolved base HEAD: `8bc06c703c5fdfa0d89f3790b2523c98187fcf15`.
History preflight: `git merge-base --is-ancestor 9933a80… HEAD` → **true**, and
the working tree was clean. `9933a80` (`Close T3C-F005…`) is an ancestor of the
base, so TASK 3D builds on the V2 correction.

## 4. Implementation / report SHAs

Recorded below; see the V3 implementation commit and its SHA-recording commit.

## 5. Candidate rule ABI / version

`CANDIDATE_RULE_ABI_VERSION` 2 → 3; `POLICY_VERSION_RUST_CALL` 2 → 3;
`candidate_fingerprint` regenerated (`rule_abi=3 rust=3`).

## 6. New rule id

`rust.call.imported_function_candidate` — separate from
`rust.call.local_function_candidate` so imported and lexical-local provenance
stay distinguishable. Records emitted by the import path carry this rule id.

## 7–8. Import applicability and alias policy

An import candidate is produced **only where the `use` already blocks lexical
lookup under V2** — never for every same-name import in the file. The bound
local name is the alias when present (`use p::real as name` binds `name`),
otherwise the leaf `::` segment; the candidate keeps the imported *target*
name. Wildcard `use`s bind no single name and never reach this path.

## 9–12. TASK 3B outcome handling

* `Exact` + `function` target → `single_candidate` (provenance `link_outcome=exact`).
* `Ambiguous` → filter structural candidates to `function`s; 0 → `no_candidate`,
  1 → `single_candidate`, 2+ → `multiple_candidates`, with `link_outcome=ambiguous`.
* `Unresolved` → `no_candidate(import_structurally_unresolved)`.
* `OutOfScope` → `no_candidate(import_out_of_scope)`.
* No link for the occurrence → `no_candidate(blocked_by_import_binding)` (conservative).

## 13–14. Non-function target and re-export policy

Only `DeclarationKind::Function` is eligible (verified against the target file's
analysis, not just the link's `declaration_kind`). Non-function / file /
structural targets → `import_target_not_eligible_function`. Re-export chains are
never followed transitively (T3B-F004); a `use` resolving only to a re-export
declaration is not eligible.

## 15. Lexical precedence

The V2 precedence is authoritative and unchanged: a covering local binding, a
nearer `const`/`static`, or a nearer `function` all win before the import path;
a call before a later `let` may still use the import. The import never falls
back to an unrelated outer same-name `function`.

## 16. Provenance model

Import records carry `blocker=import`, `blocker_import_id`, `blocker_item_index`,
`blocker_local_name`, `blocker_target`, `blocker_scope`, `blocker_alias`,
`matched_level`, `enclosing_module`, `import_link_id`, `import_link_rule`,
`link_outcome`, `import_written`, `target`/`excluded_non_function(_count)` —
references to the link id/locators, not a copy of the link record.

## 17. Mandatory fixture results

`tests/import_aware_v3.rs` — **21 tests pass**, covering: exact→single,
exact→non-function, ambiguous→filter (multiple / single-with-provenance / none),
unresolved, out-of-scope, missing-link, glob, `let`/param/nested-`fn`/`const`
precedence, call-before-`let`, outside-applicability, no-outer-borrow,
alias, grouped, grouped-alias, re-export, provenance. Plus the end-to-end
`imports_and_reexports_resolve_through_their_link` in `call_candidates.rs`.

## 18–22. Real-corpus (tokio) measurements

```text
import-blocked calls (TASK 3C V2):    1,030
→ SingleCandidate:                        0
→ MultipleCandidates:                     0
→ remain NoCandidate:
      import_out_of_scope (external)    903
      import_structurally_unresolved    127
      (all 1,030 upstream-link-limited)
candidate-bearing calls before/after: 1,396 / 1,396  (single 1,394 + multiple 2)
```

0 conversions is the honest result: TASK 3B resolves `crate::` paths only, and
tokio's called imports are `std::`/`tokio::`/`criterion::`/`nix::` (out_of_scope)
or unresolved. The 11 `exact` links attach to type imports or non-blocking
imports. The expansion is real — the fixture proves it — but the corpus does
not trigger it.

## 23–29. Audit

Methodology: occurrence-driven; for each import-blocked call the audit
independently looks up the exact `(path, import_id, item_index)` `use_path`
link, applies the bounded policy, and compares — never calling the production
rule. Sample: all 1,030 import-blocked calls (all files, `--all`).

```text
audited in-scope calls:    5,345  (+ 37,671 out-of-scope)
candidate TP/FP/FN:        1,396 / 0 / 0
FALSE_IMPORTED_CANDIDATE:  0
WRONG_IMPORTED_TARGET:     0
LEXICAL_PRECEDENCE_ERROR:  0
MISSING_IMPORTED:          0
FALSE_CANDIDATE:           0   WRONG_SCOPE: 0   MISSING: 0
upstream-link-limited:     1,030
```

## 30–33. Determinism / cross-root / update-vs-fresh / stale invalidation

Repeated build → identical `candidate_digest` and record bytes (verified);
across output directories and checkout roots identical (existing tests pass).
Update-vs-fresh: import removed / alias renamed / target-fn added / target-fn
removed / local-fn-blocker added (+ existing let-blocker, import-added, param,
nested-block cases) all produce identical `candidate_digest` + bytes. A stale
V2 artifact fails `RuleFingerprintMismatch` (`a_stale_rule_fingerprint_is_rejected`).

## 34–36. Artifact size / performance / RSS

```text
candidate artifact: 17,805,856 -> 18,131,386 bytes  (+325,530, +1.83%)
records:            43,016 (unchanged)
V3 candidates build: ~1.70 s (load 698 + derive 127 + serialize 40 + verify 695 ms)
peak RSS:            ~175 MB
```

V2 was ~3.09 s / ~166 MB; V3 reads link records (added to load) — times are
machine-load dependent, not a regression.

## 37. Findings

See `docs/TASK3D_FINDINGS.md`: T3D-F001 (implemented), T3D-F002
upstream-link-limitation, T3D-F003 re-export (T3B-F004 carried), T3D-F004
precedence, T3D-F005 ambiguity, T3D-F006 zero false candidates, T3D-F007
artifact, T3D-F008 performance, T3D-F009 recommended next step.

## 38. Remaining BLOCKER/HIGH issues

None — no new BLOCKER/HIGH; T3B-F004 (re-export) remains a documented MEDIUM
limitation that bounds TASK 3D recall.

## 39. Exact Cargo verification

```text
cargo fmt --all -- --check                                       PASS
cargo check --locked                                             PASS
cargo test --locked                                              PASS (371 tests, 0 failed)
cargo clippy --locked --all-targets --all-features -- -D warnings PASS
cargo build --locked --release                                   PASS
scripts/task3c_audit.py --all                                    PASS (0 FALSE/WRONG/PRECEDENCE)
```

## 40. Not measured

Candidate `verify` wall-time not separately re-timed (it is inside build
phases); rule-derivation-only time not isolated beyond the `derive` phase;
memory profile beyond peak RSS not taken; per-item link index memory not
profiled.

## 41. Completion

`TASK3D_COMPLETE` — §29 met: 0 `FALSE_IMPORTED_CANDIDATE`, 0
`WRONG_IMPORTED_TARGET`, 0 `LEXICAL_PRECEDENCE_ERROR`.

## 42–43. Next task, confirmations

Recommended next bounded task: **Rust qualified-path call-candidate expansion**
(`self::`/`super::`/`module::` calls) — the dominant measured gap (903/1,030
out-of-scope links plus qualified calls out of plain-name scope). It was **not
started**. **Nothing was pushed.**

```text
SOURCE_PROMPT_ID: REPODEX-T3D-RUST-IMPORT-AWARE-CALL-CANDIDATES-V1
PREVIOUS_PROMPT_ID: REPODEX-T3C-LEXICAL-SHADOW-CORRECTION-V2
REPORT_DATE_TIME: 2026-09-20 19:21:15 Europe/Riga
```

PROMPT_ID: REPODEX-T3D-RUST-IMPORT-AWARE-CALL-CANDIDATES-V1