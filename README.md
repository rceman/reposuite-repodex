# RepoSuite RepoDex

Deterministic, source-grounded repository analysis.

RepoDex reads source files, parses them with Tree-sitter and emits a small,
normalized model of **unresolved syntax facts**: declarations, lexical scopes,
imports, references, call-shaped expressions, test evidence and parser recovery
information.

## What this repository currently is

This repository is the **TASK 1 foundation spike**, validated by **TASK 2**. It
establishes one pipeline and stops there:

```text
repository files
    -> language detection
    -> Tree-sitter parsing
    -> language-specific extraction
    -> normalized unresolved syntax facts
```

```text
TASK 1 status:               FOUNDATION_SPIKE_COMPLETE
TASK 1 commit validated:     4a6e8e3fe88be068932279b3bf896c731c812859
TASK 2 validation status:    VALIDATION_COMPLETE
TASK 2 architecture verdict: GO
```

TASK 2 validated the foundation against 2.42M manifest LOC (2.16M processed LOC)
across Go, PHP, Python and Rust: zero parser, extraction and read failures; 0.163%
recovery among parsed files; source-exact ranges; deterministic canonical output;
and incremental equivalence on real files with enforced per-language coverage. It
found and fixed one genuine defect (a PHP anonymous-class source-range bug, F003)
and quantified two Rust grammar boundaries (F001, F002). The completed
extraction-quality audit is at occurrence level with byte-span matching: 100%
precision in every category and language, and 97.0% strict call recall (426/439)
with every miss being the documented macro-argument boundary. See
`docs/TASK2_FINAL_REPORT.md`, `docs/TASK2_FINDINGS.md` and
`docs/SPIKE_RESULTS.md` Part II.

Everything downstream of that is deliberately absent:

```text
normalized unresolved syntax facts
    -> FUTURE semantic resolution
    -> FUTURE repository map
    -> FUTURE navigation
    -> FUTURE investigation knowledge
```

RepoDex does **not** resolve symbols, resolve references, build a call graph,
claim runtime behavior, evaluate `cfg`/build tags, expand macros, or infer
framework semantics. `obj.F()` is recorded as a member-selector call shape, not
as proof that a specific method `F` runs.

## Supported languages

| Language | Extensions | Grammar | ABI |
| --- | --- | --- | --- |
| Rust | `.rs` | `tree-sitter-rust` 0.24.2 | 15 |
| Go | `.go` | `tree-sitter-go` 0.25.0 | 15 |
| Python | `.py` | `tree-sitter-python` 0.25.0 | 15 |
| PHP | `.php` | `tree-sitter-php` 0.24.2 | 15 |

Runtime: `tree-sitter` 0.27.0. See `docs/LANGUAGE_SPIKE.md` for the per-language
support matrix and `docs/ARCHITECTURE.md` for the model and the boundaries.

## Build

```bash
cargo build --locked
cargo build --locked --release
```

Stable Rust. No async runtime, no database, no daemon, no network access.

## Commands

The CLI is **experimental**. It is a TASK 1 inspection tool, not a compatibility
contract.

### `languages`

```bash
cargo run --locked -- languages
cargo run --locked -- languages --json
```

Reports each language, its extensions, grammar version, grammar ABI, whether the
grammar loads, and the resolved runtime root.

### `parse <file>`

```bash
cargo run --locked -- parse fixtures/rust/declarations.rs
cargo run --locked -- parse fixtures/rust/declarations.rs --json
cargo run --locked -- parse fixtures/rust/declarations.rs --json --include-facts
```

Prints the language, analysis status, file metadata, counts, diagnostics and the
normalized facts. `--include-facts` adds the complete canonical fact text, which
is the deterministic representation used by the determinism and incremental
tests.

`--max-file-size <bytes>` raises or lowers the input limit for this invocation.

### `scan <repository>`

```bash
cargo run --locked -- scan .
cargo run --locked -- scan . --json
cargo run --locked -- scan . --json --include-facts
```

Recursively discovers `.rs`, `.go`, `.py` and `.php` files, analyzes each one and
reports:

```text
visited / supported / unsupported files
parsed clean / parsed with recovery / trees returned
skipped (size limit, encoding) / read / parser / extraction failures
traversal failures, and whether the scan covered the whole tree
declarations, imports, references, call-like occurrences, test candidates
recovery rate among parsed files
bytes processed and visited, elapsed time
```

A directory the walk cannot enter or an entry it cannot stat is reported, never
silently dropped. The scan continues with the rest of the tree, but it does not
claim complete coverage: the text output prints `SCAN INCOMPLETE`, the JSON
output carries a `traversal_failure_details` array, a `traversal_failures`
count and `scan_complete: false`, and the exit code is the analysis-failure code.

Provoking that failure needs a directory the process cannot read, which is a
POSIX operation, so the two tests that inject it are gated with `#[cfg(unix)]`.
The other half of the contract — a scan that visits everything reports itself
complete — is platform-independent and runs everywhere. No Windows ACL
manipulation is implemented, and the test targets type-check for
`x86_64-pc-windows-gnu`.

Scanning this repository itself (counts change as the repository grows):

```text
visited 105, supported 65, unsupported 40
parsed clean 61, parsed with recovery 4 (the malformed fixtures)
traversal failures 0 (scan covered the whole tree)
1355 declarations, 159 imports, 100 references, 5219 call-like, 169 test candidates
508195 bytes in 115 ms
```

Files are sorted deterministically. `.git/`, `target/`, `vendor/`,
`node_modules/`, `__pycache__/`, `.venv/` and `venv/` are pruned. Symlinks are
never followed. `.gitignore` rules are honoured by default; use
`--no-gitignore` to disable them.

JSON goes to stdout, logs go to stderr.

### Exit codes

```text
0  completed without recovery, analysis failures or traversal failures
1  completed, but at least one file needed recovery or failed, or part of the
   tree could not be visited
2  invocation or setup failure
```

## Tests

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
```

Integration suites live in `tests/`:

```text
tests/rust_adapter.rs        Rust extraction semantics
tests/go_adapter.rs          Go extraction semantics
tests/python_adapter.rs      Python extraction semantics
tests/php_adapter.rs         PHP extraction semantics
tests/expected_facts.rs      complete canonical fact sets for every fixture
tests/recovery.rs            ERROR/MISSING policy
tests/shared_fixtures.rs     encodings, line endings, adversarial source
tests/determinism.rs         byte-identical repeated analysis
tests/incremental.rs         incremental vs full parse equivalence
tests/scanner.rs             discovery, pruning, per-file failures,
                             traversal-failure reporting, scan completeness
tests/paths.rs               runtime path resolution
tests/cli.rs                 command behaviour and exit codes, scan
                             completeness in the JSON output
tests/canonical_completeness.rs
                             every normalized field is visible to the
                             canonical text and to structural equality
tests/tree_comparator.rs     every property the tree digest compares, and
                             that the digest separates trees that differ
tests/query_limits.rs        query match-limit exhaustion is reported
```

`fixtures/expected/**` holds the complete canonical fact set of every fixture.
Regenerate deliberately, then read the diff:

```bash
REPODEX_BLESS=1 cargo test --test expected_facts
```

Blessed expectations are never the only evidence: the per-language suites assert
extraction semantics independently.

## Benchmarks

Two harnesses exist. Both report only what they observed and neither compares
RepoDex against another implementation.

```bash
# Fixture corpus, no arguments, no output directory:
cargo bench

# Synthetic size matrix, incremental stages and the 8 MiB limit:
cargo run --release --bin repodex-bench -- --language all --size all
```

`repodex-bench` writes `results.txt` and `results.json` under
`<runtime root>/benchmarks/<run-id>/`. Generated sources are deterministic and
live in a temporary directory that is removed on exit, so no large generated
input is committed.

Sizes: `100KiB`, `1MiB`, `4MiB`, `over-8MiB`, `nested`. Use
`--size standard` for the first three, or `--output <dir>` / `--run-id <id>` to
control the artifact location.

`results.json` carries one machine-readable row per measurement, with the run id,
language, size, stage, input and processed bytes, file count, duration,
throughput, fact counts and the row's note. Every stage name describes exactly
what its timer covers. The harness asserts that every generated source, at every
size, parses clean and that the incremental edit is valid for the language, so a
measurement cannot silently become an error-recovery measurement.

See `docs/SPIKE_RESULTS.md` for the measurements that were actually taken and an
explicit list of what was not measured.

## Runtime root

All future RepoDex runtime state belongs under one root:

```text
<root>/
├── config/
├── cache/
├── indexes/
├── projects/
├── benchmarks/
└── tmp/
```

The default root is `<home>/reposuite/repodex`, where `<home>` comes from the
platform's home-directory environment variables. Override it with:

```bash
export REPOSUITE_REPODEX_HOME=/path/to/repodex-home
```

TASK 1 only *resolves* these paths. `languages`, `parse` and `scan` never create
directories, so no persistent runtime state appears during normal use. Only the
benchmark harness writes anything, and only under `benchmarks/`.

## File input rules

```text
UTF-8 only
default maximum size: 8 MiB (overridable per invocation)
invalid UTF-8 -> unsupported-input diagnostic, never a panic
oversized input -> size-limit diagnostic, never a truncated parse
missing/unreadable file -> per-file failure diagnostic
```

## Known limitations

* No semantic resolution of any kind: no resolved symbols, references, call
  graphs or runtime claims.
* Macros are not expanded, `cfg` and build tags are not evaluated, so
  macro-generated declarations do not exist in the model.
* Calls written inside Rust macro arguments are not extracted, because
  Tree-sitter-rust represents macro arguments as flat token trees with no
  expression subtree (TASK 2 finding F002).
* Tree-sitter-rust rejects a primitive-type name used as a macro name (`str!`,
  `u32!`, …); such files are reported as recovered, not silently accepted
  (TASK 2 finding F001).
* Go conversions and generic instantiations are reported exactly as the grammar
  shapes them; see `docs/LANGUAGE_SPIKE.md`.
* A recovered parse keeps neighboring declarations, but recovery may consume
  syntax after a damaged declaration. RepoDex never fabricates the missing
  parts.
* Only `.rs`, `.go`, `.py` and `.php` are supported. JavaScript, TypeScript,
  Svelte and framework-specific analysis are out of scope.
* The CLI output format is experimental.

## Documentation

```text
docs/ARCHITECTURE.md        layers, model, ranges, determinism, what Tree-sitter does not give
docs/LANGUAGE_SPIKE.md      per-language grammar, strategy, quirks and unsupported constructs
docs/SPIKE_RESULTS.md       observed results only; TASK 1 baseline (sections 1-12) and TASK 2 validation (Part II)
docs/TASK2_VALIDATION_PLAN.md  the pre-registered TASK 2 validation plan
docs/TASK2_FINDINGS.md      TASK 2 findings log with stable IDs (F001..F008)
docs/TASK2_FINAL_REPORT.md  the itemized TASK 2 final report
```
