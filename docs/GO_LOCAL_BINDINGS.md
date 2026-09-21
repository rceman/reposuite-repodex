# Go Local-Binding Facts (TASK 4B)

RepoSuite RepoDex persists Go function-local lexical bindings as the shared
`LocalBindingOccurrence` fact (added for Rust in TASK 3C). A later candidate
layer reads these persisted facts — it never reparses source — to decide whether
a closer local name blocks a package-level function from being a `plain_name`
call candidate.

## Why

Without binding facts, this would be a false candidate:

```go
func helper() {}

func run() {
    helper := func() {}
    helper()   // the local `helper` shadows the package-level `helper`
}
```

## Pipeline

```text
Go source
   ↓
Tree-sitter extraction (parser/go.rs)
   ↓
LocalBindingOccurrence (shared model — NOT a Go-only subsystem)
   ↓
TASK 3A snapshot (persisted)
   ↓
later Go candidate reasoning (consumes facts, never reparses)
```

## Emitted kinds

| `BindingKind`             | Go source form                                   | visibility                              |
|---------------------------|--------------------------------------------------|-----------------------------------------|
| `function_parameter`      | `func f(x int)`                                  | function body                           |
| `function_result`         | `func f() (r int)` named result                  | function body                           |
| `method_receiver`         | `func (s *T) m()`                                | method body                             |
| `function_literal_parameter` | `func(x int){...}`                            | literal body                            |
| `function_literal_result` | `func() (r int){...}`                            | literal body                            |
| `short_variable`          | `x := v`                                         | end of decl → end of block              |
| `variable`                | local `var x ...`                                | end of spec → end of block              |
| `constant`                | local `const x = ...`                            | end of spec → end of block              |
| `local_type`              | local `type X ...`                               | end of spec → end of block              |
| `range_variable`          | `for k, v := range x`                            | the loop body                           |
| `type_switch_variable`    | `switch v := x.(type)`                           | each clause's statement list (disjoint) |
| `select_receive_variable` | `select { case v := <-ch: }`                     | that clause's statement list            |
| `type_parameter`          | `func f[T any]`                                  | the whole declaration (signature+body)  |

## Policies

- **Blank identifier** — `_` is never a binding anywhere (params, results, `:=`,
  `var`, `const`, range, select, type-switch alias).
- **Labels** — `name:` is a separate namespace; never a value binding.
- **Fields/selectors** — struct fields, composite-literal keys, and selector
  names are never bindings.
- **Package-level decls** — top-level `var`/`const`/`type`/`func` are
  `DeclarationOccurrence`s, not `LocalBindingOccurrence`s.
- **Imports** — import names stay in `ImportOccurrence`; not duplicated as
  bindings.

## `:=` new-vs-redeclaration

A `LexBlock` tracks the names introduced *in the current block* (seeded with the
signature names for a body block). For `x, y := ...`:

- a LHS name already declared **in this block** → redeclaration, no new binding;
- a LHS name declared only in an **outer** block (or the signature) → a new
  binding in this block (shadowing);
- `_` is skipped.

This is purely lexical scope tracking — no type checking.

## Implicit blocks

Go's implicit lexical blocks are modelled through `visibility_ranges` (not
promoted to `Scope` entities):

- `if`/`switch`/`for` initializer names → visible across the whole statement
  (the implicit block), covering condition + post + branches;
- each `case`/`default`/`select` clause body → a fresh inner block;
- each `for`/`range` body → a fresh inner block;
- nested `{}` blocks → a fresh inner block.

## Type-switch variable

`switch v := x.(type)` is **one source occurrence** with **multiple disjoint
`visibility_ranges`** — one per clause body. The name is not duplicated into
fake per-clause occurrences.

## Type parameters

`func f[T any]` — `T` is persisted as `type_parameter` because `T(v)` is a
`plain_name` call form (the grammar does not classify it `type_conversion`).
Without it, `T(v)` could falsely attach a package-level `func T`.

## Go 1.22 range note

Go 1.22 changed per-iteration range-variable *runtime* semantics. RepoDex only
needs the source lexical name + visibility for candidate suppression; runtime
object identity is out of scope and not modelled.
