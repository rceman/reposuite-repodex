# RepoDex Language Spike

Per-language grammar, extraction strategy, quirks and unsupported constructs.

Runtime: `tree-sitter` 0.27.0. All four grammars load successfully and report
ABI version 15. `reposuite-repodex languages` verifies grammar loading at
runtime; `tests/cli.rs` asserts that all four report `grammar_loads = true`.

| Language | Crate | Version | Upstream | Root node | License |
| --- | --- | --- | --- | --- | --- |
| Rust | `tree-sitter-rust` | 0.24.2 | https://github.com/tree-sitter/tree-sitter-rust | `source_file` | MIT |
| Go | `tree-sitter-go` | 0.25.0 | https://github.com/tree-sitter/tree-sitter-go | `source_file` | MIT |
| Python | `tree-sitter-python` | 0.25.0 | https://github.com/tree-sitter/tree-sitter-python | `module` | MIT |
| PHP | `tree-sitter-php` | 0.24.2 | https://github.com/tree-sitter/tree-sitter-php | `program` | MIT |

Upstream query files (`tags.scm`, `locals.scm`, `highlights.scm`) were inspected
for grammar shapes. RepoDex does not reuse highlight queries as semantic
extraction: a highlight capture states what to colour, not what a construct
means.

RepoDex owns exactly one query per language:

```text
queries/<language>/recovery.scm
```

It captures `(ERROR)` and `(MISSING)` nodes. Everything else is direct AST
traversal, because declaration extraction needs the exact node ranges, flags and
containment that a flat query capture would have to reconstruct anyway. Query
compilation errors are reported as `query_error` diagnostics and never ignored.

## Extraction strategy, shared

```text
1. adapter::extract(builder, root) walks the tree in pre-order
2. declarations, scopes, imports, references, calls and test evidence are pushed
3. recovery.scm is scanned after extraction
4. builder.finish() sorts, dedupes and re-indexes
```

`FactBuilder` is the only writer. It owns the `LineIndex` used for derived
ranges, the `SourceFile` snapshot, and the dense id assignment.

## Rust

### Supported

```text
declarations   module, nested module, external `mod x;` declaration,
               struct, union, enum, variant, trait, trait method declaration,
               trait default method, impl block (scope), type alias,
               const, static, function, nested function, closure (scope),
               macro_rules! definition, struct/union field,
               associated const and associated type alias inside an impl,
               foreign function signature inside an extern block
scopes         file, module, struct, union, enum, trait, impl, extern_block,
               function, closure
imports        use, grouped use, alias, self, super, crate, glob
references     impl_target, impl_trait, receiver
calls          plain_name, qualified_path, member_selector, generic,
               indirect, macro_invocation
test evidence  attribute (#[test], #[tokio::test], and any path-qualified
               attribute whose last segment is `test`)
```

### Grammar quirks

* `impl<T> Trait for Type<U>` exposes `trait` and `type` fields; the impl body
  is a `declaration_list`. An `impl` with no body still parses (with a MISSING
  `;`), so a bodyless impl yields a scope and its target reference.
* `mod x;` has no body. It is a declaration, never a scope.
* A generic call `f::<T>(x)` keeps `::<T>` in the callee text because that is
  what the source writes. `type_arguments` records `<T>`.
* Tuple-struct construction `TupleStruct(1)` is a `call_expression`. It is
  reported as call-shaped and never claimed to be a constructor.
* Struct literal construction `Type { .. }` is not a call and produces no
  call-like fact.
* `#[cfg(feature = "x")] fn f()` is extracted as written. `cfg` is never
  evaluated, so both branches of a cfg pair would be reported.
* `macro_rules!` bodies are never expanded, so macro-generated declarations do
  not exist in the model.
* A `union` is a distinct `union` declaration and `union` scope. Reporting it as
  a struct would be a wrong fact, so it has its own kind.
* `extern "C" { fn puts(..); }` opens an `extern_block` scope and the foreign
  signature is a bodyless `Function` declaration inside it. The ABI string is not
  a fact, and two extern blocks declaring the same name stay distinguishable.
* Associated items inside an inherent impl (`impl T { const C: u32 = 1;
  type A = u32; }`) are declarations in the impl scope. The impl scope's display
  path borrows the written target (`(impl T)`), but the impl still never declares
  `T`.
* Generic parameters, lifetimes and `where` bounds are syntax, not declarations.
  `dyn Trait` and `impl Trait` produce no references.

### Unsupported or ambiguous

```text
macro expansion                     not attempted
cfg evaluation                      not attempted
trait resolution                    not attempted; impl_trait stays a string
method resolution                   obj.m() is member_selector, not a method
visibility                          not extracted
generic parameters as declarations  not extracted
`dyn`/`impl Trait` as references    not extracted
lifetimes                           not extracted
attribute semantics                 not extracted; only `test` attributes are
                                    interpreted, and only as test evidence
negative/async trait impls          impl scope and refs only, as written
```

Two measured consequences of the macro policy (TASK 2 findings F001, F002):

```text
calls inside macro arguments        NOT extracted. tree-sitter-rust represents
                                    macro arguments as a `token_tree` of flat
                                    tokens with no expression subtree, so
                                    `assert!(f())` records `assert!` but not `f`.
                                    Corpus-wide upper bound: 12,199 of 19,309
                                    macro invocations had call-like argument text.
primitive type as a macro name      REJECTED by the grammar. `str![]`, `u32![]`,
                                    `bool![]`, `char![]`, `usize![]`, `f64![]`,
                                    `i8![]` produce an ERROR node, while `foo![]`
                                    and `vec![]` parse cleanly. The file is
                                    reported as recovered, never as clean.
```

### Recovery

```text
struct Broken { field: , }   MISSING type_identifier, field name preserved
fn incomplete(               MISSING `)`, and the following `fn valid_last() {}`
                             is consumed as a function-type parameter, so
                             valid_last is NOT a declaration
impl Incomplete              MISSING `;`, impl scope and impl_target preserved
```

## Go

### Supported

```text
declarations   package, named type, type alias, struct, struct field,
               interface, interface method element, const, var (grouped and
               single, including iota forms), function, method
scopes         file, struct, interface, function, method, closure
imports        single, grouped, renamed, blank (`_`), dot (`.`)
references     receiver_type (written form, including `*T`), embedded_type
calls          plain_name, member_selector, generic (type_arguments),
               indirect, documented conversion ambiguity
test evidence  function_name_convention (Test/Benchmark prefix rules),
               signature_shape (single *testing.T / *testing.B parameter),
               file_name_convention (_test.go suffix, file level)
```

### Grammar quirks

* **Methods are declared at file scope.** `func (w Widget) Compute()` is not
  lexically inside `Widget`. The receiver type is recorded as a `receiver_type`
  reference attached to the method declaration, and two methods with the same
  name on different receiver types share the same lexical scope path. A Go
  method's source-grounded identity is therefore *name + file scope + written
  receiver*, which `tests/support/mod.rs::declaration_identity` demonstrates.
* Interface method elements *are* lexically inside the interface scope. They are
  a different construct from receiver methods and are modelled as such.
* Generic instantiation is genuinely ambiguous, and every shape is preserved as
  a call-like occurrence rather than resolved:
  * `Generic[int](value)` — a single call argument — parses as
    `type_conversion_expression`. That syntax is simultaneously a valid generic
    invocation with one argument and a valid conversion, so it **is** reported
    as a call-like occurrence, with the `type_conversion` form label and the
    exact range of its written type arguments. RepoDex does not decide which
    reading is correct.
  * `Generic[int, string](value)` still has one call argument, so the grammar
    still calls it a `type_conversion_expression` and it is reported as
    call-like with the `type_conversion` form and `[int, string]` as its
    type arguments. The number of *type* arguments is not the discriminator.
  * `pkg.Generic[int](value)` and `obj.Generic[int](value)` parse as
    `type_conversion_expression` and are reported as call-like with the
    `type_conversion` form.
  * `Generic[int](value, other)` — two or more call arguments — parses as
    `call_expression` and is reported as call-like with the `indirect` form.
  * `Generic[int]` without a call is a `type_instantiation_expression` and
    produces no call-like fact.
  * The discriminator is the **call-argument count**, not the number of type
    arguments. No Go type checker or compiler is involved.
* `int64(5)` with one argument is a `call_expression`, indistinguishable from a
  call at the syntax level. It is reported as call-shaped and never resolved.
* Embedded fields have no written name, so they produce an `embedded_type`
  reference and no field declaration. No fake field name is invented.
* `f := Base.Method` is a method expression: it produces no call-like fact and no
  declaration.
* `go f()` and `defer f()` contain an ordinary `call_expression`; the statement
  kind is not part of the call fact.
* Build tags are not evaluated; a build-tagged file is analyzed as written.
* Short variable declarations and assignments are not declarations.

### Unsupported or ambiguous

```text
selector resolution                not attempted; obj.Method stays written text
promoted methods                   not inferred
type parameters                    not extracted as declarations
build tag evaluation               not attempted
method expressions as references   not extracted
struct tags                        not extracted
short declarations and assignments not extracted
iota evaluation                    not attempted; A and B are plain constants
```

### Recovery

```text
func broken( {        MISSING `)`, ERROR node, validBefore/validAfter survive
var incomplete =      no written name, so no variable is fabricated
```

## Python

### Supported

```text
declarations   module variable, annotated assignment, `Final`-annotated constant,
               class, nested class, method, function, async function,
               nested function, class attribute
scopes         file, class, function, method, lambda
imports        import, multiple import, import alias, from-import, relative
               import (relative_levels), wildcard import
references     decorator, base_class
calls          plain_name, member_selector (attribute), chained, indirect
test evidence  function_name_convention (test_ prefix),
               decorator (any decorator, written form),
               class_base_syntax (written base clause),
               file_name_convention (test_*.py prefix, file level)
```

### Grammar quirks

* A decorated definition's range includes its decorators, so the written
  `@decorator` line is inside the declaration range.
* `getattr(obj, "m")()` produces two facts: a `plain_name` call for `getattr`
  and an `indirect` call whose written callee is `getattr(obj, "m")`. Nested call
  extraction stops at the inner call so the written form stays readable.
* `obj.chained().call()` produces the inner `member_selector` call and the outer
  `member_selector` whose written callee is `obj.chained().call`.
* A default-argument expression inside a method signature belongs to the
  *enclosing class scope*, because it is evaluated when the class body executes,
  not when the method runs. `tests/python_adapter.rs` pins this.
* A module-level `X = 1` is a `Variable` even when the name is uppercase. Only an
  explicit `Final` annotation produces a `Constant`. Uppercase is a convention,
  not syntax, so it is never treated as proof.
* A function defined inside a class body is a `Method`; a function defined inside
  a function is a `Function` with full containment.
* A lambda produces an anonymous `Lambda` scope and no declaration.
* `from . import x` and `from ..pkg import y` record `relative_levels` 1 and 2.
  The dots are not part of the written `module`.
* A base clause is recorded as written (`mixins.Other`), never resolved.

### Unsupported or ambiguous

```text
decorator resolution               not attempted
inheritance resolution             not attempted
dynamic import resolution          not attempted
monkey-patching semantics          not attempted
pytest collection semantics        not attempted
type annotation references         not extracted
global / nonlocal declarations     not extracted as declarations
comprehension scopes               not created as scopes; calls inside them are
                                   attributed to the enclosing scope
match statement bindings           not extracted as declarations
uppercase names as constants       deliberately not treated as proof
```

### Recovery

```text
def broken(:      MISSING `)`, ERROR node, valid_before/valid_after survive
class Incomplete( unterminated, so no class is declared
```

## PHP

### Supported

```text
declarations   namespace (bracketed and unbracketed), class, interface, trait,
               enum, enum case (variant), method, constructor, static method,
               function, property (static, readonly, promoted, visibility),
               class constant
scopes         file, namespace, class, interface, trait, enum, method, function,
               closure, arrow_function
imports        use, grouped use, aliased use, use function, use const
references     base_class, implemented_interface, trait_composition,
               closure_capture, include_target, first_class_callable
calls          plain_name, member_selector, nullsafe member_selector,
               static_scoped, explicit_construction, indirect
test evidence  function_name_convention (test prefix), attribute (#[Test])
```

### Grammar quirks

* The root node is `program` and the tree can contain multiple `php_tag` regions
  separated by `text` (mixed HTML). Declarations in a second `<?php` region are
  attributed to the namespace active in that region.
* A shebang line before `<?php` parses cleanly.
* `use Loggable;` inside a class body is a `use_declaration` at class level:
  trait composition. It is recorded as a `trait_composition` reference and never
  as an import. Namespace-level `use` statements are imports.
* `use App\Support\helper;` produces `category = function`; `use const VERSION;`
  produces `category = constant`. Both are still namespace imports.
* The property sigil is not part of the recorded name: `$cacheKey` is recorded as
  `cacheKey` with a `name_range` selecting `cacheKey`, not `$cacheKey`.
  Promoted constructor properties belong to the *class* scope, not the
  constructor.
* The three callable/construction forms are distinct:

  ```php
  Thing($arg);       // invocation syntax      -> plain_name call-like
  new Thing($arg);   // explicit construction  -> explicit_construction call-like
  Thing(...);        // first-class callable   -> first_class_callable reference
  ```

  Literal `Thing(...)` is neither an invocation nor construction.
* The language constructs `empty($x)` and `isset($x)` are shaped by the grammar as
  `function_call_expression` nodes with a `name` callee, so they are recorded as
  `plain_name` call-like occurrences. RepoDex reports the grammar's shape and does
  not claim they are functions (finding F009).
* `strlen(...)` is first-class callable *creation*: it produces a
  `first_class_callable` reference and no call. `strlen(...)($x)` is an
  invocation and produces an `indirect` call with `dynamic_callee = true`.
* `$callable('x')` and `$widget->{$name}()` set `dynamic_callee = true`. They are
  never resolved.
* `$widget?->render()` sets `nullsafe = true`; the form is still
  `member_selector`.
* `new self()`, `new static()` and `new Widget()` are all
  `explicit_construction` with the written callee text.
* An anonymous class construction (`new class ... { ... }`) is
  `explicit_construction` with `callee_written = "class"` and a `callee_range`
  covering exactly the five-byte `class` keyword, while `expression_range` covers
  the whole construction including the body. This is true for attributed
  anonymous classes too (`new #[Queue('x')] class extends ...`), whose
  `anonymous_class` node begins at `#[`. Before TASK 2 the callee range covered
  the whole class node; that was finding **F003** and is now fixed and pinned by
  `tests/php_adapter.rs::anonymous_class_construction_ranges_only_the_keyword`.
* Trait adaptations (`insteadof`, `as`) parse cleanly; the adaptation clause
  itself produces no facts, and the composed trait names produce
  `trait_composition` references.
* `include`, `include_once`, `require`, `require_once` targets produce
  `include_target` references with the written expression text, never a resolved
  file path.
* Closures and arrow functions have anonymous scopes. `use ($x)` captures are
  `closure_capture` references.
* PHPUnit-style detection is deliberately limited to the `test` name prefix and
  `#[Test]`. The `TestCase` base clause is recorded but is *not* used as PHP test
  evidence, because that would require resolving the imported class.

### Unsupported or ambiguous

```text
namespace resolution               not attempted
autoload resolution                not attempted
Laravel/Lumen semantics            not attempted
dynamic member resolution          not attempted
attribute semantics                not attempted; only `#[Test]` is read, and only
                                   as test evidence
type declaration references        not extracted (parameter, return and property
                                   types produce no facts)
enum backed values                 not extracted as references; cases are
                                   `variant` declarations
interface constants                extracted as `constant` declarations in the
                                   interface scope
trait adaptation clauses           parsed, but `insteadof`/`as` produce no facts
```

### Recovery

```text
public function broken( {   MISSING `)`, ERROR node, class and neighbours survive
function incomplete(        unterminated, no function is declared
```

## Cross-language notes

```text
- `include`/`require` is the only import-like construct that points at a file
  path, and even that stays unresolved written text.
- No language adapter reads another file. Extraction is strictly single-file.
- No adapter consults the filesystem. The scanner reads bytes; the adapter
  receives a path only to record it in the facts.
- Every adapter is exercised by the same shared adversarial fixtures:
  fixtures/shared/adversarial.{rs,go,py,php}.
```
