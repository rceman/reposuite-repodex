# RepoSuite RepoDex

Deterministic, source-grounded repository analysis.

RepoDex reads source files, parses them with Tree-sitter and emits a small,
normalized model of **unresolved syntax facts**: declarations, lexical scopes,
imports, references, call-shaped expressions, test evidence and parser recovery
information.

## What this repository currently is

This repository is the **TASK 1 foundation spike**. It establishes one pipeline
and stops there:

```text
repository files
    -> language detection
    -> Tree-sitter parsing
    -> language-specific extraction
    -> normalized unresolved syntax facts
```

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
declarations, imports, references, call-like occurrences, test candidates
recovery rate among parsed files
bytes processed and visited, elapsed time
```

Scanning this repository itself:

```text
visited 102, supported 62, unsupported 40
parsed clean 58, parsed with recovery 4 (the malformed fixtures)
1232 declarations, 145 imports, 94 references, 4289 call-like, 143 test candidates
407109 bytes in 94 ms
```

Files are sorted deterministically. `.git/`, `target/`, `vendor/`,
`node_modules/`, `__pycache__/`, `.venv/` and `venv/` are pruned. Symlinks are
never followed. `.gitignore` rules are honoured by default; use
`--no-gitignore` to disable them.

JSON goes to stdout, logs go to stderr.

### Exit codes

```text
0  completed without recovery or analysis failures
1  completed, but at least one file needed recovery or failed
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
tests/scanner.rs             discovery, pruning, per-file failures
tests/paths.rs               runtime path resolution
tests/cli.rs                 command behaviour and exit codes
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
docs/ARCHITECTURE.md    layers, model, ranges, determinism, what Tree-sitter does not give
docs/LANGUAGE_SPIKE.md  per-language grammar, strategy, quirks and unsupported constructs
docs/SPIKE_RESULTS.md   observed results only, plus NOT MEASURED YET markers
```
