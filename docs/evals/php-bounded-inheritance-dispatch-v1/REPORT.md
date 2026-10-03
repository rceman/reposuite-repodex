SOURCE_PROMPT_ID: REPODEX-PHP-BOUNDED-INHERITANCE-DISPATCH-V1-20261003-225200
PREVIOUS_PROMPT_ID: REPODEX-PHP-BOUNDED-RECEIVER-TYPE-EVIDENCE-V1
REPORT_DATE_TIME: 2026-10-03 23:59:00 Europe/Riga

# PHP Bounded Inheritance Dispatch V1 — Final Report

## Implementation answers (§63)

1. Structural rules added: `php.class.extends`, `php.class.implements`,
   `php.class.uses_trait` over existing parser references
   (BaseClass / ImplementedInterface / TraitComposition).
2. Source reparsed in link/candidate layers: **no** — links consume
   normalized references; candidates consume link records + normalized facts.
3. Class names resolve via `php_resolve::class_fqn`: FQN, `namespace\`
   relative, `use`-alias first-segment, current-namespace; case-insensitive;
   no global class fallback.
4. Resolver shared: **yes** — `src/php_resolve.rs` used by `links/rules_php`
   AND `candidates/rule_php` (construction, scoped, receiver-type,
   hierarchy). `enclosing_namespace`, `name_aliases`, `function_aliases`,
   `last_segment` also shared.
5. Duplicate parent declarations: `Ambiguous` preserved in the link; the
   candidate walk follows ALL bounded parent candidates (verified: Dup/P1
   and Dup/P2 arms each contribute their own method).
6. External/unindexed parents: `Unresolved` link; walk finds nothing →
   `no_candidate`, never guessed.
7. Inheritance cycles: visited set + depth bound 64; `Cyc1 <-> Cyc2`
   terminates and still yields the reachable method.
8. Interfaces: `php.class.implements` structural contract links + graph
   edges for relationship queries.
9. Interface declarations as runtime implementations: **no**
   (INTERFACE_RELATION_USED_AS_METHOD_IMPLEMENTATION=false).
10. Traits: `php.class.uses_trait` structural composition links.
11. Trait methods for dispatch: **no** (adaptation unmodeled → conservative).
12. Adaptations (`insteadof`/`as`): not normalized → trait dispatch
    entirely unsupported rather than partially wrong.
13. Ancestor depth: BFS levels over `extends` links; level 0 = direct
    methods, level d = d-th ancestor frontier.
14. Stops at nearest level: **yes** — first level with candidates wins.
15. Override suppresses ancestor candidates: **yes** (D::over blocks B/A).
16. `$this->m()`: lexical class direct, else nearest ancestor level
    (rule `inherited_method_candidate`, `ancestor_depth` in provenance).
17. `self::m()`: same walk from lexical class; own private reachable.
18. typed `$x->m()`: same walk per receiver class; private reachable only
    when the searched class IS the lexical class.
19. `parent::m()`: lexical class → `extends` targets → nearest-level walk
    starting at each parent; rule `php.call.parent_method_candidate`.
20. `new parent()`: parent declaration candidates, rule
    `php.call.parent_construction_candidate`; no `__construct` claim.
21. `static::m()`: OutOfScope `late_static_binding` (unchanged).
22. `new static()`: OutOfScope `late_static_binding` (unchanged).
23. Private parent methods: never reachable (DeclarationFlag::Private at
    every ancestor level; same-class level-0 only).
24. Abstract methods: remain method candidates on their declaring class
    (contract declarations); the walk still prefers nearer concrete levels.
25. ABI/policy: SCHEMA_VERSION 4→5 (Private/Protected flags),
    LINK_RULE_ABI_VERSION 3→4, POLICY_VERSION_PHP 1→2,
    CANDIDATE_RULE_ABI_VERSION 9→10, POLICY_VERSION_PHP_CALL 2→3.

## Static safety (§64)

wrong_parent_links=0, ambiguity_collapses=0, false_FACT_promotions=0,
private_method_false_candidates=0 (one PRE-EXISTING false candidate
eliminated: `A $a -> $a->hidden`), interface_implementation_guesses=0,
trait_adaptation_guesses=0, late_static_guesses=0, unbounded_cycles=0,
global_method_scans=0, transport_mismatches=0.

## Mechanical answers (§65)

1. inheritance-targeted cases tested: 19 (+ controls)
2. supported denominator: 19
3. gained hierarchy candidates: 19
4. 100% (>=80% gate PASS)
5. adaptive packets changed: callers/related/path probes gain
   class_extends edges and candidate edges (verified byte-diff on
   `related class D`, `callers method base`, `path class D to class A`)
6. wrong packet changes: 0
7. relationship queries: extends links surface as `class_extends` graph
   edges in `related` intent packets
8. candidate p50/p95/max = 1/2/2 (dup-parent arms)
9. hierarchy depth p50/p95/max = 1/3/3
10. >=80% gate: PASS (19/19 supported gained; unsupported controls stayed
    honest)

## Agent answers (§66)

1. Pilot launched: yes — 12 tasks x N0/H0/H1 x 1 rep = 36 sessions,
   0 infra-invalid.
2. N0: 12/12. 3. H0: 12/12. 4. H1: 12/12.
5. Headroom: none — ceiling on all arms.
6. Full campaign: NOT launched (§49 headroom gate).
7. Why: all arms saturated at 12/12; no strong efficiency delta
   (h1 native_search 8 vs h0 12 on n=12 — 4 calls total; wall +17%;
   reads/tools flat).

## Verdicts

PHP_INHERITANCE_DISPATCH_MECHANICALLY_USEFUL_BUT_NO_AGENT_GAIN

PHP_REPODEX_NAVIGATION_CORRECT_BUT_EFFICIENCY_GAIN_NOT_MEASURED

NO_FURTHER_PHP_STATIC_NAVIGATION_EXPANSION_CURRENTLY_JUSTIFIED

This is the THIRD consecutive PHP semantic expansion (call candidates,
receiver-type evidence, inheritance dispatch) that is mechanically correct
yet produced no measurable Agent benefit — each corpus ceilings at or near
100% on all arms. §57 applies: stop adding static-analysis features until
production telemetry names a concrete gap.

## Next milestone (exactly one)

Production telemetry instrumentation for RepoDex usage: record real
RepoDex-first sessions' failure/gap signatures (which OutOfScope/NoCandidate
reasons actually drive agents to native fallback) so the next
RepoDex-side milestone is chosen by observed need, not by the next-nearest
static category. Not implemented here.

## Final status

REPODEX_PHP_BOUNDED_INHERITANCE_DISPATCH_V1_COMPLETE
