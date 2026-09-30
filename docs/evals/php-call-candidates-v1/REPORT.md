# PHP Call Candidates V1 — final report

## §82 Implementation answers
1. PHP parser rewritten: NO — one narrow correction only (§21: scoped_call
   flags dynamic member selectors; `Foo::{$m}()` now marked dynamic).
2. Composer implemented: NO — composer.json fixtures are inert.
3. Rules added: php.call.namespace_function_candidate,
   php.call.imported_function_candidate, php.call.construction_class_candidate,
   php.call.static_method_candidate, php.call.lexical_self_method_candidate,
   php.call.lexical_this_method_candidate.
4. Candidate ABI 7 -> 8.
5. POLICY_VERSION_PHP_CALL = 1 (joined candidate fingerprint).
6. Target kinds: function, class (class-like: class/interface/trait/enum),
   method. Claimed kind is verified against the declaration.
7. `use function` (ImportCategory::Function) feeds the imported rule — direct
   and aliased names both map to the written qualified target.
8. Unqualified functions: tier=use_function -> tier=current_namespace ->
   tier=global_fallback, tier recorded in evidence; classes never fall back.
9. Duplicates preserved as MultipleCandidates (App\Dup\collide: 2/2).
10. `new Foo()` -> class-like declaration candidate (kind=class).
11. Class candidate implies __construct resolution: NO.
12. `new self()` -> the lexical class declaration.
13. `new static()` -> OutOfScope(late_static_binding).
14. `Foo::bar()` -> bounded class candidates x directly-declared methods.
15. `self::bar()` -> lexical class's directly declared methods only.
16. `static::bar()` -> OutOfScope(late_static_binding); `parent::` ->
    OutOfScope(parent_scope_not_modeled_in_v1).
17. `$this->bar()` -> lexical class direct method; `$this->prop->m()`
    dispatches on the property -> receiver_type_unavailable.
18. `$object->bar()` unknown receiver -> OutOfScope(receiver_type_unavailable);
    no repository-wide method-name scan.
19. Dynamic member names ($this->$m, $this->{$m}, Foo::{$m}) -> OutOfScope.
20. First-class callables remain references — zero invocation records.
21-23. Inherited/trait-adapted/magic methods: NOT modeled — honest
    no_candidate with reason naming the gap, never "no runtime target".
24-25. Parser correction: yes — scoped_call previously only checked scope
    literalness; `Foo::{$m}()` would have read as literal. Fixed + regression
    test.

## §83 Correctness answers
1. 14 static tests over 36-corpus-case fixture (phpnav) + graph e2e + ABI.
2. FALSE_FACT_PROMOTIONS=0 (all php edges EvidenceClass::Candidate).
3. FALSE_CANDIDATES=0. 4. Missing candidates on V1-supported: 0.
5. Ambiguity collapses: 0. 6. Dynamic-call false positives: 0.
7. Candidates per supported call: p50=1, p95=1, max=2.
8. Largest set=2 (duplicate same-FQN functions, preserved).
9. Repository-wide method-name scans: NO.
10. Direct/service mismatches: 0 (byte-identical on all transports).

## §84 Campaign answers (192 sessions, 0 INFRA_INVALID, 192 TASK_SUCCESS)
1. N0: 64/64 (100%). 2. P1: 64/64. 3. P2: 64/64.
4. Cohorts all 16/16 per arm (lk/cc/st/cd all green).
5. First-discovery RepoDex adherence: P1 53/64, P2 55/64.
6. RepoDex calls: P1 96, P2 102.
7. Native discovery: N0 140+20enum; P1 35+22; P2 38+25.
8. Source reads: N0 155, P1 150, P2 143.
9. Model/tool calls + tokens: TOKEN_ACCOUNTING.json (per-session).
10-15. ARM_SUMMARY.json / TIMING.json.
16. PHP_CALL_CANDIDATE_COVERAGE = 43/58 (74% of supported-form calls carry
    candidate evidence; all 15 non-covered are builtins/absent/external —
    resolvable in-repo calls: 43/43).
17. Call-nav success over P1: equal (both 100% — corpus ceiling).
18. Native discovery over P1: NOT reduced (38 vs 35; within noise).
19. Lookup/control behavior: preserved (lk/st all green, packet-identical
    where relevant).
20. Navigation-dependent code tasks: equal (all green).

## §85 Next-gap answers
1. Largest remaining gap: receiver TYPE INFORMATION ($obj->m, property/$
   param receivers) — 8 of 20 OutOfScope plus the dominant Agent-visible gap.
2. Composer topology failures: 1 (Vendor\Pkg\Tool::work — external dep).
3. Missing type information: 8. 4. Inherited/trait dispatch: 6 (parent + LSB
   + no_direct_method boundary). 5. Dynamic/framework: 7.
6. Composer as next milestone: NOT justified — 1 residual case. TYPE evidence
   is the measured larger gap.

## §86-§89 Verdicts
PHP_CALL_CANDIDATES_CONFIRMED
PHP_REPODEX_FIRST_NAVIGATION_CORRECT_BUT_NO_MEASURED_AGENT_GAIN
PHP_TYPE_EVIDENCE_NOW_HIGHER_PRIORITY

Recommended next milestone (one): PHP bounded receiver-type evidence —
constructor-assigned `$this->prop`, literal `new` locals and parameter
type-hints feeding `$this->prop->m()`/`$svc->m()`/`$repo->m()` candidates.
