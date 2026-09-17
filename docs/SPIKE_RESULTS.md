# RepoDex Spike Results

Observed results only. Nothing in this document is extrapolated, and nothing
compares RepoDex against another implementation.

```text
status: FOUNDATION_SPIKE_COMPLETE
```

This document describes the **corrected** TASK 1 snapshot. An independent static
review of the first snapshot raised eleven findings; all eleven were reproduced
and corrected, and two further defects were found by the new tests. Section 10.6
records each finding, its verdict and its correction. Section 10.3 lists exactly
which benchmark numbers from the first report are superseded. The original TASK 1
commit is preserved unamended for audit.

Machine and toolchain for every measurement below:

```text
platform:  Linux (WSL2), x86_64
rustc:     1.93.1 (01f6ddf75 2026-02-11)
cargo:     1.93.1 (083ac5135 2025-12-15)
profile:   release (bench and repodex-bench), debug for tests
```

## 1. Adapters implemented

```text
Rust    implemented: modules, structs, unions, enums, variants, traits,
        trait methods, impl blocks, associated consts and type aliases,
        type aliases, consts, statics, functions, nested functions, closures,
        macro_rules! definitions, extern blocks, fields, use declarations,
        impl/impl-trait/receiver references, seven call forms, #[test] evidence
Go      implemented: packages, named types, aliases, generic types, structs,
        fields, interfaces, interface method elements, consts (grouped, iota),
        vars, functions, methods with receiver references, embedded types,
        four call forms plus grammar-labelled type conversions, Test/Benchmark
        name and signature evidence, _test.go file evidence
Python  implemented: imports (all forms, relative levels), module variables,
        Final-annotated constants, classes, methods, async methods, functions,
        async functions, nested functions, nested classes, lambdas, decorators,
        base classes, plain/member/chained/indirect calls, pytest and unittest
        candidate evidence
PHP     implemented: namespaces (bracketed and unbracketed), classes,
        interfaces, traits, enums, enum cases, methods, constructors, static
        methods, properties (static, readonly, promoted, visibility), class and
        interface constants, imports (grouped, aliased, function, const),
        base/interface/trait-composition references, closure captures,
        include/require targets, first-class callables, six call forms,
        test prefix and #[Test] evidence
```

## 2. Fixtures

```text
fixtures/rust/     boundaries.rs calls.rs declarations.rs impls.rs imports.rs
                   malformed.rs tests.rs
fixtures/go/       boundaries.go calls.go declarations.go malformed.go
                   sample_test.go
fixtures/python/   boundaries.py calls.py declarations.py malformed.py
                   test_sample.py
fixtures/php/      boundaries.php calls.php declarations.php malformed.php
                   tests.php
fixtures/shared/   adversarial.rs adversarial.go adversarial.py adversarial.php
fixtures/expected/ complete canonical fact set of every fixture (26 files)
```

`fixtures/expected/**` is generated from the implementation and reviewed. It is
never the only evidence: every language suite asserts extraction semantics
independently, and `tests/recovery.rs` re-derives the recovery policy from the
model rather than from the expected files.

## 3. Tests executed and results

Command:

```bash
cargo test --locked
```

Result: **152 tests, 152 passed, 0 failed.**

```text
src/bin/repodex-bench.rs   7 passed   (unit tests: generated sources, the
                                       language-correct incremental edit, the
                                       nested depth limit, stage boundaries,
                                       JSON rows)
tests/canonical_completeness.rs  3 passed
tests/cli.rs              14 passed
tests/determinism.rs       8 passed
tests/expected_facts.rs    1 passed
tests/go_adapter.rs       12 passed
tests/incremental.rs      10 passed
tests/paths.rs             9 passed
tests/php_adapter.rs      12 passed
tests/python_adapter.rs   13 passed
tests/query_limits.rs      7 passed
tests/ranges.rs            6 passed
tests/recovery.rs          8 passed
tests/rust_adapter.rs     13 passed
tests/scanner.rs          11 passed
tests/shared_fixtures.rs  12 passed
tests/tree_comparator.rs   6 passed
```

`tests/expected_facts.rs` compares the complete canonical fact set of all 26
fixtures, so one test covers 26 fixture expectations.

Three suites exist specifically so the evidence claims are enforced rather than
asserted:

```text
tests/canonical_completeness.rs   walks every fact of every fixture, mutates
                                  every field of every fact (6002 mutations) and
                                  fails if any mutation is invisible to either
                                  `same_facts` or the canonical text
tests/tree_comparator.rs          pins every property the tree digest compares,
                                  and proves the digest separates trees that
                                  differ in kind, bytes, points, namedness,
                                  field names, leaf text or recovery state
tests/query_limits.rs             drives the match-limit guard with a query that
                                  genuinely exhausts the capture pool, and pins
                                  that the production query cannot
```

## 4. Declaration extraction observations

* Duplicate written names in different lexical contexts are all present and never
  merged. `tests/rust_adapter.rs`, `tests/php_adapter.rs` and
  `tests/shared_fixtures.rs` assert this on real fixtures.
* Go receiver methods are declared at file scope. Their source-grounded identity
  is name + file scope + written receiver, which is why the receiver is recorded
  as a reference and why the shared adversarial fixture needed
  `declaration_identity` to stay unique.
* A Rust `impl` block is a scope, never a declaration of its target. The same
  method name in two impl blocks stays distinguishable through the impl scope's
  display path, which borrows the written target.
* Rust `union` is reported as `union`, not as `struct`. This was a real defect
  found during documentation review: `union_item` had been routed to the struct
  handler, which produced a wrong declaration kind.
* Rust `extern "C"` blocks open an `extern_block` scope. Before that was added,
  foreign function signatures were attributed to the file scope, losing
  containment.
* Anonymous scopes (closures, lambdas, arrow functions) never receive a
  fabricated name.
* No declaration is emitted when the source does not write a name. `field: ,`
  inserts a MISSING type identifier and produces no invented field; `var
  incomplete =` produces no variable; `class Incomplete(` produces no class.
* Upper-case Python names are `Variable`, not `Constant`. Only an explicit
  `Final` annotation produces a constant. Uppercase is a convention, not syntax.
* Generic parameters, lifetimes, `where` bounds, struct tags, `global`/`nonlocal`
  statements, `match` bindings, short variable declarations and assignments are
  not declarations.

## 5. Import extraction observations

* All required forms extract: Rust grouped/wildcard/self/super/crate/alias, Go
  single/grouped/renamed/blank/dot, Python single/alias/from/relative/wildcard,
  PHP single/grouped/aliased/function/const.
* Grouped imports expose individual entries while retaining the shared module
  context.
* PHP `use Loggable;` inside a class body is trait composition, not an import.
  The fixture asserts both that it produces a `trait_composition` reference and
  that it produces no import item.
* `ImportHeadKeyword` was removed from the model during implementation. It was
  fully derivable from the written path text already preserved in `module` and
  `item.target`, and one of its variants would have been unreachable. Python
  `relative_levels` remains because the leading dots are not part of the written
  module.

## 6. Call-like extraction observations

* All required forms extract: plain name, qualified path, member/selector,
  static/scoped, generic, indirect, type conversion, explicit construction,
  macro invocation.
* `dynamic_callee` and `nullsafe` mark the cases where the written callee is not
  a static name or where PHP uses `?->`.
* Go generic instantiation is genuinely ambiguous at the syntax level, and
  RepoDex records **every** shape as a call-like occurrence rather than dropping
  the ones the grammar labelled a conversion:

  | written form                         | grammar node                    | `form`            | `type_arguments` |
  |--------------------------------------|---------------------------------|-------------------|------------------|
  | `Generic[int](value)`                | `type_conversion_expression`    | `type_conversion` | `[int]`          |
  | `Generic[int, string](value)`        | `type_conversion_expression`    | `type_conversion` | `[int, string]`  |
  | `pkg.Generic[int](value)`            | `type_conversion_expression`    | `type_conversion` | `[int]`          |
  | `obj.Generic[int](value)`            | `type_conversion_expression`    | `type_conversion` | `[int]`          |
  | `Generic[int](value, other)`         | `call_expression`               | `indirect`        | none             |
  | `Generic[int, string](value, other)` | `call_expression`               | `plain_name`      | `[int, string]`  |
  | `Generic[int]` without a call        | `type_instantiation_expression` | —                 | —                |

  The discriminator is the **call-argument count**, not the number of type
  arguments: one argument becomes a `type_conversion_expression`, two or more
  become a `call_expression`. Because `Generic[int](value)` is simultaneously a
  valid generic invocation and a valid conversion, RepoDex records it and never
  resolves which it is. `form` reports the grammar's own classification and
  `type_arguments_range` carries the exact source span of the written type
  arguments.
* Go `int64(5)` is a `call_expression` and is reported as call-shaped with the
  `plain_name` form. It is never claimed to be a call rather than a conversion.
* PHP `new Thing(...)` is explicit-construction syntax and is recorded with the
  `explicit_construction` form. An ordinary `Thing(...)` in PHP is a call-shaped
  occurrence like any other; it is **not** treated as construction.
* PHP `strlen(...)` is first-class callable creation and produces a reference,
  not a call. `strlen(...)($x)` is an invocation and produces an `indirect` call.
  Both facts coexist for the same source span.
* Python `getattr(obj, "m")()` produces a `plain_name` call for `getattr` and an
  `indirect` call whose written callee is `getattr(obj, "m")`.
* Rust tuple-struct construction is call-shaped; struct literal construction
  produces no call fact.

## 7. Test detection observations

* Rust: only attributes. `#[test]` and `#[tokio::test]` are evidence;
  `#[cfg(test)]` is not, and a `test_` name alone is not.
* Go: `Test`/`Benchmark` prefix rules with the Go "next character is not
  lowercase" rule, plus `single *testing.T` / `*testing.B` signature shape.
  `Testify` is not a candidate; `Test_x` is. `_test.go` is file-level evidence.
  `t.Run` stays an ordinary call and is never proof of test ownership.
* Python: `test_` prefix, any decorator (written form), `TestCase` base clause.
  An aliased `TestCase` import is deliberately not resolved, so `AliasedCase`
  carries no class-level evidence even though its method does carry name
  evidence.
* PHP: `test` prefix and `#[Test]`. The `TestCase` base clause is recorded as a
  reference but is not used as evidence, because that would require resolving the
  imported class.
* No evidence implies that a test runner would collect the candidate.

## 8. Parser recovery observations

```text
Rust   struct Broken { field: , }   MISSING type_identifier, field name kept
       fn incomplete(               MISSING `)`; the following
                                    `fn valid_last() {}` is consumed as a
                                    function-type parameter, so valid_last is
                                    NOT a declaration and nothing is invented
       impl Incomplete              MISSING `;`, impl scope and impl_target kept
Go     func broken( {               MISSING `)`, ERROR node, neighbours kept
       var incomplete =             no written name, no variable fabricated
Python def broken(:                 MISSING `)`, ERROR node, neighbours kept
       class Incomplete(            unterminated, no class declared
PHP    public function broken( {    MISSING `)`, ERROR node, class and
                                    neighbours kept
       function incomplete(         unterminated, no function declared
```

* One recovery diagnostic per recovery region, with identical ranges.
  `tests/recovery.rs` asserts the counts and the ranges match.
* MISSING regions are zero width; ERROR regions are not. Both are asserted.
* Valid fixtures must contain no recovery artifacts at all; the recovery suite
  asserts this for all 22 non-malformed fixtures.
* Recovery *consumes* syntax. RepoDex reports what the tree contains and never
  reconstructs what the damaged source might have meant.

### 8.1 Query match-limit exhaustion

Tree-sitter abandons in-progress captures when a `QueryCursor` exhausts its
capture-list pool and reports it through `did_exceed_match_limit()`. RepoDex sets
the limit explicitly and then checks it:

```text
src/parser/recovery.rs   RecoveryScanner::DEFAULT_MATCH_LIMIT = u32::MAX
                         cursor.set_match_limit(self.match_limit)
                         cursor.did_exceed_match_limit()
                           -> RecoveryError::MatchLimitExceeded { limit }
src/parser/mod.rs        apply_recovery maps that to
                           AnalysisStatus::Incomplete
                           DiagnosticKind::QueryMatchLimitExceeded
                         and maps a query that cannot run to
                           DiagnosticKind::QueryError
src/parser/builder.rs    mark_incomplete() outranks mark_recovered(), in either
                         order, so a partial analysis is never reported as
                         Clean or Recovered
```

`AnalysisStatus::Incomplete` is a distinct status with its own serialized name, and
the scanner counts a match-limit exhaustion as an extraction failure, not as
coverage.

The production recovery query is a pair of single-node patterns
(`(ERROR) @error`, `(MISSING) @missing`). A single-node pattern releases its
capture list as soon as it matches, so it cannot exhaust the pool at **any**
limit — including a limit of one, which `tests/query_limits.rs` asserts. The
truncation check is therefore a guard, not a fix for an observed truncation. The
same suite drives the guard with an injected query whose captures are held across
a deeper match, which does exhaust the pool at a low limit, and asserts that the
analysis then comes back `Incomplete` with the diagnostic rather than `Clean`.

### 8.2 Scanner traversal failures

A directory the walk cannot enter or an entry it cannot stat is recorded, not
discarded:

```text
src/scanner/mod.rs   TraversalFailure { relative_path, kind, message }
                     TraversalFailureKind::Directory | Entry
                     ScanReport::traversal_failures()
                     ScanReport::is_complete()
                     ScanReport::traversal_diagnostics()
```

The walk continues with the rest of the tree, because one unreadable directory
must not stop the scan. But the report says the scan is incomplete: `is_complete()`
is false, the failure appears in the diagnostics as
`DiagnosticKind::TraversalFailure`, the CLI prints `SCAN INCOMPLETE`, the JSON
output carries a `traversal_failures` count, a `traversal_failure_details` array
and `scan_complete: false`, and the process exit code is the analysis-failure
code. The count and the array have different JSON names on purpose: the counters
are flattened into the same object, so a field named the same as a counter would
produce a duplicate JSON key.

`tests/scanner.rs` covers this with a directory made unreadable at runtime. It
first checks that the platform actually denies access to the test process; if it
does not (a container running as root, for example), the test reports that it is
skipping rather than passing vacuously. On this machine the failure path is
genuinely exercised.

## 9. Incremental parsing observations

Command:

```bash
cargo test --locked --test incremental
```

Result: **10 tests, 10 passed.**

Every case compares an incrementally parsed tree against an independent full
parse of the same bytes, on all of:

```text
tree structure digest:
  node kind
  namedness (explicit, never inferred)
  byte range
  row/column point range
  ERROR / MISSING / has-error state
  field name within the parent
  child count and child order
  leaf text (so two leaves of the same kind with different spelling differ)
complete normalized facts, compared structurally (`same_facts`)
complete canonical text (byte-for-byte)
diagnostics
recovery state and status
```

Tree-sitter's internal node ids and symbols are deliberately not compared: they
are ephemeral and carry no meaning across two parses.

Cases covered:

```text
Rust and Go   same-length identifier rename, insertion, deletion, newline
              insertion, UTF-8 multibyte insertion, UTF-8 multibyte
              replacement (Rust), edit near EOF
Rust          introduce a syntax error, then repair it in the same chain
Go            introduce a syntax error, then repair it (damaged step asserted
              Recovered with recovery regions and error diagnostics; repaired
              step asserted Clean with none, and equal to a fresh parse)
Go            four chained edits on a CRLF source: CRLF comment insertion,
              same-length rename, CRLF→LF replacement so line endings are mixed,
              CRLF declaration appended at EOF
Go            five consecutive chained edits
Rust (CRLF)   three chained edits on a CRLF source
Python        four chained edits, including a decorator and multibyte text
PHP           four chained edits, including an inserted declaration
Rust fixture  rename and restore, then compare against the original analysis
```

Every case reported `tree_structure_equal = true`, `facts_equal = true`,
`diagnostics_equal = true` and `recovery_equal = true`. The intermediate damaged
steps really were `Recovered` and the repaired steps were `Clean` again.

Changed ranges are recorded in `IncrementalOutcome::changed_ranges` for
observation only. They are never used to invalidate facts: after an incremental
parse RepoDex re-extracts the whole file, as TASK 1 requires.

### 9.1 What the equivalence claim rests on

`tree_structure_equal` is only meaningful if the digest actually inspects the
properties it claims to. `tests/tree_comparator.rs` pins this directly: it
asserts that the digest contains each compared property for a known source, that
two trees differing only in whitespace, only in an identifier's spelling, or only
in ERROR/MISSING state produce different digests, and that two independent
parsers digest the same bytes identically.

`tests/canonical_completeness.rs` does the same for the fact comparison. It walks
every fact of every committed fixture and mutates every field of every fact in
turn, asserting that the mutation is visible both to `same_facts` and to the
canonical text. The current sweep checks 6002 mutations across 26 fixtures, and
fails if any one of them is invisible. This is what makes the canonical
comparison trustworthy: it is not a hand-maintained shadow of the model, it is
exhaustively verified against the model.

## 10. Benchmark harness results actually measured

Two harnesses were run. Both were built with the release profile.

> **Correction (TASK 1 correction pass).** The first benchmark run
> (`task1-verification`) mislabelled one stage and used an edit that was not
> valid for every language. Its numbers for those stages are **superseded** and
> are not reproduced as current results; section 10.3 lists exactly what changed.
> The numbers below are from `task1-correction-verification`.

### 10.1 Synthetic size matrix

```bash
cargo run --release --bin repodex-bench -- --language all --size all \
    --run-id task1-correction-verification --rustc "$(rustc --version)"
```

Artifacts were written to
`<REPOSUITE_REPODEX_HOME>/benchmarks/task1-correction-verification/{results.txt,results.json}`.
Generated sources are deterministic and are removed when the harness exits.

`results.json` carries one machine-readable row per measurement, each with the
run id, language, size, stage, input bytes, processed bytes, file count,
duration, throughput, declaration and call counts, and the row's note. The
`results.txt` table is a rendering of the same rows.

The stage names describe exactly what their timer covers:

```text
raw parse                     tree-sitter only, warm parser, mean of N parses
normalized extraction         extraction only, tree already parsed
full parse + extraction       parse and extraction, both inside the timer
incremental parse only        parse only; the tree edit was prepared outside the timer
incremental parse + extraction
                              tree edit, incremental parse and whole-file
                              re-extraction, all inside the timer
```

```text
lang    size        raw parse     extraction    full parse+extract   incr.parse   incr.parse+extract   decls    calls
rust    100KiB        11.853 ms     10.640 ms          22.774 ms     0.488 ms           10.732 ms    2080     624
rust    1MiB         126.541 ms    124.147 ms         260.588 ms     6.678 ms          124.623 ms   20780    6234
rust    4MiB         553.860 ms    479.488 ms         952.850 ms    28.839 ms          468.133 ms   82340   24702
rust    over-8MiB   1014.684 ms    904.791 ms        1894.583 ms    53.588 ms          982.190 ms  163020   48906
go      100KiB        14.964 ms     12.921 ms          25.913 ms     0.951 ms           13.858 ms    3025     336
go      1MiB         162.974 ms    125.286 ms         270.472 ms    11.219 ms          133.145 ms   29836    3315
go      4MiB         616.415 ms    509.602 ms         988.256 ms    41.677 ms          532.816 ms  117370   13041
go      over-8MiB   1208.921 ms   1037.302 ms        2081.776 ms    82.459 ms         1078.240 ms  231526   25725
python  100KiB        15.782 ms     12.409 ms          27.024 ms     2.268 ms           12.848 ms    2530     506
python  1MiB         164.222 ms    116.529 ms         265.105 ms    25.142 ms          141.100 ms   25035    5007
python  4MiB         605.849 ms    439.077 ms        1012.253 ms    96.957 ms          539.419 ms   97985   19597
python  over-8MiB   1193.391 ms    936.684 ms        2031.828 ms   199.993 ms         1052.549 ms  194280   38856
php     100KiB        18.624 ms     14.646 ms          32.555 ms     0.326 ms           14.228 ms    1732     433
php     1MiB         201.957 ms    143.529 ms         307.152 ms     4.810 ms          141.895 ms   17352    4338
php     4MiB         723.198 ms    573.549 ms        1317.000 ms    19.283 ms          575.515 ms   68548   17137
php     over-8MiB   1515.444 ms   1257.263 ms        2504.476 ms    39.701 ms         1149.728 ms  136264   34066
```

File discovery, including read, parse and extraction of a single-file directory:

```text
rust    1MiB  250.9 ms (3.99 MiB/s)     4MiB   996.2 ms (4.02 MiB/s)
go      1MiB  288.4 ms (3.47 MiB/s)     4MiB  1107.3 ms (3.61 MiB/s)
python  1MiB  269.3 ms (3.71 MiB/s)     4MiB  1059.9 ms (3.77 MiB/s)
php     1MiB  354.3 ms (2.82 MiB/s)     4MiB  1363.5 ms (2.93 MiB/s)
```

The over-8 MiB row under the default limit is a rejection, not a parse:

```text
rust    over default max size     4.4 ms   rejected, 0 declarations
go      over default max size     3.4 ms   rejected, 0 declarations
python  over default max size     1.2 ms   rejected, 0 declarations
php     over default max size     1.0 ms   rejected, 0 declarations
```

With the limit raised above the file size the same file parses:

```text
rust    raised   1978.5 ms   163020 declarations   48906 calls
go      raised   2206.5 ms   231526 declarations   25725 calls
python  raised   2141.9 ms   194280 declarations   38856 calls
php     raised   2714.6 ms   136264 declarations   34066 calls
```

Nested source is measured at 500 nested containers per language. Its byte count
is small, so its MiB/s numbers are not comparable with the size rows:

```text
rust    500 nested modules     10964 bytes    parse   1.080 ms  extract  1.026 ms  decls 501
go      500 nested closures    11966 bytes    parse   2.007 ms  extract  2.114 ms  decls   2
python  500 nested functions  509445 bytes    parse  15.625 ms  extract  2.190 ms  decls 500
php     500 nested functions   12446 bytes    parse   1.102 ms  extract  1.337 ms  decls 500
```

**The depth was 600 in the first run and is 500 here, because of a real limit.**
`tree-sitter-python` emits a single ERROR node covering the whole file beyond
512 nested indentation levels: at depth 513 it reports `Recovered` with 511
declarations instead of 513. The first run used depth 600 and reported
`decls 511` for Python without recognising it as error recovery, so that row
measured recovery rather than clean nested parsing. 500 is the deepest depth at
which all four grammars parse clean, and the harness now asserts it:

```text
tests::the_nested_benchmark_depth_is_within_every_grammar_limit
tests::every_generated_source_is_syntactically_clean
```

The measured limits, at `tree-sitter 0.27.0` / `tree-sitter-python 0.25.0`:

```text
rust    clean at depth 600 and beyond (tested to 600)
go      clean at depth 600 and beyond (tested to 600)
php     clean at depth 600 and beyond (tested to 600)
python  clean to depth 512, single whole-file ERROR node from depth 513
```

### 10.2 Fixture corpus

```bash
cargo bench
```

Per-fixture parse + extraction, mean of 20 iterations after one warm-up:

```text
language   fixture                          bytes    ms/iter   MiB/s  declarations  calls
rust       rust/boundaries.rs                   1555     0.355     4.18            18      5
rust       rust/calls.rs                         352     0.108     3.10             1     10
rust       rust/declarations.rs                  728     0.171     4.06            21      3
rust       rust/impls.rs                         627     0.120     5.00            10      3
rust       rust/imports.rs                       219     0.040     5.27             0      0
rust       rust/malformed.rs                     589     0.099     5.67             5      0
rust       rust/tests.rs                         245     0.049     4.72             5      0
go         go/boundaries.go                     1607     0.320     4.79             8      9
go         go/calls.go                           307     0.123     2.38             3      9
go         go/declarations.go                    850     0.239     3.39            22      4
go         go/malformed.go                       174     0.032     5.24             4      0
go         go/sample_test.go                     489     0.146     3.20             8      2
python     python/boundaries.py                  771     0.144     5.11             8      3
python     python/calls.py                       315     0.102     2.94             1     11
python     python/declarations.py                836     0.263     3.04            16      4
python     python/malformed.py                   181     0.045     3.86             3      0
python     python/test_sample.py                 535     0.114     4.48            11      1
php        php/boundaries.php                    571     0.136     4.00             9      1
php        php/calls.php                         716     0.277     2.47             5     11
php        php/declarations.php                 1195     0.352     3.24            22      1
php        php/malformed.php                     191     0.066     2.75             5      0
php        php/tests.php                         404     0.080     4.82             7      0

fixture tree scan: 52 files visited, 26 supported, 22 clean, 4 recovered,
                   17479 bytes processed in 5.973 ms (2.79 MiB/s)
canonical scan digest: fnv1a64:f0834b9a45607745
```

`go/boundaries.go` is larger and reports more calls than in the first run because
the correction pass added the `Generic[int, string](value)`,
`pkg.Generic[int](value)` and `obj.Generic[int](value)` probes to it. The digest
changed for the same reason plus the canonical-format correction.

These fixtures are tiny, so per-file and per-call overhead dominates the MiB/s
figures. They are reported because they were measured, not because they are
representative throughput numbers.

### 10.3 Benchmark numbers superseded from the first report

The first run's numbers for the affected stages must not be used. What changed:

```text
superseded                                     replaced by
---------------------------------------------- -----------------------------------
"incremental parse + extraction"               "incremental parse + extraction",
  the timer started AFTER the incremental        the timer starts before the tree
  parse had already finished, so the row         edit and ends after extraction
  measured whole-file extraction only

"parse + extraction"                           "full parse + extraction",
  (the same measurement, clearer label)          the same measurement

"incremental parse"                            "incremental parse only",
  (the same measurement, clearer label; the      the same measurement
  tree edit is still outside the timer)

every derived throughput, because the first    recomputed from the corrected
  report's incr.parse column was the parse       stages
  and its "incremental parse + extraction"
  row was extraction only

nested depth 600                               nested depth 500,
  Python at 600 was error recovery, not          all four grammars clean
  clean parsing: 511 of 600 declarations

fixture go/boundaries.go row                   go/boundaries.go grew
  (1057 bytes, 5 calls)                          (1607 bytes, 9 calls)

fixture canonical scan digest                  fnv1a64:f0834b9a45607745
  fnv1a64:586836712348bab0

results.json                                   results.json now carries one
  contained only metadata and a pointer          machine-readable row per
  to results.txt                                 measurement (164 rows)
```

The first run's raw-parse, extraction and discovery figures are not superseded as
measurements. They are reproduced here from the new run because that is the run
the artifacts on disk describe, and they differ from the first report by machine
noise only, except where the `nested` depth changed.

### 10.4 A real defect found by the harness

The first run of the synthetic matrix showed extraction degrading
super-linearly while raw parsing stayed linear. Direct measurement confirmed it:

```text
before the fix
  100 KiB   parse  17.6 ms (5.55 MiB/s)   extract   80.9 ms ( 1.21 MiB/s)
    1 MiB   parse 183.5 ms (5.45 MiB/s)   extract 6697.6 ms ( 0.15 MiB/s)
    4 MiB   parse 721.7 ms (5.54 MiB/s)   extract  103.2 s   ( 0.04 MiB/s)
    8 MiB   parse 1417.5 ms (5.65 MiB/s)  extract  413.9 s   ( 0.02 MiB/s)

after the fix
  100 KiB   parse  17.6 ms (5.54 MiB/s)   extract   16.0 ms ( 6.10 MiB/s)
    1 MiB   parse 184.1 ms (5.43 MiB/s)   extract  167.9 ms ( 5.95 MiB/s)
    4 MiB   parse 723.7 ms (5.53 MiB/s)   extract  640.9 ms ( 6.24 MiB/s)
    8 MiB   parse 1376.4 ms (5.82 MiB/s)  extract 1347.8 ms ( 5.94 MiB/s)
```

Root cause: `range_from_offsets` computed row/column by scanning the source from
byte 0 for every derived range. Derived ranges occur once per declaration header
and once per PHP callee span, so extraction was quadratic in file size.

Fix: `model::range::LineIndex`, a per-file line-start table with binary-search
lookup, built once per file in `FactBuilder`. `tests/ranges.rs` pins `LineIndex`
against an independent reference implementation for many source shapes, and
`LineIndex::point_at` clamps out-of-range offsets instead of panicking.

This is the clearest evidence that the benchmark harness earns its place: the
defect was invisible on the fixtures and fatal on 4 MiB files.

### 10.5 Peak memory

`NOT MEASURED YET`.

RepoDex reports no internal RSS metric, because a reliable one needs an external
platform tool. To measure it, use one of:

```bash
/usr/bin/time -v ./target/release/repodex-bench --language rust --size 4MiB
valgrind --tool=massif ./target/release/reposuite-repodex scan <repository>
```

No memory number is claimed in this document.

### 10.6 TASK 1 correction pass

An independent static review of the TASK 1 snapshot raised findings A-K. Every one
was independently reproduced or refuted against the source before anything was
changed. What was found and what was done:

```text
finding                                    verdict      correction
------------------------------------------ ------------ -------------------------------
A  canonical comparison omitted            CONFIRMED    complete canonical rendering;
   normalized fields (row/column                        structural `same_facts`;
   coordinates, import item ranges,                     exhaustive mutation sweep
   type-argument range, test-evidence                   (tests/canonical_completeness.rs)
   range)

B  incremental tree comparator             PARTIALLY    digest now records kind,
   omitted points, namedness and            CONFIRMED    namedness, bytes, points,
   field names; the documentation                       ERROR/MISSING state, field
   claimed they were compared                           names, child count and order,
                                                        and leaf text
                                                        (tests/tree_comparator.rs)

C  Go CRLF and Go error->repair            CONFIRMED    two new Go cases with the
   incremental cases existed only                       damaged and repaired states
   for Rust                                             asserted explicitly

D  Go `Generic[int](value)` was            CONFIRMED    recorded as a call-like
   dropped because the grammar called                   occurrence with the
   it a conversion                                      `type_conversion` form and its
                                                        exact type-argument range;
                                                        never resolved either way

E  scanner discarded walk errors           CONFIRMED    typed traversal failures in the
   (`Err(_) => continue`)                               report, diagnostics, CLI text,
                                                        JSON and exit code

F  no check for Tree-sitter query          CONFIRMED    explicit match limit, checked;
   match-limit exhaustion                               `AnalysisStatus::Incomplete`
                                                        outranks `Recovered`

G  "incremental parse + extraction"        CONFIRMED    timer now starts before the
   timer started after the parse                        tree edit and ends after
                                                        extraction

H  the synthetic incremental edit was      CONFIRMED    language-correct comment, and
   `// incremental tail` for every                      the harness asserts the edited
   language, including Python                           source parses clean

J  results.json held metadata and a        CONFIRMED    one machine-readable row per
   pointer instead of measurements                      measurement

K  the report said PHP `Thing(...)`        CONFIRMED    corrected: only
   was explicit construction, and                       `new Thing(...)` is
   that no warning was silenced                         explicit construction; the
                                                        report now names the one
                                                        lint allowance
```

Two further findings were made by the new tests rather than by the review:

```text
tree digest recorded the grammar symbol    FIXED   leaf text is now recorded, so
name but not the leaf text, so `fn f()             two leaves of the same kind
{}` and `fn g() {}` digested identically            with different spelling differ

the nested benchmark at depth 600 was      FIXED   depth is now 500, the deepest
error recovery for Python (511 of 600              depth all four grammars parse
declarations, one whole-file ERROR node)           clean, and the harness asserts it
```

The one lint allowance in the codebase, stated precisely:

```text
src/bin/repodex-bench.rs   #[allow(clippy::too_many_arguments)]
                           on `row(...)` and on `measure(...)`

reason  both functions take the benchmark row fields positionally. `row` builds
        one `Row` from its columns and `measure` needs the analyzer, the parser
        registry, the language, the size name, the file name, the path and the
        source. Bundling them into a struct would add a type whose only purpose
        is to satisfy the lint.
```

Every other Clippy warning is denied: `cargo clippy --locked --all-targets
--all-features -- -D warnings` passes with no other allowance.

Nothing in the correction pass changed the architecture. There is still no
semantic resolution, no cross-file resolution, no repository map, no persistent
index and no LSP dependency.

## 11. NOT MEASURED YET

```text
large third-party repository corpus            not measured
150k LOC cross-language corpus                 not measured
extraction precision/recall audit              not measured
comparison against the previous RepoDex        not measured
Rust-vs-Go performance verdict                 deliberately not produced
peak RSS                                       see 10.5
cold-cache filesystem performance              not measured
multi-threaded or parallel scanning            not implemented
traversal failures under a real permission
  model on a non-POSIX platform                not measured; the reporting path
                                               is covered by a POSIX test that
                                               skips when the platform cannot
                                               deny access
query match-limit exhaustion in production     not reachable; the production
                                               recovery query cannot exhaust the
                                               capture pool at any limit, so the
                                               guard is exercised with an
                                               injected query instead
incremental parsing on large real files        not measured; the harness uses
                                               synthetic sources and fixtures
changed-ranges accuracy as a fact
  invalidation signal                          not evaluated; TASK 1 does not
                                               use changed ranges for that
Go conversions vs calls ground truth           not audited against a compiler
PHP trait adaptation semantics                 not audited against a runtime
```

RepoDex is **not** claimed to be faster than the previous implementation. No
such comparison was performed.

## 12. Known limitations of the measurements

* All throughput numbers are single-run observations on one machine, not
  statistical benchmarks. Only `raw parse` and the `cargo bench` rows are means
  over repeated iterations.
* The `incremental parse only` and `incremental parse + extraction` rows measure
  one specific edit shape: an append at end of file, which Tree-sitter can
  satisfy by reusing almost the whole old tree. Neither is representative of
  edits in the middle of a file. The equivalence suite covers other edit shapes
  for correctness, but their timings were not recorded. The difference between
  the two rows is the tree edit and the whole-file re-extraction, which is why
  the combined row is much slower than the parse-only row: TASK 1 re-extracts
  the entire file after any incremental parse.
* The two incremental rows are not comparable to each other as a "speedup"
  figure. The combined row's dominant cost is extraction, not parsing.
* `over default max size (raised)` measures a first parse with a cold parser, so
  it includes grammar load. The `raw parse` row excludes it.
* The nested-source rows have very different byte counts per language, so their
  MiB/s figures must not be compared across languages. Their depth is 500
  because `tree-sitter-python` fails beyond 512 nested indentation levels; see
  10.1. The harness asserts that every generated source, at every size, parses
  clean, so a measurement can never silently become a recovery measurement.
* `python` at depth 513 and beyond is a measured grammar limitation, not a
  RepoDex limitation: the grammar produces one whole-file ERROR node, and RepoDex
  reports what the tree contains. RepoDex never reconstructs the missing
  structure.
* Synthetic sources are regular and highly repetitive, so they are likely to be
  friendlier than real code to both the parser and the extraction pass.
* Every generated source, at every size, is asserted to parse clean before it is
  measured, and the incremental edit is asserted to be valid for the language.
  A stage can therefore not silently become an error-recovery measurement, which
  is exactly what happened in the first run at nested depth 600 for Python.
