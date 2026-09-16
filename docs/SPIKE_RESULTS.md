# RepoDex Spike Results

Observed results only. Nothing in this document is extrapolated, and nothing
compares RepoDex against another implementation.

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
        five call forms, Test/Benchmark name and signature evidence,
        _test.go file evidence
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

Result: **125 tests, 125 passed, 0 failed.**

```text
tests/cli.rs              13 passed
tests/determinism.rs       8 passed
tests/expected_facts.rs    1 passed
tests/go_adapter.rs       12 passed
tests/incremental.rs       8 passed
tests/paths.rs             9 passed
tests/php_adapter.rs      12 passed
tests/python_adapter.rs   13 passed
tests/ranges.rs            6 passed
tests/recovery.rs          8 passed
tests/rust_adapter.rs     13 passed
tests/scanner.rs          10 passed
tests/shared_fixtures.rs  12 passed
```

`tests/expected_facts.rs` compares the complete canonical fact set of all 26
fixtures, so one test covers 26 fixture expectations.

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
  static/scoped, generic, indirect, explicit construction, macro invocation.
* `dynamic_callee` and `nullsafe` mark the cases where the written callee is not
  a static name or where PHP uses `?->`.
* Go generic instantiation has two grammar shapes and RepoDex reports exactly
  what the grammar produced:
  * `Generic[int](value)` is a `type_conversion_expression` and is **not**
    reported as call-like.
  * `Generic[int, string](value)` is a `call_expression` and **is** reported as
    call-like, with `type_arguments = [int, string]`.
  * `Generic[int]` without a call is a `type_instantiation_expression`.
* Go `int64(5)` is a `call_expression` and is reported as call-shaped. It is
  never claimed to be a call rather than a conversion.
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

## 9. Incremental parsing observations

Command:

```bash
cargo test --locked --test incremental
```

Result: **8 tests, 8 passed.**

Every case compares an incrementally parsed tree against an independent full
parse of the same bytes, on all of:

```text
tree structure digest (node kind, range, namedness, ERROR/MISSING flags)
complete canonical facts
diagnostics
recovery state and status
```

Cases covered:

```text
Rust and Go   same-length identifier rename, insertion, deletion, newline
              insertion, UTF-8 multibyte insertion, UTF-8 multibyte
              replacement (Rust), edit near EOF
Rust          introduce a syntax error, then repair it in the same chain
Go            five consecutive chained edits
Rust (CRLF)   three chained edits on a CRLF source
Python        four chained edits, including a decorator and multibyte text
PHP           four chained edits, including an inserted declaration
Rust fixture  rename and restore, then compare against the original analysis
```

Every case reported `tree_structure_equal = true`, `facts_equal = true`,
`diagnostics_equal = true` and `recovery_equal = true`. The intermediate damaged
step really was `Recovered` and the repaired step was `Clean` again.

Changed ranges are recorded in `IncrementalOutcome::changed_ranges` for
observation only. They are never used to invalidate facts: after an incremental
parse RepoDex re-extracts the whole file, as TASK 1 requires.

## 10. Benchmark harness results actually measured

Two harnesses were run. Both were built with the release profile.

### 10.1 Synthetic size matrix

```bash
cargo run --release --bin repodex-bench -- --language all --size all \
    --run-id task1-verification --rustc "$(rustc --version)"
```

Artifacts were written to
`<REPOSUITE_REPODEX_HOME>/benchmarks/task1-verification/{results.txt,results.json}`.
Generated sources are deterministic and are removed when the harness exits.

Throughput rows are mean or single-pass durations of the stage named in the row.
`-` means the stage processed no bytes, so no throughput is reported for it.

```text
raw parse                    (tree-sitter only, warm parser, mean of N parses)
normalized extraction        (extraction only, tree already parsed)
parse + extraction           (single file, warm parser)
incremental parse            (append at EOF reusing the old tree)
incremental parse + extract  (incremental parse, then whole-file re-extraction)
```

```text
lang    size        raw parse      extraction     parse+extract   incr.parse   decls    calls
rust    100KiB      8.14 MiB/s      9.19 MiB/s      4.43 MiB/s     0.501 ms     2080      624
rust    1MiB        7.91 MiB/s      8.75 MiB/s      4.37 MiB/s     6.773 ms    20780     6234
rust    4MiB        7.76 MiB/s      8.68 MiB/s      4.34 MiB/s    27.771 ms    82340    24702
rust    over-8MiB   7.99 MiB/s      7.55 MiB/s      4.26 MiB/s    52.250 ms   163020    48906
go      100KiB      6.78 MiB/s      7.70 MiB/s      3.66 MiB/s     0.780 ms     3025      336
go      1MiB        6.47 MiB/s      8.60 MiB/s      3.93 MiB/s    10.304 ms    29836     3315
go      4MiB        6.43 MiB/s      7.47 MiB/s      3.75 MiB/s    41.780 ms   117370    13041
go      over-8MiB   6.16 MiB/s      7.72 MiB/s      3.81 MiB/s    80.289 ms   231526    25725
python  100KiB      6.29 MiB/s      7.92 MiB/s      3.71 MiB/s     2.485 ms     2530      506
python  1MiB        6.12 MiB/s      9.29 MiB/s      4.00 MiB/s    24.394 ms    25035     5007
python  4MiB        6.57 MiB/s      8.29 MiB/s      4.00 MiB/s   100.258 ms    97985    19597
python  over-8MiB   6.66 MiB/s      8.33 MiB/s      3.88 MiB/s   207.542 ms   194280    38856
php     100KiB      4.82 MiB/s      6.35 MiB/s      2.93 MiB/s     0.388 ms     1732      433
php     1MiB        4.91 MiB/s      6.92 MiB/s      3.06 MiB/s     4.956 ms    17352     4338
php     4MiB        5.17 MiB/s      6.53 MiB/s      3.02 MiB/s    20.236 ms    68548    17137
php     over-8MiB   5.30 MiB/s      6.52 MiB/s      3.08 MiB/s    37.345 ms   136264    34066
```

File discovery, including read, parse and extraction of a single-file directory:

```text
rust    1MiB  252.1 ms (3.97 MiB/s)     4MiB  966.3 ms (4.14 MiB/s)
go      1MiB  286.0 ms (3.50 MiB/s)     4MiB 1087.4 ms (3.68 MiB/s)
python  1MiB  289.9 ms (3.45 MiB/s)     4MiB 1113.8 ms (3.59 MiB/s)
php     1MiB  374.2 ms (2.67 MiB/s)     4MiB 1431.0 ms (2.80 MiB/s)
```

The over-8 MiB row under the default limit is a rejection, not a parse:

```text
rust    over default max size   1.62 ms   rejected, 0 declarations
go      over default max size   3.85 ms   rejected, 0 declarations
python  over default max size   6.31 ms   rejected, 0 declarations
php     over default max size   1.48 ms   rejected, 0 declarations
```

With the limit raised above the file size the same file parses:

```text
rust    raised   2090.4 ms   163020 declarations   48906 calls
go      raised   2424.7 ms   231526 declarations   25725 calls
python  raised   2157.0 ms   194280 declarations   38856 calls
php     raised   2727.7 ms   136264 declarations   34066 calls
```

Nested source (600 nested containers) is measured but its byte count is small, so
its MiB/s numbers are not comparable with the size rows:

```text
rust    600 nested modules     13185 bytes    parse 10.06 MiB/s  extract 11.18 MiB/s  decls 601
go      600 nested closures    14387 bytes    parse  5.40 MiB/s  extract  5.19 MiB/s  decls   2
python  600 nested functions  731366 bytes    parse 30.94 MiB/s  extract 248.92 MiB/s decls 511
php     600 nested functions   14967 bytes    parse 10.21 MiB/s  extract  9.54 MiB/s  decls 600
```

### 10.2 Fixture corpus

```bash
cargo bench
```

Per-fixture parse + extraction, mean of 20 iterations after one warm-up:

```text
language   fixture                          bytes    ms/iter   MiB/s  declarations  calls
rust       rust/boundaries.rs                1555      0.339    4.38            18      5
rust       rust/calls.rs                      352      0.110    3.04             1     10
rust       rust/declarations.rs               728      0.178    3.91            21      3
rust       rust/impls.rs                      627      0.134    4.45            10      3
rust       rust/imports.rs                    219      0.053    3.98             0      0
rust       rust/malformed.rs                  589      0.096    5.88             5      0
rust       rust/tests.rs                      245      0.066    3.55             5      0
go         go/boundaries.go                  1057      0.220    4.59             8      5
go         go/calls.go                        307      0.122    2.40             3      9
go         go/declarations.go                 850      0.222    3.65            22      4
go         go/malformed.go                    174      0.033    4.97             4      0
go         go/sample_test.go                  489      0.148    3.16             8      2
python     python/boundaries.py               771      0.153    4.82             8      3
python     python/calls.py                    315      0.109    2.76             1     11
python     python/declarations.py             836      0.266    3.00            16      4
python     python/malformed.py                181      0.045    3.83             3      0
python     python/test_sample.py              535      0.117    4.35            11      1
php        php/boundaries.php                 571      0.147    3.71             9      1
php        php/calls.php                      716      0.281    2.43             5     11
php        php/declarations.php              1195      0.361    3.16            22      1
php        php/malformed.php                  191      0.072    2.53             5      0
php        php/tests.php                      404      0.093    4.14             7      0

fixture tree scan: 52 files visited, 26 supported, 22 clean, 4 recovered,
                   16929 bytes processed in 5.710 ms (2.83 MiB/s)
canonical scan digest: fnv1a64:586836712348bab0
```

These fixtures are tiny, so per-file and per-call overhead dominates the MiB/s
figures. They are reported because they were measured, not because they are
representative throughput numbers.

### 10.3 A real defect found by the harness

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

### 10.4 Peak memory

`NOT MEASURED YET`.

RepoDex reports no internal RSS metric, because a reliable one needs an external
platform tool. To measure it, use one of:

```bash
/usr/bin/time -v ./target/release/repodex-bench --language rust --size 4MiB
valgrind --tool=massif ./target/release/reposuite-repodex scan <repository>
```

No memory number is claimed in this document.

## 11. NOT MEASURED YET

```text
large third-party repository corpus            not measured
150k LOC cross-language corpus                 not measured
extraction precision/recall audit              not measured
comparison against the previous RepoDex        not measured
Rust-vs-Go performance verdict                 deliberately not produced
peak RSS                                       see 10.4
cold-cache filesystem performance              not measured
multi-threaded or parallel scanning            not implemented
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
* The `incremental parse` row measures one specific edit shape: an append at end
  of file, which Tree-sitter can satisfy by reusing almost the whole old tree.
  It is not representative of edits in the middle of a file. The equivalence
  suite covers other edit shapes for correctness, but their timings were not
  recorded.
* `over default max size (raised)` measures a first parse with a cold parser, so
  it includes grammar load. The `raw parse` row excludes it.
* The nested-source rows have very different byte counts per language, so their
  MiB/s figures must not be compared across languages.
* Synthetic sources are regular and highly repetitive, so they are likely to be
  friendlier than real code to both the parser and the extraction pass.
