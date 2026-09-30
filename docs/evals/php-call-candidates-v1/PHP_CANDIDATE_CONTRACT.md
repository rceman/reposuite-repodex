# PHP candidate contract (V1)

candidate != resolved target. A PHP candidate means a bounded source-grounded
rule matched one or more declarations. SingleCandidate is still a candidate.

Rules (durable ids):
- php.call.namespace_function_candidate — FQN `\A\B\f()`, qualified `A\B\f()`
  (first-segment alias -> import prefix, else current-ns prefix),
  `namespace\f()`, unqualified same-namespace, PHP global-function fallback.
- php.call.imported_function_candidate — `use function` direct + aliased.
- php.call.construction_class_candidate — `new X` literal forms + `new self`;
  target is the class-like decl, never a __construct claim. `new static` /
  `new parent` -> OutOfScope.
- php.call.static_method_candidate — `Scope::m()` literal scope+name; bounded
  class candidates' DIRECT methods only. `static::`/`parent::` -> OutOfScope.
- php.call.lexical_self_method_candidate — `self::m()` -> lexical class direct.
- php.call.lexical_this_method_candidate — `$this->m()` -> lexical class direct.

Cardinality: NoCandidate / SingleCandidate / MultipleCandidates / OutOfScope.
No ExactTarget. Duplicates preserved (multiple_candidates).
