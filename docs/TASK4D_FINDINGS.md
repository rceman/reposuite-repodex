# TASK 4D — Findings

Go imported package-qualified call candidates (`pkg.Func()`). `T4D-F001`….

## T4D-F001 — IMPORTED_PACKAGE — call fact model is sufficient

The `member_selector` `CallLikeOccurrence` carries `callee_written` (the full
`operand.field` selector text) plus `callee_range`/`type_arguments`. A direct
`pkg.Func()` is recovered by a bounded split of `callee_written` on the last
`.` — a `field_identifier` never contains `.`, so the operand text is the root;
it is a package selector only when that operand is a single identifier. No
Tree-sitter reparsing or raw-source tokenization is needed. `auth.Client.New`
(operand `auth.Client`) is rejected as a non-direct shape.

## T4D-F002 — IMPORTED_PACKAGE — structural link is the resolution authority

The import→package binding is read from the persisted TASK 4A `import_path`
link (`go.import.local_module` → `go_package` entity), never re-resolved from
the path string. An ambiguous link may expose several package targets — all are
evaluated (§14). `go.import.external` → `out_of_scope(external_import)`.

## T4D-F003 — ALIAS — local name is the package clause, not the path basename

For `import "…/client/v2"` whose clause is `package api`, the selector root is
`api` — read from the `go_package` entity key, not the path's last segment.
Hugo has **18** repo-local imports where package name ≠ basename; all resolve
through the true clause name (0 misbindings). Aliases (`import a "…"`) bind `a`.

## T4D-F004 — SHADOWING — local bindings own the selector root

A covering same-name `LocalBindingOccurrence` at the selector-root position
makes `auth.F()` a method call on the local value, not a package call →
`NoCandidate(import_root_shadowed_by_local_binding)`. Hugo: 34 calls.

## T4D-F005 — EXPORT — lexical export rule enforced

An unexported terminal (`auth.validate()`) is `NoCandidate(imported_function_not_exported)`
— Go's `first char is uppercase` rule is lexical, applied to the written
terminal before any package lookup.

## T4D-F006 — METHOD_SEPARATION — receiver selectors stay out of scope

A selector whose root is not a file import binding (`obj.Method()`, package-level
vars, etc.) is `out_of_scope(selector_root_not_an_import)` — Hugo: 22,129.
Multi-hop `a.b.C()` is `out_of_scope(not_a_direct_package_selector)` — 5,133.
Neither is ever a package-function candidate.

## T4D-F007 — EXTERNAL_IMPORT — external packages are never resolved

`import "bytes"` + `bytes.X()` → `out_of_scope(external_import)`. The root is
detected via the last path segment *only* to classify the call as import-rooted
(it never yields a target); the package name is unknowable without dependency
resolution. Hugo: 9,478.

## T4D-F008 — PACKAGE_NAMESPACE — same-name non-function is ambiguous

An imported package exposing a same-name non-function declaration (build-tag /
variant coexistence) → `NoCandidate(imported_package_namespace_ambiguous)`.
Hugo: 174.

## T4D-F009 — FALSE_CANDIDATE — audit: zero errors over all 40,234 calls

Independent audit re-derives every member_selector outcome from persisted facts
+ links + bindings without production code. FALSE_IMPORTED_PACKAGE_FUNCTION=0,
WRONG_TARGET=0, IMPORT_ROOT_SHADOWING=0, METHOD_MISCLASSIFIED=0,
CROSS_MODULE_LEAK=0, MISSING_CANDIDATE=0.

## T4D-F010 — ARTIFACT — candidate artifact grows to 23.1 MB

52,379 records; +1,106,825 B over TASK 4C (the member_selector records now carry
real outcomes/provenance instead of a bare out_of_scope). Build ~2.1 s.
