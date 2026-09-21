# TASK 3F findings log

`REPODEX-T3F-RUST-CRATE-TARGET-TOPOLOGY-V1` — Rust crate/target topology. The
structural layer now discovers Cargo targets from repository `Cargo.toml`
metadata and assigns every indexed Rust file a crate membership (`one` /
`multiple` / `NoKnownCrate`). This is structural repository metadata — not
Cargo dependency resolution, not rustc name resolution.

## T3F-F001 — `CARGO_METADATA`, INFO — TOML parsed with the `toml` crate

`Cargo.toml` is parsed with the `toml` crate (v0.8, serde-compatible,
rust-lang-maintained) — never regex. Only the fields needed for local targets
are read: `[package]` (+`auto*`), `[lib]`, `[[bin]]`, `[[test]]`, `[[bench]]`,
`[[example]]`, `[workspace].members`/`exclude`. `[target.'cfg…'.dependencies]`
and `[dependencies]` tables are ignored — they are dependency config, not
target declarations.
**Disposition: implemented.**

## T3F-F002 — `TARGET`, INFO — default + explicit target conventions

Supported: `[lib] path`/`src/lib.rs`; `[[bin]]`/`src/main.rs`/`src/bin/*.rs`/
`src/bin/*/main.rs`; `[[test]]`/`tests/*.rs`; `[[bench]]`/`benches/*.rs`;
`[[example]]`/`examples/*.rs`. `auto*` flags disable a kind's autodiscovery.
`tests/`/`benches/`/`examples/` subdirectories are support modules, not target
roots. **Disposition: implemented.**

## T3F-F003 — `CRATE_ROOT`, HIGH (fixed) — crate-root module dir was wrong for non-lib/main roots

A crate root's submodules live in the directory *containing* the root file.
`rust_module_dir` returns `dir/<stem>` for leaf files — correct for `mod`
children but wrong for a `tests/foo.rs` or `src/bin/tool.rs` crate root, whose
`mod shared` must resolve to `tests/shared`, not `tests/foo/shared`. The
root module's dir is now `directory_of(root)` in `build_rust_crate_tree`. This
is identical for `lib.rs`/`main.rs`/`mod.rs` roots, so `rust_crates` (and TASK
3E candidates) are unchanged. **Disposition: fixed; regression-tested.**

## T3F-F004 — `MEMBERSHIP`, INFO — membership is module-tree reachability

A file is a member of every target whose `mod`-reachable module tree contains
it (the root file always included). `tests/support/*` shared modules are
members of *every* test target that declares `mod support` — six tokio files
have multiple memberships, all legitimately shared test-support modules.
`NoKnownCrate` is a valid class. **Disposition: implemented.**

## T3F-F005 — `MODULE_TREE`, INFO — cross-target isolation is structural

Each target is an independent crate root with its own module tree.
`crate::helper` inside `tests/it.rs` resolves within the `it` crate, never the
package's `lib` — verified by fixture. No `mod` edge crosses a package/target
boundary. **Disposition: implemented.**

## T3F-F006 — `WORKSPACE`, MEDIUM — workspace members/exclude are bounded globs

`[workspace].members`/`exclude` accept a single-segment `*`/`?` glob subset,
resolved only to directories containing a discovered `Cargo.toml`, never
outside the repository. A virtual `[workspace]` root contributes members but
is not a package. Nested non-member `[package]` manifests still register as
packages (membership is by tree reachability, not nearest-dir). Complex Cargo
workspace semantics (e.g. `members` package-ID syntax, default-members) are
unsupported and recorded as diagnostics rather than guessed.
**Disposition: bounded subset implemented; remainder documented.**

## T3F-F007 — `CRATE_ROOT`, INFO — a missing `link_fingerprint` check was a stale-artifact gap

`links verify` validated the fingerprint's *format* but never compared it to
the current build's `LinkFingerprint`, so a link artifact derived by older
rules verified `valid`. Added `LinkError::FingerprintMismatch` and a
`link_fingerprint == current` check (mirroring the candidate rule check). Now
a stale TASK 3B artifact is rejected under the new policy.
**Disposition: fixed.**

## T3F-F008 — `MODULE_TREE`, MEDIUM — `include!`/`#[path]`/generated modules unmodelled

`#[path = "..."] mod`, `include!`, `OUT_DIR`/build-script and proc-macro
generated modules are not modelled; files reached only through them keep
`NoKnownCrate`. Carries forward the existing `rust.mod` limitation.
**Disposition: documented, not expanded.**

## T3F-F009 — `DETERMINISM`, INFO — topology is checkout-independent

Target identity is `{package_dir}::{kind}::{name}` over repo-relative paths;
manifest digests are content-addressed. Identical bytes at two absolute roots
produce identical link digests (`/tmp/t3e-xr` and `/tmp/t3e-xr2` →
`ca0fa77c…`). **Disposition: verified.**

## T3F-F010 — `AMBIGUITY`, INFO — `no_crate` reclassified by target membership

On tokio, 5,275 of the ~8,747 prior-`no_crate` `qualified_path` calls are now
in files with a crate membership (integration_test 4,997, bench 214, example
157, bin 10); 3,472 remain `NoKnownCrate` (mostly member-package support files
not reached by any target `mod`, or files outside all packages).
**Disposition: measured; candidate V2 projection in §32 of the results.**

## T3F-F011 — `STRUCTURAL_PATH` design note (§43) — path segments should become a fact field

`qualified_path` currently re-derives segments via `callee_written.split("::")`.
As path semantics expand (import-aware prefixes, multi-segment generics, `Self`
/`super` chains with associated terminals), a first-class normalized
`path_segments: Vec<(text, range)>` plus `root_form` and `terminal_range` on
the call fact would remove the implicit re-serialization and make per-segment
provenance exact. **Recommendation: add when TASK 3E V2 lands — required for
per-hop provenance, not for the current single-split rule.**
