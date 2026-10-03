# PHP Bounded Receiver-Type Evidence V1 — Report

SOURCE_PROMPT_ID: REPODEX-PHP-BOUNDED-RECEIVER-TYPE-EVIDENCE-V1
(previous: REPODEX-PHP-CALL-CANDIDATES-V1, HEAD ad83425)

## What was built

Normalized `receiver_type_evidence` fact (SCHEMA_VERSION 4) emitted by the PHP
adapter: parameter type hints, declared/promoted property type hints, literal
`new` writes to `$x`/`$this->p`, and opaque writes as invalidation markers
(dead ones pruned). New durable rule `php.call.typed_receiver_method_candidate`
(ABI 9, PHP policy 2) turns that evidence into bounded method candidates.

## §62 implementation answers

1. `ReceiverTypeEvidence` — receiver string (`$x`/`$this->p`), written type
   text + ranges, kind, scope id, evidence range.
2. §7/§8: candidate builders must not reopen source; evidence lives in the
   normalized artifact so provenance/ranges are auditable and digest-visible.
3. SCHEMA_VERSION 3→4; CANDIDATE_RULE_ABI_VERSION 8→9;
   POLICY_VERSION_PHP_CALL 1→2; expected facts re-blessed.
4. Parameter forms: plain `T`, `?T`, `A|B`, `A&B` (unsupported), builtins,
   `self`, `static`/`parent` (unsupported), FQN/alias/qualified names.
5. Property forms: declared `private T $p`, `?T`, unions, plus constructor
   promotion (`property_promotion_parameter`).
6. Promoted properties: yes — both `property_type_hint` (class scope) and a
   `parameter_type_hint` inside `__construct`.
7. Nullable `?T` → `T`; nullability is metadata, not a second target.
8. Union `A|B` → union of all named class arms. Intersection `A&B` →
   unsupported (no candidate), fact retained.
9. Builtins (`int`,`float`,`string`,`bool`,`array`,`callable`,`iterable`,
   `object`,`mixed`,`void`,`never`,`null`,`false`,`true`) filtered, never a
   class lookup.
10. `self` → lexical class via scope chain.
11. `static` → unsupported arm (late static binding not modeled).
12. `parent` → unsupported arm (parent identity not modeled).
13. Parameter reassignment: rule A — the hint applies only while NO write to
    the receiver precedes the call in the same callable scope; with writes the
    local last-write rule applies.
14. Local literal-new scoped to the enclosing callable scope only.
15. Later writes: union of literal-`new` classes after the last opaque write;
    a nearest opaque write → OutOfScope. No stale type carry-through.
16. Branch-dependent writes: no block scopes exist, so branch and sequential
    writes are lexically identical — the union rule preserves ALL literal-new
    classes rather than picking one (`branched` → multiple_candidates).
17. Constructor property assignment: `property_literal_new` recorded at the
    write's scope, mapped to the enclosing class — evidence, not init proof.
18. Multiple property assignments → multiple class candidates preserved.
19. Return values: never inferred. `f()->m()` stays OutOfScope.
20. Docblocks: not used.
21. Composer: not consulted; `Vendor\Pkg\Tool` → `no_candidate`.
22. Source reparsed in candidate layer: no — reads `receiver_type_evidence`.

## §63 static correctness answers

1. Static cases: 41 receiver-call shapes in `fixtures/phprx` (+ phpnav
   regression) — 9 suites, 9/9 green.
2. Supported: 41/41 classified (candidates, no_candidate, or honest OOS).
3. False receiver types: 0.
4. False method candidates: 0.
5. Missing candidates: 0 on resolvable in-repo typed receivers.
6. Ambiguity collapses: 0 — union/duplicate/branch cases yield
   `multiple_candidates`.
7. Stale type-after-write: 0 (`overwritten`, `lateOpaque` → OOS).
8. Global method scans: 0 — lookup bounded to direct methods of resolved class
   candidates.
9. Candidate p50=1, p95=2, max=2 (duplicate decls / union / branch evidence).
10. Receiver coverage: 17/17 targeted = 100%.
11. Transport mismatches: 0 (direct==service==auto byte-identical).
12. FACT promotions: 0 — all edges EvidenceClass::Candidate.

## §64 mechanical gate answers

1. Targeted old `receiver_type_unavailable` cases tested: 17 probes.
2. Gained candidate evidence: 17.
3. Percentage: 100%.
4. Adaptive packets changed: 17.
5. Semantically useful: yes — `Who calls handle?` now surfaces the call nodes
   + `php.call.typed_receiver_method_candidate` edges.
6. Wrong packet changes: 0 (all 6 controls byte-identical).
7. ≥70% gate: PASS (100%).
8. Agent campaign launched: yes.

## §65 agent answers

24 tasks × 3 arms × 2 reps = 144 sessions, INFRA_INVALID=0.

1. N0: 48/48. 2. T0: 48/48. 3. T1: 48/48.
4. Native discovery: N0=179, T0=29, T1=29.
5. Source reads: N0=137, T0=143, T1=138.
6. Source bytes: not separately metered.
7. Tool calls: N0=369, T0=339, T1=332.
8. Model calls: captured per trace; not differential.
9. RepoDex calls: N0=0, T0=165, T1=154.
10/11. Tokens: unavailable in this export (empty final_metrics).
12. Agent wall (median): N0=20.3s, T0=31.4s, T1=26.6s — the aggregate median
    is a cohort-mix artifact; per-cohort T1 ≥ T0 medians.
13. Validated completion wall: same measurements.
14. Receiver cohort (rx): T0 nat=5/rd=41, T1 nat=7/rd=42 — no gain.
15. Code cohort (cx): T0 nat=11/rd=55, T1 nat=11/rd=53 — flat.
16. Negative controls: all 24 (8 tasks × 2 reps × arms) answered
    INDETERMINATE correctly — 0 wrong navigations.
17. Pre-registered efficiency targets (native ≥25%, reads ≥20%, calls ≥15%,
    wall ≥15% lower than T0): NONE passed on the receiver cohort.

## Verdicts

PHP_RECEIVER_TYPE_EVIDENCE_MECHANICALLY_USEFUL_BUT_NO_AGENT_GAIN

PHP_REPODEX_NAVIGATION_CORRECT_BUT_EFFICIENCY_GAIN_NOT_MEASURED

PHP_INHERITANCE_DISPATCH_NOW_HIGHEST_VALUE
(residual: dynamic member names stay deliberately unsupported; largest
remaining implementable class is inherited/LSB dispatch)

Next milestone (recommended, not implemented): bounded PHP inheritance
surface — record `extends`/`implements`/`use trait` edges and widen
directly-declared-method lookup to a bounded declared-in-ancestor chain,
still CANDIDATE-only.

REPODEX_PHP_BOUNDED_RECEIVER_TYPE_EVIDENCE_V1_COMPLETE
