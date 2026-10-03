# Type-name resolution (reuses PHP_CALL_CANDIDATES_V1 semantics)

No second resolver: written type arms go through the same `resolve_class`
logic as `new X`/`X::m()`:

- `\A\B\C` → FQN `A\B\C`
- `namespace\X` → current namespace prefix
- `A\B` qualified → first-segment `use` alias substitution, else current-ns prefix
- `X` unqualified → `use` alias target, else current-ns `ns\X` (NO global fallback)
- `self` → lexical class; `static`/`parent` → unsupported (no candidates)
- builtins (`int`,`string`,`callable`,...) → filtered, never a class lookup
- unindexed names (vendor, missing) → honest `no_candidate`, never invented

Union/intersection: `A|B` contributes every named class arm; `?T` contributes
`T`; `A&B` contributes nothing (intersections unsupported in V1).
