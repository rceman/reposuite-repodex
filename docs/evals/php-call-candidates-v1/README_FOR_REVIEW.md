# php-call-candidates-v1 — review map

Bounded PHP call-candidate layer over existing facts (no parser rewrite, no
Composer, no dispatch modeling). See PHP_CANDIDATE_CONTRACT.md for rules,
PHP_NAME_RESOLUTION.md for language semantics, PHP_UNSUPPORTED_BOUNDARY.md for
explicit gaps.

Implementation:
- src/candidates/rule_php.rs — six bounded rules + bounded indexes
  (FQN function/class maps, file-local lexical class/import tables). O(calls)
  after O(decls) index build; no calls x decls scan.
- src/parser/php.rs — narrow §21 fix: scoped_call_expression flags a dynamic
  member selector (Foo::{$m}()); regression test in tests/php_adapter.rs.
- src/candidates/model.rs — ABI 7->8, POLICY_VERSION_PHP_CALL=1, rule registry.
- src/candidates/artifact.rs — allowed candidate kinds = actual decl kind in
  {function, method, class, interface, trait, enum}; claimed kind must match.
- tests/php_call_candidates.rs — 14 tests covering every §48 shape + §50
  safety + §42 graph/query end-to-end + §40 ABI.

Corpus: fixtures/phpnav (static gold, 36 dispositions) + committed
tools/eval/fixtures/php-lib + php-app (composer.json inert). Campaign:
tools/eval/run_php_campaign.py, N0/P1(baseline 07791c0)/P2.

Evidence: traces/ has P1-vs-P2 adaptive packets; SESSION_METRICS.csv +
VALIDATION_RESULTS.json + PACKET_METRICS.json land after the campaign.
