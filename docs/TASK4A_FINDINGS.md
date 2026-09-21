# TASK 4A — Findings

`REPODEX-T4A-GO-PACKAGE-MODULE-TOPOLOGY-V1`. IDs `T4A-Fxxx`, categories
`MODULE`/`PACKAGE`/`TEST_PACKAGE`/`MULTI_MODULE`/`AMBIGUITY`/`BUILD_CONSTRAINT`/
`IMPORT_STRUCTURE`/`FACT_PREREQUISITE`/`PERFORMANCE`.

## T4A-F001 — `MODULE`, INFO — multi-module discovery now general

`metadata::read_go_module` read only the repo-root `go.mod`. TASK 4A discovers
every repository-local `go.mod` (root + nested) via the scanner and the shared
`parse_module_directive`; each becomes a `go_module` entity and a content-digest
dependency. Hugo has **3** valid modules (root, `docs/`, `internal/warpc/genavif`)
plus **1** malformed `go.mod` (`internal/warpc/genwebp`, no `module` directive).
**Disposition: implemented.**

## T4A-F002 — `MODULE`, INFO — malformed `go.mod` still bounds its subtree

A `go.mod` with no readable `module` directive does not create a `GoModule`,
but it remains a module *boundary*: files under it are `NoKnownModule`, not
folded into the parent module. Hugo's `internal/warpc/genwebp` is such a
directory (no `.go` files there, so no practical effect in the corpus).
**Disposition: implemented + documented.**

## T4A-F003 — `TEST_PACKAGE`, INFO — external-test packages are distinct

`package foo_test` → `external_test` kind, separate `go_package` entity from
`foo`, and **no** fabricated canonical import path (§17). Hugo: **82**
external-test packages. **Disposition: implemented.**

## T4A-F004 — `PACKAGE`, INFO — `package main` is command_main

`package main` directories are `command_main`, directory-scoped, never assumed
to be repo/module roots. Hugo: **4** command packages. **Disposition:
implemented.**

## T4A-F005 — `AMBIGUITY`, INFO — same-name and conflicting packages preserved

Two directories with `package util`, and one directory with `package alpha` +
`package beta`, are preserved as distinct packages — never repaired. Conflicting
directory package names emit a diagnostic. **Disposition: implemented.**

## T4A-F006 — `BUILD_CONSTRAINT`, MEDIUM — `//go:build` not evaluated

Build tags are not evaluated (§20). Files that would be tag-separated into
different package names remain distinct structural packages; a single package
that is tag-gated keeps all its indexed files. Documented, not a defect.
**Disposition: documented.**

## T4A-F007 — `IMPORT_STRUCTURE`, INFO — `go.import.local_module` is multi-module

`go.import.local_module` previously matched only the repo-root module path. It
now matches the nearest (longest-prefix) repository-local module path, so an
import of a nested module's path resolves to that module's package directories.
Hugo link counts unchanged (`same_directory` 912, `local_module` 2,207,
`external` 3,866) — Hugo's imports all target the root module path, but the
machinery now handles nested-module imports. **Disposition: implemented.**

## T4A-F008 — `FACT_PREREQUISITE`, HIGH — Go persists no local-binding facts

§41 prerequisite check: the Go adapter emits **zero** `LocalBindingOccurrence`
facts and `Scope` carries no parameter/local names. There is no `BindingKind`
for Go `:=` short-var, `var`, `for`/`range` variables, parameters, or local
`const`. The §42 `helper := func(){}` shadowing case **cannot** be detected from
persisted facts today. **Disposition: prerequisite identified — Go needs a
bounded local-binding fact extension before plain-name candidates.** (See
`GO_PLAIN_NAME_CANDIDATES_NEED_BINDING_FACTS`.)

## T4A-F009 — `PACKAGE`, INFO — package-qualified calls are the dominant form

`member_selector` calls (39,389) dwarf `plain_name` (10,608) on Hugo — Go call
sites are overwhelmingly `pkg.Fn()` / `x.M()` selector form. The package-local
plain-name population (10,608; 3,826 with a unique same-package `func`) is the
bounded first step, but `pkg.Helper()` import-alias resolution is the larger
next population (§43). **Disposition: measured.**

## T4A-F010 — `PERFORMANCE`, INFO — topology adds ~45% to the link artifact

Hugo link artifact 6,206,857 → 9,028,096 B (+2,821,239, +45%) for +275 enriched
package + 3 module entities plus the metadata dependencies. Build ~1.2 s, RSS
~105 MB. **Disposition: measured.**
