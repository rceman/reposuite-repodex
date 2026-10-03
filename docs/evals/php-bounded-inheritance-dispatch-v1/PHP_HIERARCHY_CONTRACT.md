# PHP hierarchy contract — FACT / CANDIDATE boundary

| source form | layer | status |
|---|---|---|
| `class C extends P` written | parser `ReferenceKind::BaseClass` | FACT (source declares the clause) |
| `P` mapped to repo decl | `php.class.extends` link | STRUCTURAL_EXACT_UNDER_RULE / AMBIGUOUS / Unresolved |
| `class C implements I,J` | `php.class.implements` link | structural contract; never an implementation body |
| `use T;` in class body | `php.class.uses_trait` link | structural composition; adaptations unmodeled |
| `$this->m()` / `self::m()` / `Foo::m()` direct | existing rules | CANDIDATE (unchanged) |
| `…->m()` via ancestor | `php.call.inherited_method_candidate` | CANDIDATE, nearest-level only |
| `parent::m()` | `php.call.parent_method_candidate` | CANDIDATE |
| `new parent()` | `php.call.parent_construction_candidate` | CANDIDATE (class only, no __construct claim) |
| `parent` type hint | typed receiver evidence → extends link | CANDIDATE |
| `static::m()` / `new static()` | — | UNSUPPORTED (late_static_binding) |
| trait method dispatch | — | UNSUPPORTED |
| interface receiver → implementers | — | UNSUPPORTED (never enumerated) |

Nothing here is `RESOLVED_CALL` or `PROVEN_RUNTIME_TARGET`. A `Base $x`
parameter may hold a subclass at runtime; the candidate only says the
*declared-type hierarchy* contains the relevant method declaration.
