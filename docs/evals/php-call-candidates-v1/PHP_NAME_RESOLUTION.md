# PHP name semantics applied (verified)

- class/interface/trait/enum names: CASE-INSENSITIVE. Verified via
  `new repo()` -> `class Repo`, `helper::RUN()` -> `Helper::run`.
- function + method names: CASE-INSENSITIVE. `SLUGIFY()` -> `slugify`.
- constants/variables: case-sensitive — no V1 rule consumes them.
- unqualified function `f()` in ns `N`: `N\f` first, then global `f` fallback
  (functions only). Tier recorded in provenance evidence
  (`tier=current_namespace` / `tier=global_fallback` / `tier=use_function`).
- unqualified class `new X` in ns `N`: `N\X` ONLY — no global fallback.
- qualified `A\B\f()`: first segment substitutes through the file's `use`
  alias table; else `ns\A\B\f`. `\A\B\f()` = FQN. `namespace\X` = ns-relative.
- `self`/`static`/`parent` are scope keywords, not names: self -> lexical
  class; static/parent -> OutOfScope (LSB / unmodeled inheritance).
