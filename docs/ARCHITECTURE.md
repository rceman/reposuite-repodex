# RepoDex Architecture

This document describes what the TASK 1 implementation actually is. It does not
describe planned systems.

## Layers

```text
Parser (Tree-sitter runtime + one grammar per language)
    |
Language adapter (language-specific extraction)
    |
Normalized unresolved syntax facts
    |
FUTURE semantic resolution
    |
FUTURE repository map
    |
FUTURE navigator
```

Everything above `Normalized unresolved syntax facts` exists today. Everything
below it is out of scope for TASK 1 and does not exist in this crate.

## What Tree-sitter provides

```text
a syntax tree for one source snapshot
source-grounded structure
byte ranges and row/column points
syntactically visible declarations
syntactically visible imports
call-shaped expressions
incremental parsing against a previously edited tree
recovery information (ERROR and MISSING nodes)
```

## What Tree-sitter does not provide

```text
complete semantic validation
type resolution
cross-file resolution
runtime dispatch
a resolved call graph
reflection resolution
dependency injection resolution
framework semantics
behavior ownership
"why does this happen?"
```

RepoDex therefore never:

* treats `obj.F()` as proof that a specific method `F` runs;
* treats `Thing()` in Python as proof that `Thing` is a constructor;
* treats `pkg.F()` in Go as a resolved package function;
* treats `T(x)` in Go as a call rather than a possible conversion, or drops it
  because the grammar labelled it a conversion: `Generic[int](value)` is
  simultaneously a valid generic invocation and a valid conversion, so it is
  recorded as a call-shaped occurrence and never resolved;
* conflates the three distinct PHP forms below, or treats `Thing(...)` as an
  invocation or as construction;
* expands macros or evaluates `cfg`/build tags;
* claims that a test runner would collect a candidate test.

Two consequences of the macro policy are measured and documented as TASK 2
findings F001 and F002: Tree-sitter-rust represents macro arguments as flat
token trees with no expression subtree, so calls written *inside* a macro's
arguments are not extracted (F002), and a primitive-type name used as a macro
name (`str!`, `u32!`, …) is rejected by the grammar and reported as a recovered
parse rather than silently accepted (F001). RepoDex reports both explicitly and
never hides them behind a clean status.

### PHP callable and construction syntax

These three PHP forms are different constructs and RepoDex keeps them apart:

```php
Thing($arg);       // invocation syntax   -> CallLikeOccurrence, form plain_name
new Thing($arg);   // explicit construction -> CallLikeOccurrence, form explicit_construction
Thing(...);        // first-class callable creation -> Reference, kind first_class_callable
```

`Thing(...)` is **not** an invocation and **not** construction. It produces a
`first_class_callable` reference and no call-like occurrence. The callable is
invoked only when it is called, as in `Thing(...)($arg)`, which produces an
`indirect` call-like occurrence whose written callee is `Thing(...)` and whose
`dynamic_callee` flag is true.

## Module layout

```text
src/lib.rs              crate root and public re-exports
src/main.rs             CLI entry point
src/cli.rs              argument parsing, text and JSON rendering
src/canonical.rs        reproducible digests over canonical facts
src/incremental.rs      incremental-vs-full parse equivalence harness
src/input.rs            bounded, UTF-8-validated file input
src/model/mod.rs        LanguageId and re-exports
src/model/range.rs      SourceRange, LineIndex
src/model/facts.rs      scopes, declarations, imports, references, calls, evidence
src/model/diagnostic.rs diagnostics
src/model/analysis.rs   SourceFile, FileAnalysis, canonical rendering
src/parser/mod.rs       ParserRegistry, Analyzer, extract_from_tree
src/parser/builder.rs   FactBuilder, DeclarationDraft
src/parser/recovery.rs  ERROR/MISSING scanning
src/parser/rust.rs      Rust adapter
src/parser/go.rs        Go adapter
src/parser/python.rs    Python adapter
src/parser/php.rs       PHP adapter
src/paths/mod.rs        runtime path resolver
src/scanner/mod.rs      repository scanner
src/bin/repodex-bench.rs synthetic benchmark harness
benches/spike.rs        fixture-corpus benchmark harness
queries/<lang>/recovery.scm  recovery queries
```

## Processing pipeline

```text
analyze_path / analyze_bytes
    -> input validation (size limit, UTF-8)
    -> language selection (extension)
    -> ParserRegistry adapter lookup
    -> Tree-sitter parse
    -> FactBuilder::new + file scope
    -> adapter::extract(builder, root)
    -> recovery query scan (ERROR / MISSING)
    -> builder.finish()  (sort, dedupe, re-index, freeze)
```

`extract_from_tree` is the single extraction entry point. The incremental
harness calls it for both trees, so incremental parsing and full parsing can
never drift into different extraction code paths.

## Normalized model

All identifiers are dense indices into vectors, assigned in pre-order. There are
no random ids, no hashes of addresses, and no map iteration in any output path.

```text
SourceFile      snapshot_id, relative_path, language, byte_len, line_count,
                has_final_newline
Scope           scope_id, parent_scope_id, kind, name, range, header_range
Declaration     declaration_id, snapshot_id, language, relative_path, scope_id,
                kind, name, name_range, range, body_range, flags, test_evidence
ImportOccurrence import_id, form, module, module_range, relative_levels, scope_id,
                range, items
ImportItem      target, target_range, alias, alias_range, wildcard, category, range
ReferenceOccurrence reference_id, kind, written, range, scope_id, declaration_id
CallLikeOccurrence  call_id, form, callee_written, callee_range, expression_range,
                type_arguments, scope_id, dynamic_callee, nullsafe
TestEvidence    kind, detail, range
Diagnostic      severity, kind, message, range
```

Deliberately absent: `ResolvedSymbol`, `CallEdge`, `ResolvedReference`. TASK 1
does not resolve semantic identity, and the model must not suggest that it does.

### Declarations versus scopes

A declaration is a *written name*. A scope is a *lexical container*. They are
separate because the languages disagree about what contains what:

* **Go** — a method with a receiver is declared at file scope. The receiver type
  is recorded as an unresolved `receiver_type` reference attached to the method
  declaration, never as lexical nesting. Interface method elements *are*
  lexically inside the interface.
* **Rust** — `impl Type { .. }` opens an `impl` scope. It is not the declaration
  of `Type`; the written target is recorded as an `impl_target` reference and the
  written trait as an `impl_trait` reference. `impl` scopes carry no invented
  name, but their display path borrows the written target so two impl blocks stay
  distinguishable.
* **Python** — nested functions and nested classes keep full containment.
* **PHP** — namespace, class, trait, interface, enum and method contexts are all
  distinguishable scopes; a trait `use` inside a class body is trait composition,
  not an import.

Anonymous scopes (closures, lambdas, arrow functions) never receive a fabricated
name.

### Imports

Imports are unresolved syntax occurrences. Each occurrence preserves the
statement range, the form (`single`, `grouped`, `wildcard`, `include`), the
written module, Python `relative_levels`, and one entry per imported item with
its written target, alias, wildcard flag and category (`normal`, `function`,
`constant`, `blank`, `dot`).

Relative and root-relative information for Rust is preserved as written in the
target text (`crate::`, `super::`, `self::`). Python's `relative_levels` is stored
separately because the leading dots are not part of the written module.

Imports are never resolved to files, packages or modules.

### References

Only source-visible relationship syntax is extracted:

```text
Rust     impl_target, impl_trait, receiver
Go       receiver_type, embedded_type
Python   decorator, base_class
PHP      base_class, implemented_interface, trait_composition,
         closure_capture, include_target, first_class_callable
```

All references remain unresolved and carry the written text and its exact range.

### Call-like occurrences

`CallLikeOccurrence` is a call-shaped expression, not a call. Forms:

```text
plain_name            f(x)
qualified_path        a::b(x)
member_selector       obj.m(x), obj?->m(x)
static_scoped         Type::m(x)
generic               f::<T>(x) / f[T](x)
indirect              (expr)(x), handlers[0](x)
type_conversion       Go T(x) / Generic[int](x), grammar-labelled conversions
explicit_construction new Thing()
macro_invocation      m!(x)
```

`dynamic_callee` marks a callee whose written form is not a static name
(`$callable('x')`, `$widget->{$name}()`, `strlen(...)($x)`). `nullsafe` marks
PHP `?->`.

Ambiguities are recorded as the grammar shapes them and never resolved:

* Go `int64(5)` with one argument is a `call_expression` and is reported as
  call-shaped even though it is a conversion.
* Go `Generic[int](value)` with one argument is a
  `type_conversion_expression`. It is also exactly how a generic function is
  invoked with one argument, so it **is** reported as call-like, with the
  `type_conversion` form label and the exact range of its written type
  arguments. Neither reading is chosen.
* Rust tuple-struct construction `TupleStruct(1)` is call-shaped; it is not
  claimed to be a constructor call.
* PHP `strlen(...)` is first-class callable *creation* and is a reference, not a
  call; `strlen(...)($x)` is an invocation and is a call with
  `dynamic_callee = true`.
* Python `getattr(obj, "m")()` is two facts: a `plain_name` call for `getattr`
  and an `indirect` call whose written callee is `getattr(obj, "m")`.

### Test evidence

A test is never a declaration kind. A `Function` or `Method` declaration carries
`TestEvidence[]`:

```text
attribute                Rust #[test], #[tokio::test]; PHP #[Test]
decorator                Python @pytest.mark.parametrize(...)
function_name_convention Go Test/Benchmark prefix rules; Python test_ prefix;
                         PHP test prefix
signature_shape          Go single *testing.T / *testing.B parameter
class_base_syntax        Python TestCase base clause
file_name_convention     Go _test.go suffix; Python test_*.py prefix
```

`file_name_convention` is attached to the file, not to a declaration. No
evidence implies that a test runner would collect the candidate.

### Diagnostics and recovery

```text
Clean        no ERROR, no MISSING
Recovered    at least one ERROR or MISSING node
Unsupported  input could not be analyzed (encoding, size, language)
Failed       input could not be read
```

Recovery policy:

1. `ERROR` nodes and `MISSING` tokens are detected by a per-language
   `queries/<lang>/recovery.scm` query.
2. Each one produces exactly one recovery region and one diagnostic carrying the
   same range. MISSING regions are zero-width; ERROR regions are not.
3. Facts are still extracted from a recovered tree, so recoverable declarations
   around the damage survive.
4. A declaration whose name is not written in the source is never emitted. If
   recovery inserts a MISSING name, no declaration is fabricated.
5. Recovery may *consume* syntax: for example an unterminated Rust
   `fn incomplete(` can absorb a following `fn valid_last() {}` as a
   function-type parameter. RepoDex reports what the tree contains and never
   reconstructs what it might have meant.
6. Facts that intersect a recovery region keep their exact written ranges.

## Source ranges

```text
byte ranges are half-open: [byte_start, byte_end)
rows are zero-based
columns are zero-based UTF-8 byte columns
```

Never UTF-16 columns, never display columns, never 1-based rows, never a line
number as identity. The original source bytes are authoritative: newlines are
never normalized before positions are computed, so an LF file and a CRLF file
report the columns that actually exist in them.

Node ranges use the points Tree-sitter already computed. Ranges RepoDex derives
itself (a declaration header that ends where the body begins, a trailing `*` of a
wildcard import, a PHP callee span) go through `LineIndex`, a per-file
line-start table with binary-search lookup. The first implementation scanned the
source for every derived range, which made extraction quadratic in file size;
`tests/ranges.rs` pins `LineIndex` against an independent reference
implementation so the optimisation cannot change what a range means.

TASK 2 validated ranges on real files: 320 corpus files (36,058 ranges) and 26
LF→CRLF conversions (1,388 ranges), including multibyte content, all source-exact
with zero bad ranges. That validation found one genuine defect, **F003**: PHP
anonymous-class construction used `callee_written = "class"` but a `callee_range`
covering the whole `anonymous_class` node (569 facts in 182 files). It is fixed
by narrowing the callee range to the `class` keyword — robust to attributed
anonymous classes, whose node begins at `#[` — and pinned by
`tests/php_adapter.rs::anonymous_class_construction_ranges_only_the_keyword`.
See `docs/TASK2_FINDINGS.md`.

## Determinism

Canonical facts must be a pure function of the source bytes:

* fact ids are dense indices assigned in pre-order;
* references, calls, diagnostics and test evidence are sorted by
  `(byte_start, byte_end, kind, text)` in `finish()`;
* recovery regions are sorted and deduplicated;
* declarations are emitted in source order;
* the snapshot id is derived from the file bytes only;
* no timestamp, hostname, pid, elapsed time or absolute path can reach the model;
* the scanner sorts discovered paths, so filesystem order cannot matter;
* `canonical::digest` and `canonical::scan_digest` are FNV-1a 64-bit change
  detectors over complete canonical fact text, not counters.

`tests/determinism.rs` checks repeated analysis, identical content under
different roots, different file-creation orders, absence of timing metadata, and
that the scan digest changes when facts change even if counters do not.

## Incremental parsing

`src/incremental.rs` implements the TASK 1 experiment:

```text
parse original
    -> apply edit
    -> InputEdit
    -> Tree::edit
    -> incremental parse
    -> independent full parse of the same bytes
    -> extract_from_tree for both
    -> compare tree structure, ranges, recovery state, facts and diagnostics
```

`run_sequence` chains edits, so step *n* edits the incrementally parsed tree from
step *n-1*. Changed ranges are recorded for observation only. They are never used
as a fact-invalidation mechanism: after an incremental parse RepoDex re-extracts
the whole file, exactly as TASK 1 requires.

## Input handling

```text
UTF-8 only
default maximum size 8 MiB (DEFAULT_MAX_FILE_SIZE)
```

The size limit is enforced while reading, not from metadata: the reader takes
`limit + 1` bytes, so an oversized file is detected even when metadata is
missing, stale or lying. `InputError::TooLarge` records the configured `limit`
and the `observed` byte count, which is a lower bound on the real size because
the read stops as soon as the limit is exceeded.

Invalid UTF-8 produces an `unsupported_encoding` diagnostic with the offset of
the first invalid byte, never a panic and never a truncated parse.

## Runtime paths

`src/paths/mod.rs` is the only place that knows where RepoDex state lives:

```text
<root>/config, cache, indexes, projects, benchmarks, tmp
<root> = REPOSUITE_REPODEX_HOME or <home>/reposuite/repodex
```

`RepoDexPaths` never touches the filesystem. TASK 1 commands do not create any
runtime directory; only the benchmark harness writes, and only under
`benchmarks/`.

## Scanner

```text
recursive walk (ignore crate, root-scoped .gitignore honoured by default)
pruned: .git, target, vendor, node_modules, __pycache__, .venv, venv
symlinks are not followed
supported extensions: rs, go, py, php
deterministic sorted output
per-file outcome: parsed, recovered, skipped, failed
```

A missing or non-directory root is a scan-level error rather than an empty
result, because silently reporting "0 files" would hide a typo. Per-file read
failures never abort the scan.

## Dependency discipline

```text
tree-sitter + four grammars   parsing
serde, serde_json             JSON output
ignore                        repository walking with gitignore support
```

No async runtime, no database, no daemon, no watcher, no network access, no
framework dependency. Nothing is included "for later".
