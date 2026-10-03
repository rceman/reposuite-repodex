# PHP Bounded Receiver-Type Evidence V1 — contract

Scope: explicit, source-written type evidence only. Facts say "the source
writes evidence that the receiver MAY carry type T" — never that the runtime
object has type T. Every method relationship remains `CANDIDATE`.

## Evidence sources (ReceiverEvidenceKind)

| kind | shape | scope |
|---|---|---|
| parameter_type_hint | `function f(Service $x)` / promoted param `$x` | callable scope |
| property_type_hint | `private Service $s;` / promoted `private Cache $c` | class scope |
| local_literal_new | `$x = new Foo(...)` | enclosing scope (callable) |
| property_literal_new | `$this->p = new Foo(...)` | enclosing scope, mapped to class |
| local_opaque_write | `$x = <non-literal-new>` | callable scope; invalidates |
| property_opaque_write | `$this->p = <non-literal-new>` | recorded as fact; rule ignores |

Local opaque writes that cannot affect any bounded decision (no parameter
hint and no literal-new write for the same receiver in the same scope) are
pruned at extraction to keep the artifact proportional.

## Receiver rules (php.call.typed_receiver_method_candidate)

- `$x->m()` (or `$x?->m()`): find enclosing callable scope. If any write to
  `$x` precedes the call → the parameter hint is dropped and the last-write
  rule applies: union of literal-`new` classes after the last opaque write;
  nearest write opaque → OutOfScope(receiver_type_unavailable). With no
  writes → the parameter hint's class arms apply.
- `$this->p->m()`: property type hints in the class scope + literal-`new`
  writes to the same property anywhere in the class → union of class
  candidates.
- `?T` → `T`; `A|B` → all named class arms (never first-pick); `A&B`
  intersection → unsupported; builtins → filtered; `self` → lexical class;
  `static`/`parent` → unsupported arms.
- Method lookup is bounded to directly declared methods of the resolved class
  candidates. No global method scan, no inherited/trait/magic dispatch, no
  return-type propagation through chained calls.

## Last-write boundary (no CFG)

Without block scopes, sequential and branch writes are lexically
indistinguishable, so the rule is conservative both ways: union of all
literal-`new` classes written after the last opaque write. `new A(); new B()`
and `if{new A}else{new B}` both yield {A,B} — never a silent single pick, and
an opaque nearest write yields OutOfScope.

Generated with Devin — see REPORT.md for results.
