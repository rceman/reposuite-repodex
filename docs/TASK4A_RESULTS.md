# TASK 4A — Go Package & Module Topology — Results

`REPODEX-T4A-GO-PACKAGE-MODULE-TOPOLOGY-V1`. Establishes the repository-level
Go package/module topology later Go call candidates need. No Go candidates
implemented (§1).

## 1–3. Base / SHAs / ABI

* Base: `7eaf7757a5dc6092016763a5a75fec103b37f8e3`.
* `LINK_RULE_ABI_VERSION` 2→3, `POLICY_VERSION_GO` 1→2, link fingerprint
  regenerated (`7101754c`→`5b91a304`). Analyzer ABI, candidate ABI, and
  `POLICY_VERSION_RUST` unchanged.

## 4–17. Model

* **GoModule**: `module_id` (root_dir), `manifest_path`, `root_dir`,
  `module_path`, `manifest_digest`, `rule go.module.go_mod` — one per valid
  `go.mod`, content-addressed, no absolute paths.
* **GoPackage**: `package_id` (`{module}@{dir}:{name}`), `directory`, `name`,
  `kind`, `module_id`, `module_rel_dir`, `import_path`, member `files`.
* **Kinds**: `ordinary` / `command_main` (`package main`) / `external_test`
  (`package foo_test`, no fabricated import path, no same-package visibility).
* **File role**: `source` / `test` (`_test.go`), separate from package identity.
* **Discovery**: every repo-local `go.mod` via the scanner (nested included);
  `module` via the shared bounded `parse_module_directive` (not regex).
* **Ownership**: nearest enclosing `go.mod` boundary; malformed `go.mod` bounds
  its subtree to `NoKnownModule`; files with no enclosing `go.mod` are
  `NoKnownModule` but still get a directory/package.
* **Ambiguity**: same-name dirs distinct; multi-name dirs preserved;
  `//go:build` not evaluated; generated files normal.

## 18–19. Fixtures

`tests/go_topology.rs` — 12 topology cases (root/nested/main/test+external/
conflict/no-module/nested-module/siblings/malformed-go.mod/build-tags/
determinism) + 5 update-vs-fresh cases, all pass. §32 cross-module isolation:
nested + sibling modules keep distinct identities.

## 21–26. Hugo corpus

```text
go.mod discovered     4   (3 valid modules + 1 malformed: internal/warpc/genwebp)
Go source files     912   (all carry a package clause -> 912 memberships)
packages            275   ordinary 189 / command_main 4 / external_test 82
known module        275   no-module 0
files with membership   912 / 912
```

Hugo is **multi-module** (§35): root `github.com/gohugoio/hugo`, `docs/` →
`github.com/gohugoio/hugoDocs`, `internal/warpc/genavif` → `gohugoio/hugo/…`.

## 27–33. Independent audit — `scripts/task4a_audit.py`

Re-derives module/package/membership per file from source `go.mod` + `package`
clauses (independent of production topology). All 912 files + 275 packages +
3 modules audited:

```text
CORRECT_MEMBERSHIP          912
CORRECT_PACKAGE             193   (ordinary + command_main)
CORRECT_EXTERNAL_TEST        82
CORRECT_MODULE                3
FALSE_MODULE                  0
WRONG_MODULE_MEMBERSHIP       0
WRONG_PACKAGE_MEMBERSHIP      0
PACKAGE_KIND_ERROR            0
MISSING_MODULE/PACKAGE        0
```

## 33. Go link counts before/after

```text
go.package.same_directory   912  ->  912
go.import.local_module     2207  -> 2207
go.import.external         3866  -> 3866
```

Identical — Hugo's imports all target the root module path; the multi-module
matching engages only when an import prefixes a nested module path. No
regression; validated by the audit and fixtures.

## 34–37. Projections (measure only — no candidates emitted)

Go `CallLikeForm` census on Hugo:

```text
member_selector    39,389   (pkg.Fn() / x.M() selector form — §43)
plain_name         10,608   (ordinary 9,815 / external_test 700 / command_main 93)
type_conversion       518
indirect              182
```

Package-local plain-name `func` projection (same-package same-name `func`
decls): **3,826 zero-or-one-unique → 3,826 `1`, 6,782 `0`, 0 `2+`** — Go's
no-duplicate-func rule means a same-package function name is unique or absent,
so a package-local rule yields at most one candidate.

## 38–39. Lexical-blocker sufficiency (§41)

**Not sufficient.** The Go adapter emits no `LocalBindingOccurrence`; `Scope`
carries no parameter/local names; `BindingKind` has no Go `:=`/`var`/`range`/
param/`const` variants. The §42 `helper := func(){}` case is undetectable from
persisted facts today. → `GO_PLAIN_NAME_CANDIDATES_NEED_BINDING_FACTS`.

## 40–46. Perf / determinism / invalidation

Link artifact 6,206,857 → 9,028,096 B (+45%); build ~1.2 s; RSS ~105 MB.
Repeated + cross-root (`/tmp/hugo-repo` vs `/tmp/hugo-repo2`) → identical
link digest `62c00200`. Update-vs-fresh: 5 go.mod/package cases equal.
Stale pre-T4A artifact → `FingerprintMismatch`. Rust regressions:
`task3e2_audit` PASS, crossfile_links 42/42.

## 47–50. Findings / status

`docs/TASK4A_FINDINGS.md` T4A-F001…F010; T4A-F008 is the prerequisite. Gates:
fmt/check/test(454)/clippy/release PASS.

```text
TASK4A_COMPLETE
GO_PLAIN_NAME_CANDIDATES_NEED_BINDING_FACTS
```

## 51–56. Status & next task

* **Rust**: bounded foundation frozen — untouched.
* **Go**: package/module topology ready; plain-name candidates need a Go
  local-binding fact extension first.
* **Next task (one)**: **TASK 4B — Go local-binding facts**: emit
  `LocalBindingOccurrence` for Go `:=` short-var, `var`, function/method
  parameters, `for`/`range` variables, `if`/`switch`/`select` init bindings,
  and local `const`, with visibility ranges — the minimal fact extension that
  unblocks `GO_PLAIN_NAME_CANDIDATES` without a semantic pass. Not started.
* No Go candidate task started; no further Rust expansion; nothing pushed.
