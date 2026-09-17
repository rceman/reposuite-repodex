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

Every lint allowance in the codebase is listed here. An allowance is not a
defect, but an undocumented one is, and a `#[allow]` guarding code nothing calls
is dead code rather than a justified allowance.

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
           are one fact, not eight parameters, and every adapter calls it.
measure    takes the analyzer, the parser registry, the language, the size name,
           the file name, the path and the source.
row        takes the benchmark row fields positionally.
reason     bundling either into a struct would add a type whose only purpose is
           to satisfy the lint, and would move the fields away from the call
           sites that read as a table.

tests/support/mod.rs  the shared helper module is compiled into every
                      integration test binary, and each binary uses a subset of
                      the helpers, so an unused one is expected rather than
                      suspicious.
```

Any new allowance must be added here with its reason. Do not remove a justified
allowance to claim there are none; do remove the code that makes an unjustified
one necessary.

## Portability

The crate must build and its test targets must type-check on native Windows as
well as on Unix. Any platform-specific API belongs behind a `#[cfg]` gate —
including imports, since an import used only by a gated item becomes an unused
import on the other platform and fails `-D warnings` there.

The test targets are cross-checked with:

```bash
cargo clippy --locked --target x86_64-pc-windows-gnu --all-targets \
  --all-features -- -D warnings
```

That is a cross-target type-check, not a native Windows run; do not describe it
as one. Do not add Windows ACL manipulation.

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

## TASK 2 validation tooling

```bash
# source-range validation over a real corpus (no machine-specific path committed)
python3 scripts/task2_range_validation.py \
    --binary ./target/release/reposuite-repodex \
    --root "$CORPUS" --language rust --limit 80

# tree/source retention A/B/C memory experiment (Linux; peak 0 elsewhere)
cargo run --release --example retention -- "$CORPUS" A

# real-file incremental equivalence; skips cleanly when the corpus is absent
REPODEX_TASK2_CORPUS_DIR="$CORPUS" cargo test --locked --test task2_real_incremental
```

The pinned corpus manifest is `benchmarks/corpora.json`; local checkout paths are
supplied externally through environment variables and are never committed. Raw
benchmark output stays under `~/reposuite/repodex/benchmarks/<run-id>/` and is
not committed.

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
