# TASK 2 Findings Log

Validation of the TASK 1 foundation commit
`4a6e8e3fe88be068932279b3bf896c731c812859`.

The boundary under validation is unchanged:

```text
source bytes
→ Tree-sitter
→ language adapter
→ normalized unresolved syntax facts
```

Findings use stable IDs. Categories are drawn from `task2.md` §48. Severities
follow `task2.md` §49 and are assigned conservatively: a bounded grammar or
recall limitation is MEDIUM or INFO, not HIGH, and is not inflated into a
defect.

---

## F001 — Rust rejects a primitive-type name used as a macro name

```text
language:       Rust
repository:     public:clap-rs/clap
path/example:   tests/**  (13 files)
category:       GRAMMAR
severity:       MEDIUM
status:         OPEN — grammar limitation, correctly reported as recovery
```

**Description.** Every recovered file in the entire baseline corpus scan that
was not one of the four intentional `malformed.*` fixtures came from this one
cause. Tree-sitter-rust fails to parse a macro invocation whose macro name is a
primitive-type keyword:

```rust
str![]        // ERROR node
u32![]        // ERROR node
bool![]       // ERROR node
char![]       // ERROR node
usize![]      // ERROR node
f64![]        // ERROR node
i8![]         // ERROR node

foo![]        // clean
vec![]        // clean
```

The reported recovery is a single 1-byte ERROR node at consistent columns.

**Root cause.** In the tree-sitter-rust grammar the macro name position accepts
an `identifier`, but a primitive-type name is produced by the lexer as a
`primitive_type` token, not an `identifier`. The two token kinds are not
interchangeable in that grammar position. This is a grammar-level limitation,
not an adapter error.

**Impact.** 13 files, all in `tests/**` of clap, all recovered — 13 of the 17
Rust recoveries in the baseline scan (the other 4 are the intentional
`malformed.rs` fixture runs). RepoDex reports recovery explicitly and never
claims a clean parse.

**Regression test.** None. The behaviour is grammar-inherent and the correct
outcome (explicit recovery) is already covered by the recovery tests. Adding a
fixture would assert the grammar's limitation rather than RepoDex's correctness.

**Not fixed by design.** Working around it would require preprocessing Rust
source before Tree-sitter, which is outside the syntax-only boundary.

---

## F002 — Calls nested inside Rust macro arguments are not extracted

```text
language:       Rust
repository:     public:tokio-rs/tokio, public:clap-rs/clap, and all Rust corpora
path/example:   tests/**/*.rs  (any file with `assert!(f())` etc.)
category:       EXTRACTION / GRAMMAR
severity:       MEDIUM
status:         OPEN — grammar-inherent recall boundary, documented policy
```

**Description.** The Rust adapter records a macro invocation itself as a
call-like occurrence (`assert!`, `vec!`, `println!`, `format!`), but does **not**
record ordinary calls written inside the macro's argument list:

```rust
assert!(service_res.is_pending());
//       ^^^^^^^^^^^^^^^^^^^^^ recorded by RepoDex?  NO
```

A manual audit of a real tokio window found 14 calls by hand where RepoDex
reported 13: the missing one was `service_res.is_pending()` nested inside
`assert!`.

**Root cause.** Tree-sitter-rust represents macro arguments as a `token_tree`
node whose children are flat lexical tokens (`identifier`, `_literal`,
`primitive_type`, …). There is no `call_expression` subtree inside a
`token_tree`, so the syntax walker cannot see nested calls without
re-parsing the token tree as expressions. This is the same mechanism behind
F001.

**Impact (quantified).** Corpus-wide Rust scan:

```text
Rust files scanned:                                   1,454
macro-invocation calls recorded:                     19,309
macro invocations whose argument text looked like a call:  12,199
```

12,199 is an **upper bound** for call-like sites invisible inside macro
arguments; it counts argument text that *looks* like a call, not confirmed
calls.

**Documented policy.** `task2.md` and the adapter documentation state that macro
expansion is not attempted. Extracting nested macro-argument calls would require
a token-tree expression parser, which is a new analysis layer, not a bounded
fix.

**Regression test.** None added; the boundary is intentional and documented.
The Rust adapter tests assert the macro invocation itself is recorded.

---

## F003 — PHP anonymous-class construction reported a range covering the whole class body

```text
language:       PHP
repository:     public:laravel/framework (and all PHP corpora)
path/example:   src/Illuminate/Support/Facades/*.php and 182 affected files
category:       RANGE
severity:       HIGH  (systematic range corruption)
status:         FIXED
```

**Description.** For an anonymous class construction, the adapter emitted:

```text
callee_written = "class"
callee_range   = the entire anonymous_class node
```

so a five-byte written name (`class`) claimed a `callee_range` covering
attributes, `extends`/`implements`, and the whole class body — for example
`callee_range = 146..900` instead of `146..151`. `expression_range` correctly
covered the whole construction and was left unchanged.

**Root cause.** In `src/parser/php.rs`, the `anonymous_class` branch set
`written = "class"` but reused `builder.range(target)` — the range of the whole
`anonymous_class` node — as the callee range. The node spans `class ... { ... }`,
so the range was far too wide.

**Fix.** Narrow the callee range to the `class` keyword. The `class` keyword is
an anonymous direct child of `anonymous_class`, so the fix locates the direct
child whose kind is `class` and uses its range. This also handles attributed
anonymous classes (`new #[Queue('x')] class extends ...`), where the
`anonymous_class` node begins at `#[` rather than at `class`.

**Pre-fix / post-fix evidence (corpus-wide).**

```text
                      pre-fix     post-fix
PHP files scanned      4,095       4,095
anonymous-class facts    569         569
incorrect callee ranges  569           0
affected files           182           0
```

The first fix reduced incorrect ranges from 569 to 9; the remaining 9 were the
attributed anonymous classes, fixed by locating the `class` token. The final
count is 0.

**Regression test.** `tests/php_adapter.rs::anonymous_class_construction_ranges_only_the_keyword`
asserts, for both a plain and an attributed anonymous class:

```text
callee_written == "class"
callee_range length == 5
source bytes in callee_range == b"class"
expression_range substantially larger than callee_range
```

The fixture `fixtures/php/calls.php` gained both an ordinary and an attributed
anonymous class; `fixtures/expected/php/calls.php.txt` was re-blessed and the
diff contained only the intended anonymous-class additions plus the range
shifts caused by fixture growth.

---

## F004 — PHP trait `use` is not a namespace import (audit artifact, not a defect)

```text
language:       PHP
repository:     public:laravel/framework
path/example:   src/Illuminate/Auth/Access/Gate.php
category:       EXTRACTION
severity:       INFO
status:         RESOLVED — RepoDex was correct; the independent check was wrong
```

An independent regex counted 15 `use` lines in `Gate.php`; RepoDex emitted 14
imports. The 15th was a trait composition:

```php
use HandlesAuthorization;   // inside the class body, not a namespace import
```

RepoDex correctly excluded it. The discrepancy was an artifact of the
line-based check, not an adapter defect. Confirmed by
`tests/php_adapter.rs::namespace_imports_and_trait_composition_are_distinct`.

---

## F005 — Go `_test.go` filename evidence is legitimate without `func Test` (audit artifact, not a defect)

```text
language:       Go
repository:     public:gohain... (Go corpora)
path/example:   a helper `_test.go` file with no `func Test…`
category:       EXTRACTION
severity:       INFO
status:         RESOLVED — RepoDex was correct; the independent check was wrong
```

An independent filename-based check flagged a `_test.go` file as having no test
function. The file contained a helper function but no `func Test…`. RepoDex
emitted a filename-convention test candidate, which is correct: the documented
convention includes `_test.go` filename evidence. Not a defect.

---

## F006 — Retaining parsed trees dominates process memory

```text
language:       all four
repository:     tokio, hugo, django, laravel
category:       MEMORY
severity:       INFO  (design input for TASK 3, not a defect)
status:         OPEN — measured, carried forward
```

The A/B/C retention experiment (see `docs/SPIKE_RESULTS.md` and
`examples/retention.rs`) shows that retaining Tree-sitter trees multiplies peak
RSS roughly 15–18× over the no-retention variant, while additionally retaining
source buffers adds only about the size of those buffers:

```text
corpus    A (release)   B (trees)    C (trees+source)
tokio      10.4 MiB     164.0 MiB     169.6 MiB
hugo        9.2 MiB     158.6 MiB     164.5 MiB
django     20.8 MiB     518.2 MiB     536.8 MiB
laravel    36.2 MiB     650.5 MiB     667.5 MiB
```

Fact counts were identical across all three variants in every language. The
implication for TASK 3 is that a repository map which retains trees per file
will not scale; trees must be released after extraction, or a bounded retention
window must be used.

---

## F007 — Incremental parse is fast, but incremental *extraction* is not

```text
language:       all four
repository:     synthetic + real corpora
category:       PERFORMANCE
severity:       INFO  (design input for TASK 3, not a defect)
status:         OPEN — measured, carried forward
```

Incremental parsing itself is much faster than a fresh parse, but the current
pipeline re-extracts the whole file after any edit, so the end-to-end
incremental gain is bounded:

```text
stage                         rust 100KiB      php 1MiB
fresh parse                     12.845 ms      215.740 ms
incremental parse only           0.675 ms        6.783 ms   (~19×, ~32×)
fresh parse + extraction        22.431 ms      414.228 ms
incremental parse + extraction  10.745 ms      156.137 ms   (~2.1×, ~2.7×)
```

Extraction is not incremental in TASK 1/2; only the parse step reuses the
previous tree. This is expected and is recorded so TASK 3 does not assume a
large end-to-end incremental speedup.

---

## F008 — `RecoveryError::Query` is declared but not producible

```text
language:       n/a (framework)
repository:     n/a
path/example:   src/parser/recovery.rs
category:       MODEL
severity:       MEDIUM
status:         OPEN — carried forward from the TASK 1 correction pass
```

`RecoveryError::Query` (and `DiagnosticKind::QueryError`) are declared but no
code path constructs them. A query that fails to compile is rejected at adapter
construction time by `with_query_source`; Tree-sitter's `QueryCursor::matches`
has no failure signal. `DiagnosticKind::QueryError` predates this work — it
exists in the original `8f3314bc` model — so removing it would be a model
change outside a bounded correction pass. It is documented as unproduced in
`src/parser/recovery.rs` and in `docs/SPIKE_RESULTS.md`, and no test asserts it
is reachable. This is a MEDIUM note for TASK 3, not a blocker.

---

## F009 — PHP language constructs are recorded as `plain_name` calls

```
id:             F009
category:       DOCUMENTATION / EXTRACTION
severity:       LOW
status:         OPEN — documentation gap; recording itself is correct
```

Tree-sitter-php shapes the language constructs `empty($x)` and `isset($x)` as
`function_call_expression` nodes with a `name` callee. The PHP adapter maps that
node kind to a `plain_name` call-like occurrence without special-casing them, so
they are emitted as call-like occurrences.

This is **source-grounded and therefore not a false positive**: the source
literally contains a call-shaped `name(args)` occurrence, and RepoDex reports the
grammar's shape without claiming the construct is a function. The gap is only
that `docs/LANGUAGE_SPIKE.md` listed the call forms without noting that these two
constructs are shaped as calls by the grammar. One clarifying sentence was added
to the PHP quirks section; no code change was made.

Found by the extraction-quality audit (region
`laravel_framework/tests/Integration/Console/PromptsAssertionTest.php`,
occurrences at lines 304 and 371).

---

## F010 — Extraction-audit methodology was weaker than the frozen protocol (validation process)

The frozen plan requires occurrence matching by source anchor / byte span. The
first audit-completion attempt matched by recorded name and `(row, callee name)`
instead, and its source-only scanner was a convenience aid with favourable blind
spots: it excluded `print`/`len`/`append`/`make` and `Some`/`Ok`/`Err` by name,
missed calls on declaration lines and multiline macro token trees, and
mis-computed offsets after string stripping. Equal aggregate counts are not
evidence of zero FP/FN.

Resolution: the audit was re-implemented at occurrence level
(`scripts/task2_occurrence_ledger.py` + `examples/grammar_enum.rs`), matching
literally on byte spans with **zero tolerance matches**, and the earlier counts
were demoted to non-ground-truth. The earlier attempt is preserved for
traceability (report §31) and superseded (report §32). No production defect was
exposed; the corrected result is 730 TP / 0 FP / 13 FN, every match an exact
byte-span equality. M10 steady-state sample counts, which the earlier pass did
not record, are now recorded (`audit/m9-m10-evidence-v3.txt`).

Severity: PROCESS / VALIDATION. Status: RESOLVED by the closure pass.

---

## Findings summary

| ID | Category | Severity | Status |
|---|---|---|---|
| F001 | GRAMMAR | MEDIUM | OPEN — grammar limitation, reported as recovery |
| F002 | EXTRACTION / GRAMMAR | MEDIUM | OPEN — documented recall boundary |
| F003 | RANGE | HIGH | **FIXED** with regression test |
| F004 | EXTRACTION | INFO | RESOLVED — not a defect |
| F005 | EXTRACTION | INFO | RESOLVED — not a defect |
| F006 | MEMORY | INFO | OPEN — TASK 3 input |
| F007 | PERFORMANCE | INFO | OPEN — TASK 3 input |
| F008 | MODEL | MEDIUM | OPEN — carried from TASK 1 |
| F009 | DOCUMENTATION | LOW | OPEN — doc sentence added, no code change |
| F010 | PROCESS / VALIDATION | MEDIUM | **RESOLVED** — occurrence-level audit supersedes the name-based pass |

No unresolved BLOCKER or HIGH finding remains.
