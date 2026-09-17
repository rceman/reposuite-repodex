# AGENTS.md

RepoSuite RepoDex, TASK 1 foundation spike. See `README.md` for what the project
is and `docs/ARCHITECTURE.md` for the model and the architectural boundary.

## Build

```bash
cargo build --locked
cargo build --locked --release
```

Stable Rust, no async runtime, no database, no network access.

## Verify (required before considering any change complete)

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release
cargo bench
cargo run --release --bin repodex-bench -- --language all --size all
```

Do not silence warnings to obtain green output, and do not weaken assertions to
hide a parser or extraction limitation.

There is exactly one lint allowance in the codebase:

```text
src/bin/repodex-bench.rs   #[allow(clippy::too_many_arguments)] on `row(...)`
                           and on `measure(...)`
```

Both take the benchmark row fields positionally; bundling them into a struct
would add a type whose only purpose is to satisfy the lint. Any new allowance
must be listed here with its reason, and the count must stay small.

## Tests

Integration suites live in `tests/`. `tests/support/mod.rs` holds the shared
helpers (`analyze_fixture`, `all_fixture_files`, `TempDir`,
`declaration_identity`, summary renderers).

```bash
cargo test --locked                      # everything
cargo test --locked --test rust_adapter  # one suite
cargo test --locked --test incremental
```

Three suites exist to enforce evidence claims rather than assert them, and must
stay green when the model changes:

```text
tests/canonical_completeness.rs   every normalized field must be visible to both
                                  `FileAnalysis::same_facts` and
                                  `FileAnalysis::canonical_text`. Adding a field
                                  to the model without rendering it fails here.
tests/tree_comparator.rs          every property the incremental tree digest
                                  claims to compare must appear in its output.
tests/query_limits.rs             query match-limit exhaustion must produce
                                  `AnalysisStatus::Incomplete`, never `Clean`.
```

Tests must never write into the user's real home directory. Use
`support::TempDir`.

## Expected canonical facts

`fixtures/expected/**` holds the complete canonical fact set of every fixture and
is compared by `tests/expected_facts.rs`. After an intentional extraction change:

```bash
REPODEX_BLESS=1 cargo test --test expected_facts
git diff fixtures/expected   # read it, do not accept it blindly
```

Blessed expectations are never the only evidence: every language suite asserts
extraction semantics independently.

## Benchmarks

```bash
cargo bench                                                     # fixture corpus
cargo run --release --bin repodex-bench -- --language all --size all
```

`repodex-bench` writes `results.txt` and `results.json` under
`<REPOSUITE_REPODEX_HOME or ~/reposuite/repodex>/benchmarks/<run-id>/`. Generated
sources are deterministic, live in a temporary directory and are removed on exit.
Record observed numbers in `docs/SPIKE_RESULTS.md`; never invent them.

## Project rules

* Facts stay unresolved and source-grounded. No resolved symbols, no resolved
  references, no call graph, no runtime claims, no repository map, no navigator.
* Do not add a dependency "for later". Every dependency needs a reason.
* New runtime state goes under the `src/paths` root; TASK 1 commands must not
  create directories.
* Preserve deterministic output: dense ids in pre-order, sorted collections, no
  timestamps, no absolute paths, no map iteration in output paths.
* Byte ranges are half-open, rows are zero-based, columns are zero-based UTF-8
  byte columns.
* Keep grammar-specific code inside the matching language adapter.
