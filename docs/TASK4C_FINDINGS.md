# TASK 4C — Findings

Go package-local plain-name call candidates. Identifiers `T4C-F001`….

## T4C-F001 — PACKAGE_CANDIDATE — package identity is the lookup boundary

`go.call.package_local_function_candidate` resolves a `plain_name` call only
against `function` declarations inside the call's **own** TASK 4A `go_package`
entity (module + directory + package clause + kind). It never does a
repository-wide same-name lookup, so `foo`/`foo_test`, sibling directories, and
nested modules stay isolated.

## T4C-F002 — LEXICAL_SHADOW — persisted bindings are the authoritative blockers

A covering same-name `LocalBindingOccurrence` (`binding.covers(call_pos)`) makes
a call `NoCandidate(shadowed_by_local_binding)` before any package lookup. On
Hugo this suppressed exactly the 1,556 calls TASK 4B measured as covered — no
more, no fewer. Visibility ranges are authoritative; no scope reconstruction
from raw source.

## T4C-F003 — IMPORT_SHADOW — determinable import names block; external names are a documented gap

An import blocks a `plain_name` call only when its file-block local name is
*determinable*: an explicit alias (`import h "..."`), or a repository-local
import resolved to its package name through the TASK 4A `import_path` → package
map. A normal **external** import's package name is not in the persisted facts
and is deliberately not guessed from the path's last segment — a documented
under-capture limitation (an external package could shadow a same-name package
function, but RepoDex cannot see it without dependency resolution).

## T4C-F004 — PACKAGE_NAMESPACE — dot imports poison the whole file block

A dot import (`import . "pkg"`) can inject arbitrary external names into the
file block. Since RepoDex cannot enumerate them, **every** `plain_name` call in
a file containing a dot import is `NoCandidate(dot_import_namespace_uncertain)`.
Hugo: 22 calls.

## T4C-F005 — PACKAGE_NAMESPACE — same-name non-function declaration is ambiguous

When a package contains a same-name `var`/`const`/`type`/etc. alongside a
`function` (possible because build tags are not evaluated and source variants
coexist), the call is `NoCandidate(package_namespace_ambiguous)` rather than a
false function candidate. Hugo: 413 calls, of which 211 had a real function
suppressed.

## T4C-F006 — AMBIGUITY — multiple same-name functions are preserved

Two or more source-written same-name `function` declarations in one package
yield `MultipleCandidates` with all locators — never collapsed. Hugo: 3 calls
(unevaluated build-tag variants). Zero was *not* hardcoded.

## T4C-F007 — FALSE_CANDIDATE — audit confirms zero false candidates

The independent audit (`scripts/task4c_audit.py`) re-derives every plain-name
call's expected outcome from persisted facts + topology without production
candidate code. Result: FALSE_PACKAGE_FUNCTION_CANDIDATE=0,
WRONG_PACKAGE_FUNCTION_TARGET=0, CROSS_PACKAGE_LEAK=0, LOCAL_SHADOWING_ERROR=0,
IMPORT_SHADOWING_ERROR=0, MISSING_PACKAGE_FUNCTION_CANDIDATE=0.

## T4C-F008 — EXTERNAL_TEST — `foo_test` never borrows `foo`

External-test packages are a distinct `go_package` identity; their 746
plain-name calls resolve only within `foo_test`. Ordinary-package leak count = 0.

## T4C-F009 — ARTIFACT — candidate artifact +22.1 MB on Hugo

The combined Rust+Go candidate artifact is 22,064,278 B (52,379 records:
3,918 single + 3 multiple + 7,213 no_candidate + 41,245 out_of_scope). Carried
forward with the §47 snapshot-size design debt (snapshot +60% in TASK 4B); no
storage optimization in this task.

## T4C-F010 — PERFORMANCE — derive ~156 ms, verify ~800 ms

Hugo candidate build: ~1.9 s total (load 760 ms, derive 156 ms, serialize 53 ms,
verify 800 ms). The rule is O(calls × package-files) over persisted facts.
