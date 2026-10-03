# Unsupported dispatch boundary (kept explicit)

- `static::m()` / `new static()` → OutOfScope `late_static_binding`.
- Trait methods → never candidates (adaptations `insteadof`/`as` unmodeled).
  `php.class.uses_trait` links remain structural evidence.
- Interface methods → contract declarations only; an interface-typed
  receiver does NOT enumerate implementers.
- `parent` unindexed → `no_indexed_parent_for_lexical_class`.
- Cycles → visited-set + 64-depth bound; traversal terminates with whatever
  the nearest reachable level yields.
- `__construct` dispatch for `new parent()` → not claimed.
- Composer/vendor parent classes → Unresolved links → no candidates.
