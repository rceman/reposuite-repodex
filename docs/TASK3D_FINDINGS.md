# TASK 3D findings log

`REPODEX-T3D-RUST-IMPORT-AWARE-CALL-CANDIDATES-V1` — Rust import-aware
plain-name call candidate expansion. A `use` that blocks a call under the
TASK 3C V2 lexical rule is now resolved through its persisted TASK 3B
`use_path` relationship into zero/one/many `function` candidates under the new
rule `rust.call.imported_function_candidate` (`candidate_rule_abi_version` 3).
This is candidate generation, never Rust name resolution.

## T3D-F001 — `IMPORT_CANDIDATE`, INFO — imported candidates derive only from the persisted link

An import-aware candidate is emitted only where the `use` occurrence already
owns the call name lexically (the V2 `blocked_by_import_binding` case) and the
exact `(path, import_id, item_index)` `use_path` relationship yields eligible
`function` targets. `Exact` yields that function; `Ambiguous` is filtered to
`function`s; `Unresolved`/`OutOfScope`/non-function targets yield a
`no_candidate`. The link is structural evidence, never semantic proof.
**Disposition: implemented.**

## T3D-F002 — `UPSTREAM_LINK_LIMITATION`, MEDIUM — real-corpus recall is bounded by TASK 3B

On tokio all 1,030 import-blocked calls remain `no_candidate`: 903 resolve to
`out_of_scope` links (non-`crate` paths — `std::`, `tokio::`, `criterion::`,
`nix::`, `core::`) and 127 to `unresolved` links (`crate::` paths TASK 3B could
not resolve). The 11 `exact` `use_path` links on the corpus point at type
imports or non-blocking imports, so **0 calls convert to a candidate** here.
The recall expansion is real (the fixture matrix exercises it end to end) but
the corpus does not trigger it — TASK 3B resolves `crate::` paths only, and
tokio's called imports are external. **Disposition: documented, not a
candidate-rule defect; widening it is a TASK 3B change, out of scope.**

## T3D-F003 — `REEXPORT`, MEDIUM — re-export chains are not followed (T3B-F004 carried forward)

`pub use` is a direct import binding, so `use crate::other::reexported_fn` can
still produce a candidate when its *direct* link resolves to a `function`. But
TASK 3D never follows a *transitive* re-export: a `use` whose link names a
re-export declaration (a `use` item, not a `function`) is not an eligible
target and yields `import_target_not_eligible_function`. The T3B-F004
re-export limitation is unchanged and still bounds recall.
**Disposition: documented; deliberately unsolved.**

## T3D-F004 — `LEXICAL_PRECEDENCE`, INFO — closer blockers beat the import

A covering `LocalBindingOccurrence` (definite or ambiguous), a nearer
`const`/`static`, or a nearer `function` still wins before the import path is
reached: `let helper = ...` → `shadowed_by_local_binding`, `const helper` →
`blocked_by_local_constant`, `fn helper` in `run` → the local `single_candidate`.
A call before a later `let` can still take the import. **Disposition: verified
by fixture.**

## T3D-F005 — `AMBIGUITY`, INFO — ambiguous links filter to functions and keep their provenance

When TASK 3B's `use_path` relationship is `Ambiguous`, the candidate path
collects only the eligible `function` targets. One survivor becomes
`single_candidate` but the record keeps `link_outcome=ambiguous`, so a lone
eligible function is never mistaken for a uniquely resolved import.
**Disposition: implemented.**

## T3D-F006 — `FALSE_CANDIDATE`, INFO — audit found zero false, wrong, or precedence candidates

The independent audit (occurrence-driven, link-aware) across all 799 tokio
files reported `FALSE_CANDIDATE 0`, `WRONG_SCOPE_CANDIDATE 0`,
`FALSE_IMPORTED_CANDIDATE 0`, `WRONG_IMPORTED_TARGET 0`,
`LEXICAL_PRECEDENCE_ERROR 0`, `MISSING_IMPORTED_CANDIDATE 0`, candidate
TP/FP/FN = 1396/0/0. **Disposition: acceptance met.**

## T3D-F007 — `ARTIFACT`, LOW — imported records carry compact link provenance

Each import-derived record references `import_link_id`, `import_link_rule`,
`link_outcome`, `import_written` and the excluded-candidate count rather than
embedding the whole link record. The tokio artifact grew from 17,805,856 to
18,131,386 bytes (+325,530, +1.83%). **Disposition: accepted.**

## T3D-F008 — `PERFORMANCE`, INFO — loading link records adds a bounded cost

The candidate build now reads the `use_path` link records once and indexes them
by `(path, import_id, item_index)`; derive time stays ~127 ms on tokio. The
`item_index` key matches TASK 3B's per-item `use_path` indexing, so no extra
resolution is performed. **Disposition: documented; no budget concern.**

## T3D-F009 — `MISSING_CANDIDATE`, MEDIUM — recommended smallest next step

The measured remaining gap is qualified-path resolution: tokio calls reach
imported names through `self::`/`super::`/external-crate prefixes
(`out_of_scope` links, 903 of 1,030) and through `Type::`/`obj.` forms that are
out of the plain-name scope entirely. The smallest valuable expansion is a
bounded **Rust qualified-path call-candidate** rule (`self::foo()`,
`super::foo()`, `module::foo()`) that reuses the same structural-link evidence.
**Disposition: recommended; not implemented here.**
