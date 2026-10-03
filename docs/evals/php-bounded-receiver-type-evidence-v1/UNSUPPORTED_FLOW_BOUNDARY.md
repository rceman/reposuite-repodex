# Unsupported boundary (explicit)

OutOfScope / no-candidate by design:

- `$x->m()` with no param hint and no literal-new write → receiver_type_unavailable
- `$x` opaquely reassigned before the call → receiver_type_unavailable
  (stale type evidence is never carried through a write)
- `$this->p` with no hint and no literal-new write in the class →
  receiver_type_unavailable
- `A&B` intersections, `static`, `parent` type arms → no candidates
- `$a->b->m()` property chains beyond `$this->p` → receiver_type_unavailable
- `f()->m()` return-type receivers → receiver_type_unavailable
- dynamic member names `$x->{$m}()` → dynamic_member_name (unchanged V1)
- `new static` / `parent::` / `static::` → V1 out-of-scope reasons
- DI/container/framework dispatch, magic `__call`, inheritance → not modeled

Multiple property writes (`new A` + `new B`) union into multiple candidates —
never an arbitrary winner. Sequential local writes union conservatively
because without block scopes a sequential write and a branch write are
lexically indistinguishable.
