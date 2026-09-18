# TASK 3B findings

Finding ids are stable. Severity is one of `BLOCKER`, `HIGH`, `MEDIUM`, `LOW`,
`INFO`.

The single most important result is stated first because it is the acceptance
criterion that matters most:

```text
FALSE_EXACT defects in required rules: 0
```

Across 12,296 independently adjudicated relationships on four real repositories
(see `TASK3B_RESULTS.md` § real-repository audit), no rule produced an exact link
that the independent checker could not reproduce.

---

## T3B-F001 — `from . import X` and `from .X import ...` render identically

* **Category** PYTHON_IMPORT
* **Severity** MEDIUM
* **Status** FIXED

**Observation.** `relative_written(levels, module)` renders the target of
`from . import name` as `.name` and the target of `from .name import x` as
`.name` too. Both are faithful to the written source, but a reader of the link
artifact could not tell which import form produced a record, and an auditor
could not reproduce the candidate set without guessing.

This surfaced during the real-repository audit: the independent checker flagged
`django/contrib/auth/apps.py` `from . import get_user_model` as a `FALSE_EXACT`,
because it assumed the `from .name import ...` reading and looked for a module
`django.contrib.auth.get_user_model` that does not exist. The RepoDex result was
correct — `from . import get_user_model` legitimately binds the package attribute
`get_user_model` declared in `django/contrib/auth/__init__.py` — but the artifact
did not say so.

**Disposition.** The raw import form is now carried as a plain syntax fact in the
provenance evidence:

```text
written_module=<none>        from . import name
written_module=<name>        from .name import x
```

This is a syntax fact, not a verdict, so it does not make the artifact
self-certifying. `docs/CROSS_FILE_LINKING.md` documents the collision.

---

## T3B-F002 — candidate ordering must not sort by a derived entity hash

* **Category** DETERMINISM
* **Severity** LOW
* **Status** FIXED

**Observation.** Candidate lists were sorted by the rendered target string, which
embeds the derived structural entity id (`ent-<16 hex>`). The order was
deterministic, but arbitrary-looking: for two Go packages in one directory the
`bar_test` group sorted before `bar` purely because its entity hash did.

**Disposition.** Candidates now sort by a meaningful canonical key — relative
path, declaration id, structural kind and structural key — via a single shared
`sort_candidates` helper. All four corpora still produce one distinct digest
across repeated builds and different output directories, and the entity-id
component is no longer part of ordering.

---

## T3B-F003 — `cfg`-gated `mod` declarations are treated structurally

* **Category** RUST_MODULE
* **Severity** INFO
* **Status** ACCEPTED

**Observation.** `tokio/src/loom/std/mod.rs` declares

```rust
#[cfg(all(feature = "parking_lot", not(miri)))]
mod parking_lot;
```

RepoDex records the `mod` declaration and links `crate::loom::std::parking_lot::*`
to `tokio/src/loom/std/parking_lot.rs` regardless of the attribute. The audit
confirmed these are the correct structural targets.

**Disposition.** Accepted and documented. The task requires that `cfg` is not
evaluated, and structural treatment is the honest consequence: RepoDex describes
the module tree written in the source, not a particular feature configuration.
The same policy makes package membership in Go structural over indexed files
rather than specific to a build tag set.

---

## T3B-F004 — re-exports are a bounded miss, not a false link

* **Category** RUST_MODULE / PYTHON_IMPORT
* **Severity** INFO
* **Status** ACCEPTED

**Observation.** Two real-repository patterns are deliberately not resolved:

```rust
// tokio-stream/src/empty.rs
use crate::Stream;          // `Stream` is re-exported by tokio-stream/src/lib.rs,
                            // not declared there
```

```python
# django/core/checks/async_checks.py
from . import Error, Tags, register   # re-exported into django/core/checks/__init__.py
                                      # by `from .messages import Error`
```

All 1,324 unresolved Rust relationships in tokio and 22 of django's 24 unresolved
relationships are this pattern.

**Disposition.** Accepted. The task explicitly excludes glob semantics and
re-exports from `rust.use.crate_path`, and the Python package-attribute rule
matches *declarations* in the package's own files, not names imported into them.
Each case is reported as `Unresolved` with an explicit reason naming the module
and the missing declaration, so the miss is visible and bounded rather than
hidden. Following re-exports would require a name-resolution layer that belongs
to a later task.

---

## T3B-F005 — `rust.use.crate_path` ambiguity is nearly unreachable in valid Rust

* **Category** AMBIGUITY
* **Severity** INFO
* **Status** ACCEPTED

**Observation.** Across tokio (1,335 crate-path relationships) the rule produced
zero `Ambiguous` outcomes. Valid Rust cannot declare two items with the same name
in one module, so the ambiguity branch is reachable only through structurally
duplicated declarations, which is what the curated fixture
`fixtures/crossfile/rust/src/dupmod.rs` provides:

```rust
pub fn dup() {}
pub fn dup() {}
```

**Disposition.** Accepted. The branch is real, exercised by the fixture, and
proved by `rust_fixture_produces_the_complete_expected_relationship_set` and
`ambiguous_relationships_keep_every_candidate`. Its rarity in valid code is a
property of Rust, not a gap in the rule.

---

## T3B-F006 — `Ambiguous` can carry exactly one candidate

* **Category** AMBIGUITY
* **Severity** INFO
* **Status** ACCEPTED

**Observation.** `python.absolute_import.local_candidate` reports `Ambiguous`
even when exactly one repository module matches, because `sys.path`, editable
installs and namespace packages can shadow it. In django, 8,034 absolute imports
produce this outcome.

**Disposition.** Accepted and documented explicitly, because "Ambiguous with one
candidate" is surprising. The outcome means *not uniquely determined*, not
*more than one*. Collapsing a single-candidate absolute import to `Exact` would
be exactly the unjustified exactness the task forbids. The evidence entry
`absolute_resolution_depends_on_sys_path=true` makes the reason visible in the
artifact.

---

## T3B-F007 — a bare namespace `use` matches namespace declarations

* **Category** PHP_NAMESPACE
* **Severity** INFO
* **Status** ACCEPTED

**Observation.**

```php
// laravel_framework src/Illuminate/Testing/Concerns/TestDatabases.php
use Illuminate\Foundation\Testing;
```

The written name matches 15 syntactic namespace declarations, so
`php.use.qualified_name` reports `Ambiguous` with 15 candidates. Each candidate
carries `declaration_kind: "namespace"`, so an auditor can see immediately that
these are namespace declarations rather than class declarations.

**Disposition.** Accepted. A namespace declaration *is* a syntactic qualified
declaration, so matching it is consistent with the documented rule, and
`Ambiguous` is the honest outcome for a name that denotes a namespace prefix
rather than a single class. Reporting `Exact` or `Unresolved` here would be
worse. Only bare namespace imports are affected: a class name like
`App\Service\Foo` matches only class-like declarations.

---

## T3B-F008 — only class-like declarations become namespace members

* **Category** PHP_NAMESPACE
* **Severity** INFO
* **Status** ACCEPTED

**Observation.** `fixtures/crossfile/php/src/Service/Helper.php` declares a
function and a constant in `App\Service`, and produces exactly one
`php.namespace.declaration` relationship (for the namespace itself). The function
and constant are not recorded as namespace members.

**Disposition.** Accepted. Namespace membership is restricted to classes,
interfaces, traits and enums, because those are what a qualified `use` can
denote. Function and constant imports are handled by the separate
`php.use.non_class_import` rule, which never links them to class declarations.

---

## T3B-F009 — a Go directory with two package groups is ambiguous by construction

* **Category** GO_PACKAGE
* **Severity** INFO
* **Status** ACCEPTED

**Observation.** In hugo, 1,138 of 2,207 local-module imports are `Ambiguous`,
because the target directory contains both a `foo` and a `foo_test` package.
`package foo` and `package foo_test` are never merged, as required.

**Disposition.** Accepted. A local import into such a directory genuinely has two
structural candidates under a directory-based package model. Collapsing to the
non-test package would be an unjustified exactness.

---

## T3B-F010 — `links verify` cannot check metadata without the repository

* **Category** PROVENANCE
* **Severity** LOW
* **Status** DOCUMENTED

**Observation.** `links verify <links-dir> --snapshot <snapshot-dir>` validates
the artifact against the snapshot but cannot re-check a `go.mod` content digest
without the checkout. The report distinguishes the two cases with an explicit
`metadata_checked` field, and the CLI prints a note when dependencies were not
re-checked.

**Disposition.** Documented. Passing `--repository <repo>` re-checks every
metadata dependency. The distinction is reported rather than silently dropped.

---

## T3B-F011 — the link artifact is a small derived index, not a fact copy

* **Category** ARTIFACT
* **Severity** INFO
* **Status** MEASURED

**Observation.** Measured link artifact size is 9.7%–16.8% of the TASK 3A
snapshot it derives from, because every relationship references the snapshot
through a locator instead of embedding normalized facts. A regression test
asserts that no serialized link contains `scopes` or `recovery_regions`.

**Disposition.** Accepted. The task forbids a second complete serialized copy of
all file facts, and the measurement confirms there is none.

---

## T3B-F012 — full relationship rebuild is cheap enough that incremental mutation is unnecessary

* **Category** PERFORMANCE
* **Severity** INFO
* **Status** MEASURED

**Observation.** A full link build costs 0.405×–0.558× a fresh TASK 3A snapshot
build of the same repository (0.76 s–2.96 s wall against 1.86 s–6.59 s).

**Disposition.** TASK 3B rebuilds the whole derived artifact from the whole
snapshot and performs no incremental relationship patching. The measurement says
simplicity is the right trade for now. Revisit only if a later task makes link
derivation substantially more expensive.

---

## T3B-F013 — verification proves consistency, not semantic correctness

* **Category** PROVENANCE
* **Severity** INFO
* **Status** DOCUMENTED

**Observation.** `links verify` proves that an artifact is internally consistent
with a recorded snapshot and metadata. It cannot prove that a relationship is
semantically correct at runtime.

**Disposition.** Stated in the CLI output, in `docs/CROSS_FILE_LINKING.md`, in
the module documentation and in the verification report, because a reader could
otherwise mistake a green verification for semantic validation.

---

## T3B-F014 — a link artifact cannot be silently reused after a metadata change

* **Category** ARTIFACT / DETERMINISM
* **Severity** INFO
* **Status** TESTED

**Observation.** Creating a `go.mod` where none existed, or editing one, changes
the classification of Go imports. Absence is therefore recorded as a dependency
with the digest of empty bytes and `present: false`, so creating the file
invalidates a previous artifact.

**Disposition.** Tested by `adding_a_go_mod_invalidates_a_previous_link_artifact`,
`missing_metadata_never_produces_a_local_go_import` and
`malformed_go_mod_never_produces_a_local_go_import`.
