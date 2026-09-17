# RepoDex Spike Results

Observed results only. Nothing in this document is extrapolated, and nothing
compares RepoDex against another implementation.

```text
TASK 1 status:                  FOUNDATION_SPIKE_COMPLETE
TASK 2 validation status:       VALIDATION_COMPLETE
TASK 2 architecture verdict:    CONDITIONAL_GO
TASK 1 commit validated:        4a6e8e3fe88be068932279b3bf896c731c812859
```

**TASK 2 validates the TASK 1 foundation.** Sections 1–12 below are the TASK 1
baseline results. The TASK 2 validation campaign — corpus manifest, per-language
parsing/recovery, extraction audit, range validation, determinism, incremental
correctness, performance, memory and retention, open findings, decision and
TASK 3 readiness — is in **Part II** at the end of this document. The findings
log with stable IDs is `docs/TASK2_FINDINGS.md`; the itemized TASK 2 final report
is `docs/TASK2_FINAL_REPORT.md`; the pre-registered plan is
`docs/TASK2_VALIDATION_PLAN.md`.

TASK 2 changed one production file (`src/parser/php.rs`, finding F003). The
boundary is unchanged:

```text
source bytes
→ Tree-sitter
→ language adapter
→ normalized unresolved syntax facts
```

This document describes the **corrected** TASK 1 snapshot. Two independent
static reviews were run against it.

The first raised eleven findings; all eleven were reproduced and corrected, and
two further defects were found by the new tests. Section 10.6 records each
finding, its verdict and its correction. Section 10.3 lists exactly which
benchmark numbers from the first report are superseded.

The second concluded `TASK_2_SHOULD_WAIT` and raised one HIGH portability issue
plus five claim-accuracy items; all six were reproduced and corrected. Section
10.7 records them, including the Windows portability gate and the two test names
that claimed more than they asserted.

Both previous commits are preserved unamended for audit:

```text
8f3314bc9cb2768efab32f0cdc9de4c93332eb82   original TASK 1 snapshot
718891d1344b9fa0aec541fa4027160f9658fd64   first correction pass
```

Machine and toolchain for every measurement below:

```text
platform:  Linux (WSL2), x86_64
rustc:     1.93.1 (01f6ddf75 2026-02-11)
cargo:     1.93.1 (083ac5135 2025-12-15)
profile:   release (bench and repodex-bench), debug for tests
```

Portability note: the test targets are also type-checked for
`x86_64-pc-windows-gnu` (see 10.7). No measurement in this document was taken on
native Windows, and none is claimed to have been.

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

Result: **155 tests, 155 passed, 0 failed.**

```text
src/lib.rs (unit tests)    1 passed   (scanner counter classification)
src/bin/repodex-bench.rs   7 passed   (unit tests: generated sources, the
                                       language-correct incremental edit, the
                                       nested depth limit, stage boundaries,
                                       JSON rows)
tests/canonical_completeness.rs  3 passed
tests/cli.rs              15 passed
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
tests/scanner.rs          12 passed
tests/shared_fixtures.rs  12 passed
tests/tree_comparator.rs   6 passed
```

The Unix-only half of the traversal contract (`tests/scanner.rs` and
`tests/cli.rs`, one test each) is gated with `#[cfg(unix)]` because provoking a
traversal failure needs a directory the process cannot read. The
platform-independent half — that a scan which visits everything reports itself
complete — runs everywhere, so the portability gate does not remove all coverage
of those accessors on non-Unix platforms.

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
* PHP has three distinct forms, and RepoDex keeps them apart:

  ```php
  Thing($arg);       // invocation syntax      -> plain_name call-like
  new Thing($arg);   // explicit construction  -> explicit_construction call-like
  Thing(...);        // first-class callable   -> first_class_callable reference
  ```

  Literal `Thing(...)` is neither an invocation nor construction: it produces a
  `first_class_callable` reference and no call-like occurrence.
* PHP `strlen(...)` is first-class callable creation and produces a reference,
  not a call. `strlen(...)($x)` is an invocation and produces an `indirect` call
  with `dynamic_callee = true`. Both facts coexist for the same source span.
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

The complete lint inventory, stated precisely. An earlier version of this
report said "no warning was silenced", which was wrong: it omitted
`FactBuilder::push_call`, and it described only Clippy allowances while two
`dead_code` allowances also existed.

```text
src/parser/builder.rs:267        #[allow(clippy::too_many_arguments)]
                                 on `FactBuilder::push_call(...)`
src/bin/repodex-bench.rs:257     #[allow(clippy::too_many_arguments)]
                                 on `measure(...)`
src/bin/repodex-bench.rs:284     #[allow(clippy::too_many_arguments)]
                                 on `row(...)`
tests/support/mod.rs:7           #![allow(dead_code)]
                                 module-level, shared test helpers
```

```text
push_call  takes the eight fields of one CallLikeOccurrence positionally. They
           are one fact rather than eight independent parameters, and every
           language adapter calls it.
measure    takes the analyzer, the parser registry, the language, the size name,
           the file name, the path and the source.
row        takes the benchmark row fields positionally.
reason     bundling either into a struct would add a type whose only purpose is
           to satisfy the lint, and would separate the fields from the call
           sites that read as a table.

tests/support/mod.rs  the shared helper module is compiled into every
                      integration test binary and each binary uses a subset of
                      the helpers, so an unused helper is expected rather than
                      suspicious.
```

One allowance that this pass removed rather than documented: `src/cli.rs` had

```text
#[allow(dead_code)] fn scan_start_diagnostic(root, message) -> Diagnostic
```

which nothing called — the CLI reports a scan failure with
`.map_err(|error| format!("scan failed: {error}"))?`, and the helper's message
text did not match that path. It was dead code kept alive by the allowance, so
the function was deleted and the allowance with it, along with the two imports
that then became unused. That is a deletion of dead code, not the removal of a
justified allowance.

Every other Clippy warning is denied: `cargo clippy --locked --all-targets
--all-features -- -D warnings` passes with these allowances and no others, on
Linux and on the `x86_64-pc-windows-gnu` target.


### 10.7 TASK 1 portability and claim-accuracy pass

A second independent review concluded `TASK_2_SHOULD_WAIT` and raised one HIGH
portability issue plus five smaller claim-accuracy items. All were reproduced
against the source. The correction commit is
`718891d1344b9fa0aec541fa4027160f9658fd64`.

```text
finding                                    verdict      correction
------------------------------------------ ------------ -------------------------------
H1 Unix-only test helpers were not         CONFIRMED    helpers and the tests that
   gated, so a native Windows build                     use them moved into
   would fail while compiling the test                  `#[cfg(unix)]` modules; the
   targets before any runtime skip                      platform-independent half of
   could run                                            the traversal contract was
                                                        split out into tests that
                                                        run everywhere
                                                        (see the portability note
                                                        below)

2  documentation still said               CONFIRMED    docs/LANGUAGE_SPIKE.md now
   `Generic[int](value)` is not                         states that the form is
   call-like                                            preserved as a call-like
                                                        occurrence and never
                                                        resolved

3  the lint inventory was incomplete      CONFIRMED    complete inventory in 10.6
   (it omitted `FactBuilder::push_call`                 and in AGENTS.md; one
   and both `dead_code` allowances)                     allowance on dead code was
                                                        removed with the dead code

4  PHP examples were ambiguous            CONFIRMED    the three PHP forms are now
   (`Thing(...)` described as                           given as concrete, distinct
   invocation or construction)                          examples in ARCHITECTURE.md,
                                                        LANGUAGE_SPIKE.md and 6

5  two test names claimed more than       CONFIRMED    one test moved to the code
   they asserted:                                       that owns the classification
   `an_incomplete_analysis_is_counted_                  and genuinely drives it;
   as_an_extraction_failure` never                      the other was rewritten and
   produced an `Incomplete` analysis;                   renamed to assert the
   `a_query_error_is_not_reported_as_                   reachable distinction
   a_match_limit_exhaustion` never                      (see below)
   produced a query error

6  `combined_ms > parse_ms` does not      CONFIRMED    the timing inequality was
   prove the timer covers the parse                     removed; the timed unit is
                                                        now a named function whose
                                                        two outputs prove what the
                                                        interval contains
```

#### Windows portability

The two traversal-failure tests make a directory unreadable with
`std::os::unix::fs::PermissionsExt`. That import is unconditional in the test
crate, so on a native Windows host the test target would fail to *compile*,
before any runtime skip could run — under `cargo test` and under
`cargo clippy --all-targets`.

Both helpers and both tests now live inside `#[cfg(unix)] mod
unreadable_directory`, and no Windows ACL manipulation was added. No other
Unix-only API exists in the repository: a sweep for `std::os::unix`,
`PermissionsExt`, `from_mode`, `OsStrExt`, `MetadataExt`, `FileTypeExt`,
`CommandExt`, `ExitStatusExt` and `libc::` finds only those two modules and the
pre-existing `#[cfg(unix)]` symlink test in `tests/scanner.rs`.

Because the failure-injection half of the traversal contract is Unix-only, the
platform-independent half was split out so non-Unix platforms still exercise the
accessors:

```text
tests/scanner.rs  a_scan_that_visits_everything_reports_itself_complete
tests/cli.rs      a_complete_scan_reports_itself_complete_in_json
```

Writing the gate also removed a second, subtler Windows failure that a reviewer
would have hit next: `repodex::model::DiagnosticKind` and
`repodex::scanner::TraversalFailureKind` were imported at file scope in
`tests/scanner.rs` but used only by the gated test, so they became unused
imports on non-Unix — which fails `-D warnings` there. Both imports moved into
the gated module.

#### Regression-test claim accuracy

```text
an_incomplete_analysis_is_counted_as_an_extraction_failure
  claimed   an Incomplete analysis is counted as an extraction failure
  actually  asserted extraction_failures == 0 on a *Recovered* file, and never
            produced an Incomplete analysis at all
  now       the claim is tested where the classification lives, by a unit test in
            src/scanner/mod.rs that calls `accumulate` with a genuinely
            Incomplete analysis and asserts extraction_failures == 1 and
            FileOutcome::Failed. The end-to-end test is renamed
            `a_recovered_file_is_not_counted_as_an_extraction_failure` and keeps
            the control it always was.
  why not end to end
            `Scanner::scan` uses the default match limit of u32::MAX, so no file
            can reach Incomplete through the scanner without a production hook.
            The counter's owner is `accumulate`, so the counter is tested there.

a_query_error_is_not_reported_as_a_match_limit_exhaustion
  claimed   a genuine query error maps to QueryError, not to the match-limit
            diagnostic
  actually  used the production scanner, which succeeds, so no query error was
            ever produced; both assertions were negative assertions about a
            diagnostic that cannot occur
  now       renamed `a_match_limit_exhaustion_is_reported_only_as_a_match_limit_exhaustion`
            and rewritten to assert a real contrast: the same tree does not
            report a truncation through the production scanner (the control
            asserts `not Incomplete`, not `Recovered`) and is `Incomplete` with
            exactly one recovery-failure diagnostic through a scanner whose
            limit is low enough to abandon captures. `Recovered` on the
            production scanner is pinned separately by
            `the_production_recovery_query_cannot_exhaust_the_pool`.
  root cause
            `RecoveryError::Query` is not produced by any code path. A query that
            fails to compile is rejected by `with_query_source` while an adapter
            is being built, and Tree-sitter's `QueryCursor::matches` iteration
            has no failure signal. The variant is now documented as unproduced
            rather than implied to be exercised, and no test claims to reach it.
```

#### Benchmark interval claim

The old test asserted `combined_ms > parse_ms`. That is a timing inequality: it
can hold for reasons unrelated to where the timer starts, and it would not
demonstrate the claim even when it held. It has been removed, and no
timing-based assertion replaced it.

What the test now proves is deterministic and structural. The timed work was
extracted into a named function:

```text
src/bin/repodex-bench.rs   fn incremental_parse_and_extract(...) -> (Tree, FileAnalysis)
```

`measure` opens the timer immediately before that single call and closes it
immediately after, so anything the function returns was produced inside the
interval. The test calls the function directly and asserts that it returns both
a tree and an analysis, that the tree matches a fresh parse of the edited bytes
and differs from the base tree, and that the analysis matches a fresh extraction.
Together with the row-level assertions (parse-only reports no facts, combined
reports the edited source's facts and bytes), that is what the test can prove.

What is established by code structure rather than by a test is the *start point*
of the interval: the timer opens before the call. No test asserts that, and none
claims to.

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
  model on a non-POSIX platform                not measured; the failure path is
                                               Unix-only and gated with
                                               `#[cfg(unix)]`, so only the
                                               completeness half of the
                                               contract runs on other platforms
native Windows execution of the test suite     not executed; the test targets are
                                               type-checked for
                                               `x86_64-pc-windows-gnu`, which is
                                               a cross-target check, not a
                                               native Windows run
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

---

# Part II — TASK 2 Validation

## 13. Executive summary

TASK 2 validated the TASK 1 foundation commit
`4a6e8e3fe88be068932279b3bf896c731c812859` against a pinned, manifest-backed
corpus of 2.42M LOC across four languages, and against a synthetic performance
matrix.

The foundation holds. Across 19,165 visited files there were **zero parser
failures, zero extraction failures and zero read failures**; among the 11,054
files that parsed, the recovery rate was **0.163%** (18 files), and every
recovery was a known grammar limitation or an intentional malformed fixture.
Determinism, incremental equivalence and source-range exactness all held on real
repositories.

One genuine defect was found and fixed: **F003**, a systematic PHP
anonymous-class callee-range corruption affecting 569 facts in 182 files. It was
fixed, regression-tested, and re-verified to zero across the corpus.

Two Rust grammar boundaries were quantified and documented (**F001**, **F002**);
neither is an adapter defect and neither is fixable inside the syntax-only
boundary.

The verdict is **CONDITIONAL_GO** (§35). No unresolved BLOCKER or HIGH finding
remains.

## 14. TASK 1 commit validated

```text
4a6e8e3fe88be068932279b3bf896c731c812859
Gate Unix-only tests and correct overstated TASK 1 claims
```

The mandatory TASK 1 verification was rerun on this exact commit before any
TASK 2 benchmark or audit: all five gates passed (155 tests at that point), and
every TASK 1 capability was present. Only after that were the two non-blocking
documentation cleanups applied (see `docs/TASK2_FINAL_REPORT.md`).

## 15. Validation status

```text
VALIDATION_COMPLETE
```

Every mandatory evidence class required by `task2.md` was produced and recorded.
What was *not* measured is listed explicitly in the TASK 2 final report; nothing in this document
infers a result that was not observed.

## 16. Architecture recommendation

```text
CONDITIONAL_GO
```

See §35 for the exact evidence behind this verdict.

## 17. Environment

From `environment.json` in the run directory:

```text
platform:        Linux (WSL2), x86_64 — not native Windows
distro:          Ubuntu 24.04.1 LTS
kernel:          6.18.33.2-microsoft-standard-WSL2
cpu:             Intel(R) Core(TM) i9-9900K @ 3.60GHz, 8 logical CPUs
ram:             15,603 MiB
rustc:           1.93.1 (01f6ddf75 2026-02-11)
cargo:           1.93.1 (083ac5135 2025-12-15)
tree-sitter:     0.27.0
tree-sitter-rust: 0.24.2   tree-sitter-go: 0.25.0
tree-sitter-python: 0.25.0 tree-sitter-php: 0.24.2
build profile:   release for benchmarks/scans, debug for tests
scanner threads: 1 (single-threaded scans)
file-size limit: 8,388,608 bytes (8 MiB), overridable
ignore policy:   root-scoped .gitignore only (optional); global git ignore,
                 global excludes, parent-directory ignores outside the root and
                 developer-specific git config are NOT consulted
```

Ignore policy was verified empirically: files matched by a global gitignore
outside the root were still visited, and only a root `.gitignore` (with
`--no-gitignore` off) affected the scan.

## 18. Corpus manifest

Committed manifest: `benchmarks/corpora.json`
(`schema: reposuite-repodex/task2-corpora/1`). It pins repository URL, revision,
license, source scope, exclusions and observed size per corpus. Local checkout
paths are supplied externally through environment variables and are **not**
committed.

Public corpora (pinned shallow clones):

```text
rust    BurntSushi/ripgrep    clap-rs/clap       serde-rs/serde     tokio-rs/tokio
go      gin-gonic/gin         spf13/cobra        gohugoio/hugo
python  pallets/flask         psf/requests       django/django
php     laravel/framework     composer/composer  symfony/console
```

User-owned corpora (revisions pinned, paths external):

```text
local:repodex             go     old Go RepoDex implementation
local:gpt-tunnel-gateway  go     user-owned Go service
local:vps_desk            python user-owned Python tool
local:reposuite-repodex   rust   RepoDex self-scan
```

Per-language observed volume, all above the 150k LOC requirement:

```text
language  files  bytes       LOC
rust      1,510  12,052,157   381,770
go        2,094  12,630,410   421,647
python    4,110  31,159,438   866,844
php       4,095  24,699,053   752,336
total    11,809  80,541,058  2,422,597
```

Baseline scan aggregate (single-threaded, one pass over every corpus):

```text
language  visited  supported  clean  recovered  unsupported
go          4,047      2,140   2,140          0        1,907
php         5,047      4,053   4,052          1          994
python      7,874      3,341   3,341          0        4,533
rust        2,197      1,520   1,503         17          677
total      19,165     11,054  11,036         18        8,111
```

```text
parser failures:    0
extraction failures: 0
read failures:      0
recovery among parsed files: 18 / 11,054 = 0.163%
processing coverage:        11,054 / 19,165 = 57.7%
```

Facts produced across the corpus:

```text
declarations: 180,109   imports: 41,508   references: 38,794
calls:        696,545   test candidates: 44,754
processed bytes: ~68 MiB
```

The 18 recoveries are 17 Rust files (13 from F001 in clap `tests/**`, plus 4
intentional `malformed.rs` fixture runs) and 1 PHP file. No recovery is
unexplained.

## 19. Extraction audit methodology

Audit regions were **frozen before any RepoDex output was inspected**. Two files
per language were selected by a rule that uses only line counts and path names,
never RepoDex predictions:

```text
per language: the largest corpus by supported files; the sorted supported-file
list; drop paths under config|locale|migrations|fixtures|testdata|vendor|
node_modules|third_party|generated; then take the first two files at or above a
line-count threshold.
```

The frozen selection is recorded in `audit/frozen-regions.json`. Two checks were
run per region:

```text
precision  every fact RepoDex emits must have a range whose bytes exist in the
           source and whose recorded (row, column) is the true position
recall     occurrences enumerated independently by regex/text search must not
           include anything RepoDex failed to predict (a lower bound on recall)
```

For each region the audit recorded the number of predicted declarations,
imports and calls, the number of range mismatches, and the number of
independently-found occurrences that RepoDex did **not** predict
(`not_predicted`). Results are in `audit/extraction-audit.json`.

## 20. Extraction audit results

```text
language  file                        status  decl(pred/mismatch/indep)  import  call(pred/mismatch)  test  not_predicted
rust      tokio benches/copy.rs       clean   31 / 0 / 25                7 / 7   81 / 0             0/0   0
rust      tokio rt_multi_threaded.rs  clean   19 / 0 / 19                7 / 7  126 / 0             0/0   0
go        gpt-tunnel main.go          clean   26 / 0 / 20                1 / 1  113 / 0             0/0   0
go        gpt-tunnel ..._test.go     clean   18 / 0 /  2                1 / 1  102 / 0             1/0   0
python    django apps/config.py       clean   13 / 0 / 11                7 / 7   55 / 0             0/0   0
python    django __init__.py          clean   36 / 0 / 27               11 /11   89 / 0             0/0   0
php       laravel Gate.php            clean   54 / 0 / 43               14 /15  167 / 0             0/0   0
php       laravel AuthManager.php     clean   25 / 0 / 19                7 / 8   45 / 0             0/0   0
```

`not_predicted` is empty in every region: the independent enumeration found
nothing RepoDex missed, so there is no observed false negative in the audit
sample. `range_mismatch` is 0 for every declaration and every call.

`indep_found < predicted` for declarations is expected and is not a recall
failure: the independent check is a deliberately conservative lower bound (it
recognises fewer declaration forms than the adapter). What matters is that it
never found something the adapter missed.

Two apparent discrepancies were investigated and both were artifacts of the
independent check, not adapter defects:

* PHP `Gate.php` — the independent check counted 15 `use` lines, RepoDex 14.
  The 15th was a trait composition inside the class body
  (`use HandlesAuthorization;`), which is not a namespace import. RepoDex was
  correct. Recorded as **F004**.
* Go — a helper `_test.go` file with no `func Test…` was flagged by a
  filename-only check. RepoDex emitted a filename-convention test candidate,
  which is the documented behaviour. Recorded as **F005**.

A manual call-recall check was also done on a hand-read window of each language:
in the Rust window, hand enumeration found 14 calls where RepoDex reported 13.
The missing call was nested inside a macro argument and is the **F002** grammar
boundary, not an oversight.

## 21. Rust

**Parsing / recovery.** 2,197 visited, 1,520 supported, 1,503 clean, 17
recovered, 0 parser failures. All 17 recoveries are explained: 13 are **F001**
(a primitive-type name used as a macro name, in clap `tests/**`) and 4 are the
intentional `malformed.rs` fixture runs. Recovery rate 1.1% of supported files,
entirely from one known grammar limitation.

**Extraction audit.** `tokio benches/copy.rs` and `rt_multi_threaded.rs`: 0 range
mismatches, 0 `not_predicted`. 19,309 macro-invocation calls were recorded
corpus-wide; 12,199 macro invocations had argument text that looked like a call
— an upper bound on the **F002** boundary.

**Ranges.** 80 files / 10,733 ranges / 0 bad.

**Determinism.** Tokio was not in the three-run set; determinism was confirmed on
django, laravel and hugo (§29) and across checkout roots for flask and cobra.
Tokio's digest was stable across the 5 end-to-end runs used for timing.

**Performance.** See §26 and §27.

**Incremental.** Covered by the shared real-file incremental test (§30) and the
synthetic matrix (§26).

**Memory.** Tokio/Rust end-to-end peak RSS 30,132 KiB (29.4 MiB); retention A/B/C
in §33.

**Limitations.** **F001** and **F002**.

## 22. Go

**Parsing / recovery.** 4,047 visited, 2,140 supported, 2,140 clean, 0
recovered, 0 parser failures. Go is the only language with a zero recovery rate
on the corpus.

**Extraction audit.** `gpt-tunnel main.go` and a `_test.go` file: 0 range
mismatches, 0 `not_predicted`. The Go test candidate in the `_test.go` file is
filename-convention evidence (**F005**).

**Ranges.** 80 files / 15,993 ranges / 0 bad; 4 files contain multibyte content.

**Determinism.** Hugo digest identical across 3 runs (§29); cobra identical
across two different checkout roots.

**Performance.** Hugo (Go, 912 files, 230,409 LOC): median 2,266.8 ms,
402.3 files/s, 2.62 MiB/s, 101.6 kLOC/s.

**Memory.** Hugo end-to-end peak RSS 32,480 KiB (31.7 MiB); retention A/B/C in
§33.

**Limitations.** None observed in the audit sample.

## 23. Python

**Parsing / recovery.** 7,874 visited, 3,341 supported, 3,341 clean, 0
recovered, 0 parser failures.

**Extraction audit.** `django apps/config.py` and `__init__.py`: 0 range
mismatches, 0 `not_predicted`.

**Ranges.** 80 files / 2,326 ranges / 0 bad; 7 files contain multibyte content —
the most of any language in the sample.

**Determinism.** Django: 2,932 files, 297,050 facts, digest
`fnv1a64:0b54ce97af0cf2fb`, identical across 3 full canonical runs (§29).

**Performance.** Django (2,932 files, 526,612 LOC): median 6,098.7 ms,
480.8 files/s, 3.03 MiB/s, 86.3 kLOC/s.

**Memory.** Django end-to-end peak RSS 104,876 KiB (102.4 MiB); retention A/B/C
in §33.

**Limitations.** None observed. The 4,533 unsupported Python files are mostly
non-`.py` content in the checkouts (templates, data, vendored trees), not
parser failures.

## 24. PHP

**Parsing / recovery.** 5,047 visited, 4,053 supported, 4,052 clean, 1
recovered, 0 parser failures. The single recovery is a malformed file.

**Extraction audit.** `laravel Gate.php` and `AuthManager.php`: 0 range
mismatches, 0 `not_predicted`. The `Gate.php` import count difference is **F004**
(a trait composition, not an import).

**Ranges.** 80 files / 7,006 ranges / 0 bad; 1 file contains multibyte content.

**Determinism.** Laravel: 3,086 files, digest `fnv1a64:205500ee5ee155a7`,
identical across 3 full canonical runs (§29).

**Performance.** Laravel (3,086 files, 562,801 LOC): median 5,021.3 ms,
614.6 files/s, 3.38 MiB/s, 112.1 kLOC/s — the fastest language by kLOC/s.

**Memory.** Laravel end-to-end peak RSS 123,920 KiB (121.0 MiB) — the highest of
the four, consistent with the largest per-file ASTs; retention A/B/C in §33.

**Limitations fixed / found.** **F003**, the anonymous-class callee-range bug,
was found in this language and fixed.

### F003 — the PHP anonymous-class range fix

Before the fix, every PHP anonymous-class construction used
`callee_written = "class"` with a `callee_range` covering the whole
`anonymous_class` node (e.g. `146..900` for a five-byte name). Corpus-wide:

```text
                      pre-fix   post-fix
PHP files scanned      4,095     4,095
anonymous-class facts    569       569
incorrect callee ranges  569         0
affected files           182         0
```

The first correction (range the `class` keyword) reduced the count to 9; the
remaining 9 were attributed anonymous classes (`new #[Queue('x')] class ...`),
where the node begins at `#[`. Locating the `class` direct child fixed those
too. Regression test:
`tests/php_adapter.rs::anonymous_class_construction_ranges_only_the_keyword`,
covering both a plain and an attributed anonymous class. See
`docs/TASK2_FINDINGS.md` F003 and `diagnostics/F003-php-anonymous-class.txt`.

## 25. Cross-language comparison

```text
language  visited  supported  clean  recovered  recovery%  parse-failures
go          4,047      2,140   2,140          0      0.00%  0
php         5,047      4,053   4,052          1      0.02%  0
python      7,874      3,341   3,341          0      0.00%  0
rust        2,197      1,520   1,503         17      1.12%  0
```

```text
language  kLOC/s  MiB/s  peak RSS (end-to-end)  ranges checked  bad ranges
php        112.1   3.38   121.0 MiB              7,006           0
go         101.6   2.62    31.7 MiB             15,993           0
python      86.3   3.03   102.4 MiB              2,326           0
rust        83.1   2.50    29.4 MiB             10,733           0
```

Rust is the slowest by kLOC/s and has the only material recovery rate, both
attributable to Rust's macro-heavy corpus and the F001/F002 grammar boundaries.
Go has the lowest memory footprint; PHP and Python peak higher, tracking their
larger per-file ASTs.

## 26. Stage performance analysis (synthetic matrix)

Synthetic artifacts:
`~/reposuite/repodex/benchmarks/task2-validation-20260917T101413Z-synthetic/`
(`results.txt`, `results.json`). The matrix separates discovery/read, parser
initialisation, raw parse, normalized extraction, and the combined stages, so a
slow stage can be attributed rather than averaged away.

Representative full parse + extraction:

```text
rust  100 KiB   ~22.4 ms
rust    1 MiB  ~255.6 ms
php     4 MiB  ~1,331.9 ms
```

The stage split shows that extraction is a substantial share of the combined
cost, not a rounding error — which is why the incremental end-to-end gain in
§31 is far smaller than the parse-only gain.

The default 8 MiB file-size guard behaved correctly in the matrix: an over-limit
file was rejected under the default limit and processed when the limit was
raised. §32 confirms this on real files at the exact boundary.

## 27. Real repository full-scan results

Five fresh processes per corpus, single-threaded, release build. Combined
campaign: **8,838 files, 55.2 MiB, 1,764,320 LOC, 824,781 facts.**

```text
corpus                  lang    files     LOC     median ms  files/s  MiB/s  kLOC/s
public:laravel/framework php    3,086   562,801    5,021.3   614.6    3.38   112.1
public:django/django     python 2,932   526,612    6,098.7   480.8    3.03    86.3
public:gohugoio/hugo     go       912   230,409    2,266.8   402.3    2.62   101.6
public:tokio-rs/tokio    rust     799   183,416    2,206.4   362.1    2.50    83.1
public:composer/composer php      589   133,843    2,086.2   282.3    2.19    64.2
public:clap-rs/clap      rust     338    84,668      925.5   365.2    2.68    91.5
public:gin-gonic/gin     go        99    24,226      296.6   333.8    2.23    81.7
public:pallets/flask     python    83    18,345      176.6   470.1    3.18   103.9
```

Five single repositories exceed 150k LOC (laravel, django, hugo, tokio, and the
local `vps_desk` Python tool at 309,855 LOC), so the single-repository size
requirement is met without a synthetic collection.

## 28. Throughput denominators

Throughput is reported against **supported files that were actually processed**,
not against visited files. Every corpus above is a single language, so the
denominator is the number of files of that language in the checkout. MiB/s uses
processed source bytes, not file-system allocation. kLOC/s uses the same LOC
counts as the corpus manifest. Synthetic sources are regular and repetitive and
are likely friendlier than real code to both the parser and the extractor, so the
synthetic figures in §26 are an optimistic bound, not a target.

## 29. Determinism validation

Three full canonical scans each, comparing both the top-level canonical digest
and the complete fact signature:

```text
corpus                files   digest                    runs  distinct digests  distinct signatures
django/django         2,932   fnv1a64:0b54ce97af0cf2fb    3        1                 1
laravel/framework     3,086   fnv1a64:205500ee5ee155a7    3        1                 1
gohugoio/hugo           912   fnv1a64:9d4beccaf8bbfafc    3        1                 1
```

The same contents scanned from two different checkout roots produced identical
digests and identical complete fact signatures:

```text
pallets/flask   rootA = rootB = fnv1a64:3ee14e1e50449c3c
spf13/cobra     rootA = rootB = fnv1a64:8c7d031ead80281c
```

Canonical output is therefore independent of run and of checkout path. Evidence:
`diagnostics/determinism.txt`.

## 30. Real-file incremental correctness

`tests/task2_real_incremental.rs` drives the incremental API over real corpus
files: **12 real files, 8 comparisons each** (single edits of several kinds plus
a multi-step edit sequence), asserting that the incremental result is equivalent
to an independent fresh parse+extract of the edited bytes. All comparisons were
equivalent. The test has an explicit skip path when `REPODEX_TASK2_CORPUS_DIR`
is unset, so the default suite passes without the corpus; the skip path itself
was exercised and passes.

## 31. Incremental performance

Incremental parsing reuses the previous tree, but extraction re-processes the
whole file, so the end-to-end gain is bounded:

```text
stage                            rust 100 KiB    php 1 MiB
fresh parse                          12.845 ms    215.740 ms
incremental parse only                0.675 ms      6.783 ms   (~19×, ~32×)
fresh parse + extraction             22.431 ms    414.228 ms
incremental parse + extraction       10.745 ms    156.137 ms   (~2.1×, ~2.7×)
```

Recorded as **F007**: TASK 3 must not assume a large end-to-end incremental
speedup while extraction remains whole-file.

## 32. Large / deep workload results

Evidence: `diagnostics/large-deep.txt`.

```text
exactly 8 MiB (8,388,608 bytes), default guard:  accepted, status recovered
  (a valid over-limit-free file is processed at the exact limit)

8,388,609 bytes (one over), default guard:      status unsupported, exit 1,
  diagnostic: file_too_large — "larger than the configured maximum of 8388608
  bytes (the read was truncated after 8388609 bytes)"; bounded read (0 bytes
  retained); no panic

8,388,609 bytes, --max-file-size 16777216:      processed, status recovered

deep nesting: 500 nested `if true {` levels,
  1,003 lines, 1,006,026 bytes:                 status clean, no stack overflow,
  no panic; 1 declaration recovered at scope (file)
```

The file-size guard refuses clearly and never panics; raising the limit is an
explicit operator action.

## 33. Process memory and retention A/B/C

Peak RSS of the end-to-end scan processes (`/usr/bin/time -v`, `Maximum resident
set size`):

```text
tokio/rust     30,132 KiB   29.4 MiB
hugo/go        32,480 KiB   31.7 MiB
django/python 104,876 KiB  102.4 MiB
laravel/php   123,920 KiB  121.0 MiB
```

The retention experiment (`examples/retention.rs`) parses and extracts a whole
corpus in three variants that differ only in what is kept alive after each
file's facts are produced:

```text
A  release the tree and the source buffer
B  retain the trees, release the source buffers
C  retain the trees and the source buffers
```

Each variant runs in its own process; peak RSS is the kernel high-water mark
(`VmHWM`). Evidence: `memory/retention-abc.txt`.

```text
corpus   variant  files  bytes       facts    retained trees  sources  peak RSS
tokio    A          874  5,793,899   62,373        0            0       10.4 MiB
tokio    B          874  5,793,899   62,373      799            0      164.0 MiB
tokio    C          874  5,793,899   62,373      799          799      169.6 MiB
hugo     A        2,568  6,236,812   73,233        0            0        9.2 MiB
hugo     B        2,568  6,236,812   73,233      912            0      158.6 MiB
hugo     C        2,568  6,236,812   73,233      912          912      164.5 MiB
django   A        7,091 19,402,248  278,589        0            0       20.8 MiB
django   B        7,091 19,402,248  278,589    2,932            0      518.2 MiB
django   C        7,091 19,402,248  278,589    2,932        2,932      536.8 MiB
laravel  A        3,411 17,817,062  266,600        0            0       36.2 MiB
laravel  B        3,411 17,817,062  266,600    3,095            0      650.5 MiB
laravel  C        3,411 17,817,062  266,600    3,095        3,095      667.5 MiB
```

Two conclusions:

1. **Fact counts are identical across A, B and C** in every language, so
   retention changes memory only, never output.
2. **Retaining trees dominates memory** (~15–18× over variant A); additionally
   retaining sources adds roughly the size of those buffers (~5–19 MiB). The
   B→C delta matches the source byte count in every case.

Recorded as **F006**: a TASK 3 repository map that retains a tree per file will
not scale. Trees must be released after extraction, or a bounded retention
window used.

## 34. Open findings

Full log with stable IDs: `docs/TASK2_FINDINGS.md`.

```text
F001  GRAMMAR / MEDIUM    Rust rejects a primitive-type name as a macro name — grammar
                          limitation, correctly reported as recovery
F002  EXTRACTION / MEDIUM Calls nested in Rust macro arguments are not extracted —
                          grammar-inherent recall boundary, documented policy
F003  RANGE / HIGH        PHP anonymous-class callee range — FIXED, regression-tested
F004  EXTRACTION / INFO   PHP trait `use` is not an import — not a defect
F005  EXTRACTION / INFO   Go `_test.go` filename evidence — not a defect
F006  MEMORY / INFO       Tree retention dominates memory — TASK 3 input
F007  PERFORMANCE / INFO  Incremental extraction is whole-file — TASK 3 input
F008  MODEL / MEDIUM      RecoveryError::Query declared but not producible — carried
                          from TASK 1
```

No unresolved BLOCKER or HIGH finding remains.

## 35. Decision

```text
architecture_recommendation: CONDITIONAL_GO
```

The evidence:

* The boundary held: syntax-shaped facts only, no semantic resolution, no
  cross-file resolution, no repository map, no LSP, no compiler integration.
* Real parsing is broadly reliable: 0 parser failures, 0 extraction failures,
  0.163% recovery among parsed files, all recoveries explained.
* All four adapters are substantive (extraction audit: 0 range mismatches, 0
  independently-found misses in every region).
* Canonical output is deterministic across runs and checkout roots.
* Incremental and fresh extraction agree on real files.
* Source ranges are exact on 320 real files and 26 CRLF conversions, including
  multibyte content.
* One genuine defect (F003) was found, fixed, regression-tested and re-verified.

The conditionality is the two quantified Rust grammar boundaries (**F001**,
**F002**) and the two TASK 3 design inputs (**F006** tree retention, **F007**
whole-file extraction). None is a foundation blocker; each constrains how TASK 3
may be built. This is why the verdict is `CONDITIONAL_GO` rather than `GO`.

## 36. TASK 3 readiness

TASK 3 can proceed, under the constraints recorded above:

```text
* do not retain a tree per file for the whole repository (F006)
* do not assume incremental extraction is cheap (F007)
* keep Rust macro recall boundaries visible rather than silently absent (F001, F002)
* keep recovery and grammar limitations explicit in any new layer
```

The smallest useful TASK 3 is a **bounded, non-retaining repository pass** that
consumes the existing normalized syntax facts and produces a per-file,
deterministic index — no semantic resolution, no resolved call graph, no
retained trees — so that the next layer is built on validated, deterministic
facts rather than on unresolved assumptions.

## 37. Reproducibility

```text
RepoDex SHA validated:   4a6e8e3fe88be068932279b3bf896c731c812859
corpus manifest:         benchmarks/corpora.json (pinned revisions, no local paths)
range-validation script: scripts/task2_range_validation.py
retention harness:       examples/retention.rs
validation plan:         docs/TASK2_VALIDATION_PLAN.md
                         (plan sha256 recorded in the run directory)
run directory:           ~/reposuite/repodex/benchmarks/<run-id>/
  environment.json  corpora.json  raw-results.jsonl  end-to-end.json
  verification.txt  memory/  diagnostics/  audit/
```

Corpus roots are supplied externally through environment variables; no
machine-specific absolute path is committed. The synthetic and end-to-end
numbers in this document were produced after the F003 fix, so no pre-change
performance is reported as post-change evidence.
