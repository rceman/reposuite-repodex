# Rust local call candidates (TASK 3C)

TASK 3C adds the first deliberately narrow call-candidate layer on top of the
TASK 3A snapshot and the TASK 3B structural-link artifacts:

```text
Rust plain-name call-like occurrence
    -> lexical/module-local syntactic candidate search
    -> zero / one / many candidate declarations
    -> provenance-bearing candidate record
```

RepoDex implements **one bounded candidate-generation rule**. It does not
resolve calls, and this document never claims it does.

## The central rule

```text
A candidate is not a resolved call target.
```

A candidate is evidence that a declaration *could be relevant* under the
bounded syntactic rule. It is never proof that the call dispatches to that
declaration. Rust name resolution — imports, re-exports, visibility, generics,
traits, method dispatch, `#[cfg]` — is richer than anything this layer models.

## Why a candidate outcome is not a structural outcome

TASK 3B's `Exact` / `Ambiguous` / `Unresolved` / `OutOfScope` vocabulary fits
bounded structural relationships, where "exactly one structural candidate"
*is* the answer the rule exists to give. That vocabulary does not fit call
candidates:

```text
one candidate    must not become Exact
one candidate    must not become Ambiguous-with-one-candidate
```

A single lexical match is still only a candidate — it is not dispatch, not
type-checking, not resolution. So the candidate layer uses its own cardinality:

| Cardinality | Meaning |
| --- | --- |
| `no_candidate` | The rule searched and found no eligible declaration. It does **not** mean the call has no runtime target. |
| `single_candidate` | The rule found exactly one eligible declaration. This is **not** a resolved call target. |
| `multiple_candidates` | The rule found more than one eligible declaration; every one is kept, never collapsed. |
| `out_of_scope` | The call shape is deliberately outside this rule. It is recorded, never silently dropped. |

The cardinality is enforced by a dedicated `CandidateOutcome` type. It has no
`Exact` variant and no "ambiguity" concept, so the meaning cannot be smuggled
back in through a reused enum.

## What calls are in scope

A call-like occurrence is in scope only when **all** of these hold:

```text
language             == Rust
syntactic form       == plain_name
dynamic_callee       == false
callee_written       is exactly one source-written identifier
                       (a raw identifier `r#name` also qualifies)
```

Examples in scope: `helper();`, `process(value);`, `foo();`.

Examples explicitly **out of scope** (each produces an `out_of_scope` record
naming the reason):

```text
crate::foo()            qualified_path
self::foo()             qualified_path
obj.foo()               member_selector
Type::foo()             static_scoped
(foo)()                 indirect
factory()()             the outer call is indirect
Foo(value)              a type-conversion/constructor-shaped form
my_macro!()             macro_invocation
calls inside macro token trees   (never extracted; see below)
```

## What declarations are candidates

Candidate declarations are only source-written Rust `function` declarations —
the normalized `DeclarationKind::Function`, which covers free functions and
functions nested inside a function body. The rule deliberately does **not**
include:

```text
method, associated function, trait method
closure, local variable, static, const
tuple-struct constructor, enum-variant constructor
macro, imported symbol, re-exported symbol
```

So `let helper = || {}; helper();` produces `no_candidate`, and `User(1);` for
`struct User(u64);` produces `no_candidate` — under this rule there is no
eligible `function` declaration, even though a runtime callable may exist.

## The lexical/module selection rule

For each in-scope call:

1. Start at the call's containing scope and walk the parent chain until the
   first `module` or `file` scope, inclusive. This is the **bounded lexical
   chain** — `function` and `closure` scopes are searched, `impl` is
   transparent, and the chain stops at the innermost enclosing module.
2. At each level, innermost first, collect `function` declarations whose
   `scope_id` is that level and whose `name` equals the written callee.
3. The **first** level that has a match supplies the whole candidate set.
4. If the chain is exhausted with no match, the outcome is `no_candidate`.

Consequences:

* **Shadowing.** A function nested inside a function is at level 0, so it
  shadows a same-named module-level function. `fn outer() { fn helper(){}; helper(); }`
  yields the nested `helper`, not the module one.
* **Module isolation.** The chain stops at the innermost module, so a
  same-named function in a sibling or parent module is never borrowed.
  `mod b { fn helper(){} }` is not a candidate for a call inside `mod a`.
* **Nearest-scope only.** Once a level matches, outer levels are not searched,
  so a module-level `helper` cannot also be returned for a call that already
  matched a nested `helper`.

Rust's real rules are richer (visibility, `use`, trait dispatch, generics).
This policy is a documented bound, not compiler equivalence.

## Imports and re-exports are out of scope

TASK 3B carries a known re-export limitation (T3B-F004). TASK 3C does not solve
it. `use crate::other::helper;` followed by `helper();`, and `pub use ...`,
produce `no_candidate` under this rule — the imported symbol is not a
source-written `function` declaration in the lexical chain. That is a bounded
miss, **not** proof that no real target exists. On the tokio corpus, roughly a
quarter of `no_candidate` calls reference a `use`-imported name.

## The macro-token-tree boundary

Calls written inside a macro token tree (`my_macro! { helper() }`) are never
extracted by TASK 1 (TASK 2 F002), so they are `NOT_AVAILABLE_TO_TASK3C` rather
than candidate misses. The audit counts call-shaped tokens inside macro bodies
separately; they are never fabricated.

## Artifact and dependencies

```text
<candidates-dir>/
  manifest.json             versions, dependencies, counts, digest, rule registry
  call_candidates.jsonl     one canonical candidate record per call
```

The manifest records the exact upstream identity the artifact was derived
from: the TASK 3A snapshot digest and the TASK 3B link digest (plus their
schema/fingerprint/ABI fields). Rebuilding either upstream artifact invalidates
the candidate artifact, and `candidates verify` rejects a stale dependency.

Candidate records reference snapshot locators (a file path plus a file-local
call id) and declaration locators (a file path plus a file-local declaration
id); they never copy `FileAnalysis`, normalized facts, source, or Tree-sitter
trees.

## Provenance

Each record carries its `rule_id`, the call locator, the written callee, the
language, the outcome, and a provenance block holding the readable lexical
scope path, the number of levels searched, the enclosing module that bounded
the search, and rule-specific evidence. The registry documents the rule's
conditions, assumptions and known exclusions once, in the manifest.

## Determinism

The artifact is a pure function of the snapshot bytes and the link-artifact
bytes: repeated builds, different output directories and relocated checkout
roots all produce identical canonical bytes and digest. Candidate order and
record order are deterministic.

## Verification proves consistency, not resolution

`candidates verify` checks the manifest, the schema and rule ABI, both upstream
dependencies, the record digest, that every call locator and candidate
declaration locator exists, that candidate kinds are `function`, that candidate
lists are sorted and duplicate-free, and that each record's cardinality matches
its candidate count. It proves internal consistency — it does not prove Rust
semantic resolution.
