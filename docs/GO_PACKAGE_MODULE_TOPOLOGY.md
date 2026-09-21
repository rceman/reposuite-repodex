# Go Package & Module Topology (TASK 4A)

The structural answer to **"which Go package is this file in, and which Go
module owns that package?"** — built from persisted `package` clauses plus
repository-local `go.mod` files. Never `go list`, `go env`, a compiler, or a
module-cache lookup.

```text
RepositoryFactSnapshot + go.mod bytes (content-digest-addressed)
        ↓
GoPackageTopology
        ├── GoModule    one per discovered go.mod
        ├── GoPackage   one per (directory, package_name) clause group
        └── membership  file -> package
```

## GoModule

One per repository-local `go.mod` (root and nested). A `go.mod` is discovered
by scanning, then its `module` directive is read with the shared bounded parser
(`metadata::parse_module_directive` — not regex). A `go.mod` with no readable
`module` directive is *malformed*: it still bounds its subtree as a module
boundary (files under it are `NoKnownModule`), but yields no `GoModule`.

```text
module_id        = root_dir ("" -> "<root>")          deterministic, no checkout data
manifest_path    = repo-relative go.mod
root_dir         = directory containing the go.mod
module_path      = `module` directive value
manifest_digest  = content digest (invalidation)
```

## GoPackage

One per `(directory, package_name)` group of indexed files. Package identity is
`module-context + directory + package_name` — never the name alone: two
`package util` directories are distinct, and `foo`/`foo_test` never merge.

```text
package_id     = {module_root_dir or "nomodule"}@{directory}:{name}
directory      = repo-relative directory
name           = written package clause
kind           = ordinary | command_main | external_test
module_id      = nearest enclosing module (or none)
module_rel_dir = directory relative to module root
import_path    = module_path + "/" + module_rel_dir   (ordinary/command)
               = none for external_test and NoKnownModule
files          = member source files
```

## Package kinds

* **ordinary** — `package foo`.
* **command_main** — `package main` (a command; still a directory-scoped
  package, not a repo/module root by assumption).
* **external_test** — `package foo_test`: a distinct package that never gains
  same-package lexical visibility into `foo` and gets **no** fabricated
  canonical import path.

## File role

`_test.go` marks a **test file**; anything else is **source**. Role is separate
from package identity: `pkg/a_test.go` with `package foo` is a *test file* in
the *ordinary* `foo` package; `pkg/e_test.go` with `package foo_test` is a test
file in the *external-test* `foo_test` package.

## Module ownership

A file/package belongs to the **nearest enclosing `go.mod` directory** —
deepest `root_dir` prefix — subject to nested-module boundaries (§14). A
malformed `go.mod` is still a boundary: its subtree is excluded from the parent
module and becomes `NoKnownModule`. Files outside every `go.mod` are
`NoKnownModule` but still form a directory/package from their `package` clause.

## Ambiguity preserved

* A directory declaring **multiple package names** produces one package per
  name — never repaired or merged (§9).
* **Build constraints** (`//go:build`, `// +build`) are **not** evaluated;
  variants that would be tag-separated are preserved as distinct packages
  (§20).
* **Generated** files participate like any indexed file (§21).
* The repository scanner's ignore policy governs vendor/dependency trees; no
  new crawler (§22).

## Entities

The existing `go_package` entities (keyed `dir:name`, the membership that
`go.package.same_directory` and `go.import.local_module` reference) are
**enriched** with `package_kind` / `module` / `module_rel_dir` / `import_path`
provenance — ids unchanged so existing link targets stay valid. One `go_module`
entity per module is added. `go.import.local_module` now matches any
repository-local module path (nearest/longest prefix), so imports into nested
modules resolve locally (§25).

## Invalidation

Every consulted `go.mod` is a content-digest `MetadataDependency`; adding,
removing, or editing one (module path, nested module) invalidates the derived
link artifact. `LINK_RULE_ABI_VERSION` 2→3, `POLICY_VERSION_GO` 1→2.
