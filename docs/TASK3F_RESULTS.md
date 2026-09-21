# TASK 3F — Rust Crate & Target Topology Foundation — Results

`REPODEX-T3F-RUST-CRATE-TARGET-TOPOLOGY-V1` extends TASK 3B's structural layer
with Cargo crate/target topology: every `lib`/`bin`/`integration_test`/
`example`/`bench` target is an independent crate root discovered from
`Cargo.toml`, and each indexed Rust file gains an explicit crate membership.
This is structural metadata — not Cargo dependency resolution, not rustc name
resolution. **No call-candidate semantics changed** (TASK 3E rule untouched).

## 1–2. Base and implementation SHAs

Base HEAD: `701f17ffa1a845bd063c9f35bb236809cf37ef4b` — clean, `2bbfdee`
(TASK 3E) is an ancestor.

Implementation commit: recorded below.

## 3. Link ABI / policy

`LINK_RULE_ABI_VERSION` 1→2; `POLICY_VERSION_RUST` 1→2; link fingerprint
regenerated (`rule_abi=2 rust=2`). TASK 3A analyzer ABI unchanged (no
extraction change); candidate ABI unchanged (§39 — only the link digest
dependency moves).

## 4–9. Topology model, Cargo fields, conventions, paths, workspace

* Model: `RustTargetTopology` = `RustTarget` records + one `RustCrate` module
  tree per target (built by the existing `visit_rust_module`) + `file →
  [target_id]` memberships. Emitted as `rust_crate_target` entities.
* Cargo fields parsed (via `toml` crate): `[package]`/`auto*`, `[lib]`,
  `[[bin]]`, `[[test]]`, `[[bench]]`, `[[example]]`, `[workspace]` members/
  exclude.
* Default conventions: `src/lib.rs`, `src/main.rs`, `src/bin/*.rs`,
  `src/bin/*/main.rs`, `tests/*.rs`, `benches/*.rs`, `examples/*.rs`.
* Explicit `path=` overrides win over defaults; provenance records manifest +
  digest + rule.
* Workspace: `members`/`exclude` via bounded single-segment `*`/`?` globs to
  `Cargo.toml`-bearing dirs. Limitations: `default-members`, package-ID
  member syntax, and nested non-member manifests are handled permissively or
  diagnosed, not fully Cargo-faithful.

## 10–15. Target kinds, identity, membership, ambiguity, isolation, integration

Kinds: `lib`,`bin`,`integration_test`,`example`,`bench`. Identity is
`{package_dir}::{kind}::{name}` over repo-relative paths (checkout-,
output-, time-, PID-independent). Membership = `mod`-reachable module-tree
files; `NoKnownCrate` is valid. Ambiguity is preserved (a file may be in many
targets — e.g. a shared `tests/support` module). Cross-target isolation is
structural: a `crate::` in `tests/it.rs` resolves only inside the `it` crate.
Topology integrates into the link artifact as entities; `rust_crates` (the
candidate-facing lib/main view) is unchanged.

## 16–18. Fixtures / workspace / cross-crate

`tests/crate_topology.rs` — 15 topology cases + 5 update-vs-fresh. Covers:
default lib/main, lib+bin, explicit `[lib]`/`[[bin]]`/`[[test]]`/`[[example]]`/
`[[bench]]` paths, `src/bin/*.rs` + `src/bin/*/main.rs`, integration test as a
separate crate, examples, benches, no-known-crate, workspace two-member /
glob / exclude, same module name in two packages, and cross-target isolation
(a `tests/it.rs` `crate::helper` never sees the lib). All pass.

## 19–26. Tokio real-repository result

```text
manifests: 13   packages: 12
targets by kind: lib 7, bin 5, integration_test 244, example 21, bench 18  (=295)
distinct crate roots: 295
Rust files: 1 membership 401 · multiple 6 · none 392 · total 799
```

**Prior `no_crate` = 8,458 qualified-path calls → reconciliation:**

```text
now attached to a crate:        5,040   (int_test 4,778, bench 199, example 156, bin 10)
still NoKnownCrate:             3,418
```

Projected TASK 3E eligibility among the 5,040 newly-attached calls (no
candidate emitted — measurement only):

```text
crate::            1     self::  0    super::  0
relative module / other  1,234
external_crate     1,197
type/associated    2,608
```

The topology mainly unlocks `crate`/`self`/`super`/relative-module paths in
test/bench/example targets; most newly-attached calls are `tokio::`/`Type::`
external or associated calls, which remain out of scope.

## 27–31. Audit

`scripts/task3f_audit.py` independently re-derives the topology (`tomllib`
parsing, own target enumeration, own module-tree membership) for **all 295
targets and all 407 member files**:

```text
CORRECT_TARGET        295
CORRECT_MEMBERSHIP    407
FALSE_TARGET            0
WRONG_CRATE_MEMBERSHIP  0
MISSING_TARGET          0
MISSING_MEMBERSHIP      0
```

## 32–36. Determinism / invalidation / stale chain

* Repeated build → identical `link digest`; different output dir identical.
* Cross-root: `/tmp/t3e-xr` and `/tmp/t3e-xr2` (identical bytes, snapshot
  `b78ee0fd`) → identical link digest `ca0fa77c`.
* Update-vs-fresh: 5 target-relevant cases (target added/removed, explicit
  path changed, workspace member added, source changed) all equal.
* `Cargo.toml` change → `metadata_dependency_changed` rejection.
* Old TASK 3B artifact (fingerprint `7101754c`, abi 1) → `FingerprintMismatch`
  under the 3F build (a `link_fingerprint` check gap in `verify` was found and
  fixed — T3F-F007).
* Old TASK 3E candidate artifact → `link_digest` mismatch (stale through the
  link dependency).

## 37–39. Size / perf / RSS

```text
link artifact: 4,549,242 → 4,766,659 B (+217,417, +4.78%)
build ~1.2 s (was ~0.9 s); RSS ~94 MB
```

## 40. Structured qualified-path fact (§43)

`callee_written.split("::")` is sufficient today; as path semantics expand a
first-class `path_segments`/`root_form`/`terminal_range` field would give exact
per-segment provenance — recommended for TASK 3E V2, not required now
(T3F-F011).

## 41–45. Findings / verification / status

`docs/TASK3F_FINDINGS.md` T3F-F001…F011; `docs/RUST_CRATE_TARGET_TOPOLOGY.md`.
No new BLOCKER — T3F-F003 (crate-root dir) and T3F-F007 (fingerprint check)
were found and fixed. Gates: fmt/check/test(415)/clippy/release PASS.
`TASK3F_COMPLETE`.
