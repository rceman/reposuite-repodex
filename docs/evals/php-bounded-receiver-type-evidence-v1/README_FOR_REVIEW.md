# PHP Bounded Receiver-Type Evidence V1 — review guide

Change surface:

- `src/model/facts.rs` — `ReceiverTypeEvidence` + `ReceiverEvidenceKind`
- `src/model/analysis.rs` — `receiver_type_evidence` collection,
  SCHEMA_VERSION 3→4, canonical rendering, deterministic ordering
- `src/parser/builder.rs` — `push_receiver_evidence`, `receiver_evidence`,
  `retain_receiver_evidence`
- `src/parser/php.rs` — emission of parameter/property type hints, literal
  `new` writes, opaque writes; dead-opaque-write pruning
- `src/candidates/rule_php.rs` — `typed_receiver` rule (member-call dispatch
  refactor: `$this` unchanged, everything else through evidence)
- `src/candidates/model.rs` — rule registry entry, ABI 8→9,
  POLICY_VERSION_PHP_CALL 1→2
- `fixtures/phprx/` — frozen 6-file receiver corpus covering every §35 shape
- `tests/php_receiver_type.rs` — 9 source-grounded suites
- `tests/canonical_completeness.rs` — mutation sweep covers the new fact type
- `tools/eval/fixtures/php-rx/` + `PHP_RX_TASK_CORPUS.json` — 24-task
  distractor-heavy Agent campaign fixture

Correctness anchors for review:

1. Last-write safety: an opaque write between the hint and the call drops the
   type evidence entirely (`overwritten`, `lateOpaque` cases).
2. Branch unions: `if/else` literal writes produce multiple candidates, never
   a silent pick (`branched`, `sequential`).
3. Property evidence is class-scoped: `$this->p` in class A cannot type
   `$this->p` in class B; writes in any method attribute to the class.
4. Typed receivers produce CANDIDATE edges only — EvidenceClass::Candidate
   through the graph into adaptive RDX; no FACT promotion exists.
