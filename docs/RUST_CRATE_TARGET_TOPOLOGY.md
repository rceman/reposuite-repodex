# Rust Crate & Target Topology (TASK 3F)

RepoDex derives a **crate/target topology** for every indexed Rust file: which
Cargo target (and therefore which crate) a file belongs to. This is the
structural answer to "which crate am I in?" — needed before any `crate::` /
`self::` / `super::` or relative-module path can be evaluated.

This is repository-local structural metadata. It is **not** Cargo dependency
resolution, feature resolution, `cfg` evaluation, or rustc name resolution.

## Model

```text
RepositoryFactSnapshot
        +
Cargo.toml bytes (read at link-build time, content-digest-addressed)
        ↓
RustTargetTopology
        ├── RustTarget       one per discovered target
        ├── RustCrate        a module tree per target (TASK 3B builder)
        └── memberships      file -> [target_id]
```

A **target** records:

```text
target_id        = {package_dir}::{kind}::{name}   (checkout-independent)
package_name     [package].name
package_dir      repo-relative dir of the package's Cargo.toml
kind             lib | bin | integration_test | example | bench
name             the Cargo target name
root_file        repo-relative crate-root source file
manifest_path    repo-relative Cargo.toml that declared it
manifest_digest  sha256 of that manifest
rule_id          the discovery rule (provenance)
```

## Discovery rules

| rule                              | meaning                                     |
|-----------------------------------|---------------------------------------------|
| `rust.target.lib.explicit_path`   | `[lib] path = "…"`                          |
| `rust.target.lib.default_src_lib` | `src/lib.rs` present                        |
| `rust.target.bin.explicit`        | `[[bin]] path` (or `src/bin/<name>.rs`)     |
| `rust.target.bin.default_src_main`| `src/main.rs` present                       |
| `rust.target.bin.default_src_bin` | `src/bin/*.rs` or `src/bin/*/main.rs`       |
| `rust.target.integration_test.*`  | `[[test]]` explicit / `tests/*.rs` default  |
| `rust.target.example.*`           | `[[example]]` explicit / `examples/*.rs`    |
| `rust.target.bench.*`             | `[[bench]]` explicit / `benches/*.rs`       |

`[package] autobins/autotests/autobenches/autoexamples = false` disables the
corresponding convention. `tests/`/`benches/`/`examples/` **subdirectories**
are support modules, not target roots — only files directly in the directory
are targets.

## Crate-root module directory

A crate root's submodules live in the directory *containing* the root file —
for `lib.rs`/`main.rs`/`mod.rs` and equally for a `tests/foo.rs` or
`src/bin/tool.rs` crate root (`mod shared` → `tests/shared/`, not
`tests/foo/`). `mod` children of *non-root* module files still use
`<dir>/<stem>`.

## Workspace

`[workspace].members`/`exclude` are matched with a single-segment `*`/`?` glob
subset resolved only to directories that contain a `Cargo.toml`. A virtual
workspace root contributes members but is not itself a package. Unsupported
workspace constructs are recorded as diagnostics, not guessed.

## Membership & isolation

`memberships[file]` = the target_ids whose `mod`-reachable module tree contains
the file. A file may be in **zero** (`NoKnownCrate`), **one**, or **many**
crates (shared test-support modules). Each target is an independent crate: a
`crate::` path inside `tests/it.rs` resolves within the `it` crate and can
never reach the package's `lib` — cross-target isolation is structural.

## Artifact integration

Each target is emitted as a `rust_crate_target` `StructuralEntity` whose
`files` are the crate's member files. Manifests are recorded as
`MetadataDependency` records, so any `Cargo.toml` change invalidates the link
artifact. `LINK_RULE_ABI_VERSION` and `POLICY_VERSION_RUST` are bumped to 2.
