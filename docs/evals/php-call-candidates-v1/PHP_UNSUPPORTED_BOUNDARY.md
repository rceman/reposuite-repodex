# Explicitly OutOfScope in V1 (agent-visible honest gaps)

- `$obj->m()` / `$obj?->m()` — receiver_type_unavailable (no type inference)
- `$this->prop->m()` — property receiver (same reason)
- `$this->{$m}()`, `$this->$m()`, `Foo::{$m}()` — dynamic member name
- `$cb()` — variable callable; `foo(...)`/`Foo::m(...)` first-class callable —
  recorded as a *reference*, never an invocation
- `static::` / `new static()` — late_static_binding
- `parent::` / `new parent()` — parent_scope_not_modeled_in_v1
- `new $cls()` — dynamic_class_expression
- magic __call/__callStatic, trait-adapted/inherited dispatch — not modeled;
  no_direct_method_* is honest no_candidate, not "no runtime target"
- vendor/Composer-resolved names — not indexed (composer inert fixture)
