# TASK 3C lexical-shadowing correction — status report

Correction prompt: `REPODEX-T3C-LEXICAL-SHADOW-CORRECTION-V1`
Source task: `REPODEX-T3C-RUST-LOCAL-CALL-CANDIDATES-V1`

```text
STATUS: TASK3C_CORRECTION_BLOCKED_BY_FACT_MODEL
```

**A candidate is not a resolved call target.** This document reports a
correction that could not be implemented because the persisted normalized fact
model does not expose the syntax facts the correction requires.

## 1. What was asked

For a Rust `plain_name` call `X()`, before walking outward to an eligible
`Function` declaration, detect whether a *closer* lexical scope binds `X` as a
non-function value (`let`, parameter, pattern binding, `const`, `static`,
closure/`for`/`match`/`if-let` binding, import). If such a closer binding owns
the name, the outer `fn X` must **not** be returned — emit `NoCandidate` with a
`shadowed_by_unsupported_local_binding`-style reason instead.

## 2. The defect, demonstrated

Current committed behavior (`536b63e`). For:

```rust
fn helper() {}
fn run() { let helper = || {}; helper(); }      // closer `let` binding
fn runp(helper: fn()) { helper(); }            // closer `fn` parameter
fn outer() { { let helper = || {}; helper(); } // inner-block `let`
             helper(); }                        // not shadowed
```

the rule emits:

```text
run::helper()   -> single_candidate(helper@0)   WRONG — `let helper` shadows it
runp::helper()  -> single_candidate(helper@0)   WRONG — param `helper` shadows it
outer::helper() -> single_candidate(helper@0)   WRONG — inner `let helper` shadows it
outer::helper() -> single_candidate(helper@0)   correct — the outer call is unshadowed
```

Three of the four calls return an outer `fn` candidate that a closer local
binding owns. This is exactly the unsafe candidate the correction exists to
prevent.

## 3. Why the persisted facts are insufficient

The correction is confined to persisted normalized facts (`FileAnalysis`:
`scopes`, `declarations`, `imports`, `references`, `calls`, `diagnostics`,
`recovery_regions`). The Rust adapter emits **no fact** for the constructs the
correction must detect:

| binding construct | persisted fact? | consequence |
| --- | --- | --- |
| `let X = ...` | **none** — `let_declaration` is unhandled | `X` is invisible |
| `fn f(X: T)` parameter | **none** — only `self` is noticed (for the `Receiver` flag) | `X` is invisible |
| closure `\|X\|` parameter | **none** — the closure scope exists but params are not recorded | `X` is invisible |
| `for X in ...` | **none** | `X` is invisible |
| `match`/pattern binding `X` | **none** | `X` is invisible |
| `if let`/`while let` `X` | **none** | `X` is invisible |
| local `const X` / `static X` | `Declaration{kind: constant}` at the enclosing scope | **representable** |
| `use ... X` import | `ImportOccurrence` item leaf/alias at its scope | **representable** |

Every mandatory regression fixture (§6.1 `let`, §6.2 parameter, §6.5 inner-block
`let`) depends on a binding kind that is **not represented**. There is no
`ReferenceOccurrence` kind for a general name binding either — `references` only
holds structural references (`impl` target, base class, decorator, …).

### The deeper problem: blocks are not scopes

Even a hypothetical binding fact is not enough for §6.5. In:

```rust
fn outer() {
    { let helper = || {}; helper(); }
    helper();
}
```

the inner `{ }` block is **not** a scope in the model — both `helper()` calls
carry `scope_id == outer`. The `let helper`'s live extent ends at the inner
block's close, but that boundary is not persisted anywhere. A naive positional
rule ("a same-scope binding before the call shadows it") would wrongly suppress
the *outer* call too. Correctly separating the two requires knowing the
binding's enclosing block — which is absent because blocks are not scopes.

## 4. The smallest normalized-fact extension

To satisfy the correction the model needs a **local-binding fact** that the
Rust adapter does not currently emit. The smallest sufficient shape is a new
fact vector on `FileAnalysis`, e.g.

```text
LocalBinding {
    scope_id,        // enclosing callable scope (fn/method/closure)
    name,            // the bound identifier
    name_range,
    statement_range, // the binding statement/pattern
    live_range,      // enclosing block range — the binding's lexical extent
    kind,            // let | param | closure_param | for | match | if_let | while_let | const | static
}
```

`live_range` (the enclosing `block` range) is what lets a `let` inside `{ ... }`
shadow the inner call but not the same-`scope_id` outer call — it substitutes
for making `block` a full `ScopeKind`. (The larger alternative — adding a
`ScopeKind::Block` — records the same information at far greater cost.)

This is an **extraction-layer change**, owned by TASK 1/TASK 2, not by the
candidate layer. Landing it requires:

* emitting `LocalBinding` records from `src/parser/rust.rs` for `let`
  patterns, function/closure parameters, and `for`/`match`/`if-let`/`while-let`
  patterns (pattern destructuring means one `let` can bind many names);
* a `FileAnalysis`/`schema_version` and analyzer-fingerprint change;
* re-blessing every `fixtures/expected/**` canonical-facts file and updating the
  canonical-completeness invariant;
* invalidating every TASK 3A snapshot and, transitively, TASK 3B link artifacts
  (the fingerprint no longer matches).

That blast radius is precisely why §4 of the correction prompt classifies this
as a *blocking architectural dependency* rather than a candidate-layer edit.
Introducing an ad-hoc source scan (regex/token reparse) inside the candidate
rule is explicitly forbidden, so the correction stops here rather than fake it.

## 5. Real-corpus blocker evidence

On the pinned tokio corpus (5,345 in-scope `plain_name` calls, 1,396 that
emitted a `single`/`multiple` candidate), two measurements:

**Representable blockers** (from persisted facts — `const`/`static` decls and
`use` imports):

```text
calls with a representable blocker in the lexical chain:  1,033
    const/static blocker:                                    0
    same-name `use` import binding:                      1,033
calls where the rule emitted an outer `fn` despite an
inner representable blocker:                                0
```

A same-named `use` import never coincides with a same-named `fn` in the same
scope, so the rule already returns `no_candidate` — the representable blockers
are a **real-corpus no-op**.

**Unrepresentable blockers** (`let`/parameter/pattern — not in the facts, so a
source-scan heuristic was used *for measurement only*): among the 1,396
emitted-candidate calls, essentially **zero** are clearly unsafe. The dominant
same-name patterns are legitimately *unshadowed*:

```rust
fn rt() -> Runtime { .. }
fn bench() {
    let rt = rt();          // `rt()` is inside the let initializer — the binding
                            // is not live yet, so it correctly resolves to fn rt
    { let rt = rt(); .. }   // `let rt` lives only inside `{ }`
    rt();                   // after the block closes — correctly resolves to fn rt
}
```

`benches/fs.rs`, `benches/copy.rs`, `rt_current_thread.rs`,
`rt_multi_threaded.rs` and `tokio-util/tests/task_join_map.rs` are all this
shape. The one call that survives a "binding completed before the call" filter
is the `{ let rt = rt(); .. } rt()` inner-block form — and that call is *not*
shadowed, because the inner block closed. So on this corpus the defect is
largely **latent**: it is provably real on the fixtures, but the corpus's
same-name bindings happen to be initializer/inner-block forms that are already
unshadowed.

This is itself the point: distinguishing "the `rt()` inside `let rt = rt()`"
(unshadowed) from "`let rt;` then `rt()`" (shadowed) — and "the `{ let rt }`
inner-block shadow that closes before `rt()`" — requires knowing each binding's
live extent, i.e. exactly the binding fact + block range the model lacks. Even
*measuring* the defect precisely is blocked on the same missing facts.

## 6. Why no partial fix was shipped

`const`/`static` and `use` imports *are* representable, so a partial correction
that suppresses an outer `fn` when one of those binds the name is technically
implementable. It was deliberately **not** shipped because:

* it satisfies none of the mandatory fixtures — §6.1, §6.2 and §6.5 all need
  `let`/parameter facts that do not exist;
* on the real corpus it changes **zero** outcomes (§5), so it cannot demonstrate
  the defect it claims to fix;
* it would still leak the dominant `let`/parameter shadows while a bumped rule
  ABI (§13) implied the correction had landed — a false sense of correctness.

Honest `BLOCKED` is strictly preferable to a partially-corrected rule that still
emits the unsafe candidates the correction targets.

## 7. Audit-methodology update (for when facts exist)

The source-driven audit (`scripts/task3c_audit.py`) should, per call, walk the
lexical chain innermost→outermost and stop at the **first** name-bearing
construct — a `Function` (candidate), a `LocalBinding` (blocker), or a `use`
import leaf/alias (suppressor). A call whose innermost match is a binding or
import expects **zero** function candidates. Until `LocalBinding` facts exist,
that ground truth cannot be computed, so the audit cannot yet measure
`SHADOWED_OUTER_CANDIDATE` — it is reported as `0 prevented` only because the
blockers are invisible, not because the rule is correct.

## 8. Disposition of T3C-F005

Re-evaluated and **escalated from a documented bound to an open correctness
defect**:

```text
original defect    closer local value bindings are not modeled, so an outer
                   free `fn` is returned as a candidate when a nearer let/param/
                   pattern owns the written name
reproduction       §2 — run/runp/inner-outer all emit single_candidate(helper@0)
fix                BLOCKED — requires a LocalBinding fact + block live-range,
                   an extraction-layer (TASK 1/2) extension
regression evidence cannot be added until the facts exist (fixtures 6.1/6.2/6.5
                   are unsatisfiable)
residual           let, function param, closure param, for, match, if-let,
                   while-let — all unrepresented; const/static + imports are
                   representable but no-op on the corpus
```

The finding is preserved, not erased, in `TASK3C_FINDINGS.md`.

## 9. Recommended next bounded task

`REPODEX`: a normalized **local-binding fact** for Rust (emit `LocalBinding`
records for `let`/parameter/`for`/`match`/`if-let`/`while-let`/`const`/`static`
and closure parameters, each carrying its `scope_id`, `name` and enclosing-block
`live_range`). That single extraction-layer extension unblocks this correction
and is independently useful for the future repository map. Once it exists,
re-run this correction exactly as specified.

## 10. What was changed

No candidate-rule code changed — the persisted facts cannot express the
required blockers, and the task forbids source re-scanning. Only documentation
was updated to record the blocked determination and the corrected `T3C-F005`
disposition. The candidate rule ABI (`candidate_rule_abi_version = 1`) is
unchanged: output semantics did not change, so a stale-artifact bump would be
dishonest.
