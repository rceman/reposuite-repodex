# TASK 3E findings log

`REPODEX-T3E-RUST-STRUCTURAL-QUALIFIED-PATH-CANDIDATES-V1` — Rust structural
qualified-path call candidate expansion. A `qualified_path` call whose
`::`-separated callee path is structurally proven to traverse repository
modules and terminate at a source-written `function` now produces zero/one/many
candidates under `rust.call.structural_path_function_candidate`
(`candidate_rule_abi_version` 4). This is candidate generation, never Rust name
resolution.

## T3E-F001 — `STRUCTURAL_PATH`, INFO — normalized call fact is sufficient (§28)

The `qualified_path` call fact stores `callee_written` — the `::`-joined path
exactly as written — and `type_arguments` separately. `::` never occurs inside
a path segment, so `callee_written.split("::")` deterministically recovers the
segments (the same `split` normalization TASK 3B applies to `use` targets).
This is deserializing a normalized fact, not reparsing source. No fact-model
extension was required.
**Disposition: implemented from existing facts.**

## T3E-F002 — `STRUCTURAL_PATH`, INFO — module traversal reuses TASK 3B structure

The rule calls `Structure::build` on the snapshot once and descends the
persisted `children` module edges — the same `mod`→file/`mod`→inline resolution
TASK 3B uses. No module-file discovery is re-implemented. `crate`/`self`/
`super`/`relative` roots resolve to module-index sets; `super` may repeat.
**Disposition: implemented.**

## T3E-F003 — `ROOT_CLASSIFICATION`, INFO — real-corpus root census (tokio, 9,319 `qualified_path`)

```text
no_crate (file not in a crate mod tree)   8,458
turbofish                                   299
root_shadowed (binding/import owns root)    289
external_or_unproven_root                   247
relative structural module                   12
crate                                        10
super                                         2
self                                          2
```

`no_crate` dominates because most `qualified_path` calls live in
test/bench/example files that no `lib.rs`/`main.rs` `mod` tree reaches — under
the bounded model a `crate::`/`self::`/`super::` in such a file cannot be
proven. **Disposition: documented.**

## T3E-F004 — `ROOT_CLASSIFICATION`, INFO — type/associated calls land in unproven/shadowed

`Type::assoc`/`external::x` calls resolve to `external_or_unproven_root` (247)
or `root_shadowed` (289, e.g. `Poll`/`Arc`/`Pin` bound by `use std::…` or a
local binding). Capitalization is never used as a classifier — a root only
becomes a module when a `mod` child proves it. **Disposition: implemented.**

## T3E-F005 — `SHADOWING`, INFO — relative roots are suppressed by nearer bindings

For `util::helper()`, a covering `let`/`param` `util` or a `use …::util`
binding makes the prefix a value/import expression, not a module — the rule
returns a `no_candidate` (`path_root=shadowed:<name>`). 289 tokio calls hit
this, mostly imported type names used as associated-call prefixes.
**Disposition: implemented.**

## T3E-F006 — `UPSTREAM_STRUCTURE`, MEDIUM — conversions are bounded by module-tree reachability

On tokio, `qualified_path` converts to `single_candidate` 24 times
(`crate` 9, `self` 2, `super` 2, relative module 11). The dominant mass is
`Type::`/`external::` associated calls (need type/dependency resolution) or
calls in files outside the crate `mod` tree — both deliberately out of scope.
**Disposition: documented, expected bound.**

## T3E-F007 — `REEXPORT`, MEDIUM — re-export paths not followed (T3B-F004 carried forward)

A `pub use` re-export is not a structural `mod` child, so a path that only
reaches a name through a re-export stays `no_candidate`/`out_of_scope`. TASK 3E
adds no transitive re-export semantics.
**Disposition: documented; deliberately unsolved.**

## T3E-F008 — `STRUCTURAL_PATH`, INFO — turbofish-qualified callees out of scope (§34)

`module::foo::<T>()` keeps `type_arguments` normalized separately; the V1 rule
does not resolve generic instantiation, so turbofish `qualified_path` calls
(299 on tokio) are `out_of_scope`. **Disposition: documented exclusion.**

## T3E-F009 — `ARTIFACT`, INFO — artifact +4.93% for path provenance

`call_candidates` records carry `written`, `terminal`, `path_root`, and
`module_descent` evidence; total artifact grew 18,131,386 → 19,024,783 bytes
(+893,397, +4.93%). Provenance uses locators/segment names, not copied records.
**Disposition: implemented.**

## T3E-F010 — `UPSTREAM_STRUCTURE`, MEDIUM — `super`/relative in non-crate files unreachable

`self::`/`super::`/relative roots require the call's file to be inside a crate
`mod` tree; tokio test/bench files are not, so their qualified calls are
`out_of_scope` (`no_crate`). Widening to per-file crate roots is a structure
change, not a candidate-rule change. **Disposition: documented.**
