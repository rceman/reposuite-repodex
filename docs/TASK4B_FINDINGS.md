# TASK 4B — Findings

Go local-binding facts. Identifiers `T4B-F001`….

## T4B-F001 — SHORT_DECL — `:=` new-vs-redeclaration is lexical, not positional

Every identifier on a `:=` LHS is not necessarily newly declared. A name already
declared in the *same* block is a redeclaration (no new binding); a name declared
only in an outer block or the signature is a fresh shadowing binding. The
extractor tracks the current block's declared set (`LexBlock`) to make this
distinction — pure lexical scope tracking, no type checking.

## T4B-F002 — LEXICAL_SCOPE — block scoping is structural, resolved via ranges

Bindings carry `visibility_ranges[]` (half-open bytes). A `:=`/`var`/`const`/
`type` in an inner block covers only that block; after the block ends the outer
binding is again the relevant one. Distinct same-name bindings in sibling/inner
blocks are separate occurrences, never merged.

## T4B-F003 — IMPLICIT_BLOCK — if/for/switch/select clauses via ranges

Go's implicit lexical blocks (`if`/`for`/`switch` initializers, `case`/`default`/
`select` clause bodies, `for`/`range` bodies) are represented by
`visibility_ranges`, not by promoting each to a `Scope` fact. Initializer names
cover the whole statement; clause bodies are isolated per-clause.

## T4B-F004 — RANGE — `:=` declares, `=` does not

`for k, v := range x` emits `range_variable` bindings for `k`/`v` visible in the
loop body. `for k, v = range x` emits nothing — it reuses existing bindings. The
range expression itself is not covered by the new variables.

## T4B-F005 — TYPE_SWITCH — one occurrence, disjoint clause visibility

`switch v := x.(type)` produces a single `type_switch_variable` occurrence whose
`visibility_ranges` are the disjoint per-clause statement lists. The alias is
written once in the guard; it is not duplicated into fake per-clause facts. It
does not leak after the switch.

## T4B-F006 — SELECT — `:=` receive binds per-clause; `=` does not

`case v := <-ch:` emits `select_receive_variable` scoped to that clause's
statement list. `case v = <-ch:` emits nothing. Clause bindings do not leak into
sibling clauses or after the `select`.

## T4B-F007 — LOCAL_TYPE — local `type` is a blocker

`type helper int` inside a function emits `local_type` scoped to the block —
required so `helper(1)` does not attach the package-level `func helper`. No
type-conversion semantics are modelled; only name + visibility.

## T4B-F008 — TYPE_PARAMETER — generic params are plain_name blockers

A generic type parameter `T` in `func f[T any]` is a `plain_name` blocker because
`T(v)` parses as `call_expression` (form `plain_name`), not `type_conversion`.
Persisted as `type_parameter` covering the whole declaration. This resolves §35:
type params DO produce `plain_name` calls and must be blockers.

## T4B-F009 — EXTRACTION_GAP (fixed) — `func` literals in declaration values

Package-level and local `var`/`const`/`type` declarations previously did not
descend into spec values, so `func` literals (and the calls inside them) inside a
declaration value were unreachable — no calls and no bindings. The `visit`
dispatch now descends into `var`/`const`/`type` declaration children, making
`var f = func(x int){ helper() }` fully extracted. This is why Hugo's plain_name
call count rose from 10,608 to 11,134 (+526 newly-reachable calls).

## T4B-F010 — AMBIGUITY — build-tag/generated variants preserved

RepoDex does not evaluate build tags; multiple same-name source declarations can
coexist in a package's fact set. A `plain_name` call whose unblocked package
projection yields multiple functions keeps all candidates (the §55 property is
corpus evidence, not an invariant). Hugo measured 0 multiples.

## T4B-F011 — RECOVERY — malformed input still emits established bindings

A recoverable/malformed file (e.g. a missing close-brace) still emits the
bindings introduced before the error; the binding walker only reads the syntax
Tree-sitter produced. Over-capture is impossible because bindings are emitted
only for concrete introduction constructs.

## T4B-F012 — ARTIFACT — snapshot size +60% on Hugo

Persisting 39,475 bindings raised the Hugo snapshot from 37,008,541 B to
59,285,649 B (+22,277,108 B, +60.2%). This is expected — binding facts are
per-occurrence data. No storage optimization was applied (correctness first);
recorded for awareness.

## T4B-F013 — PERFORMANCE — build ~2.4 s, RSS ~12 MB

Fresh Hugo snapshot build with bindings: ~2.4 s wall, ~12 MB peak RSS (912 files,
reparsed all). Comparable to the pre-binding build; the binding walk is linear in
statement count.

## T4B-F014 — AUDIT — independent oracle TP/FP/FN

`examples/task4b_audit.rs` re-derives the expected binding set from the
Tree-sitter tree using an *independent* traversal and its own block-scope
tracking (it never calls `parser/go.rs`). On Hugo it found TP=39,475, FP=0,
FN=0, 0 range errors, 0 over-capture — the production and independent
implementations agree exactly on every emitted and every expected binding.
