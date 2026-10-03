# Fact model — ReceiverTypeEvidence (SCHEMA_VERSION 4)

```text
receiver_type_evidence: Vec<ReceiverTypeEvidence>   # FileAnalysis
    evidence_id: u32            # dense, canonical order
    snapshot_id / language / relative_path
    scope_id: u32               # callable scope (params, writes) or class scope (property hints)
    kind: ReceiverEvidenceKind  # parameter_type_hint | property_type_hint |
                                # local_literal_new | property_literal_new |
                                # local_opaque_write | property_opaque_write
    receiver: String            # "$x" or "$this->p" (written form)
    receiver_range: SourceRange
    written: String             # type text / new-class name / opaque RHS text
    written_range: SourceRange
    evidence_range: SourceRange # whole parameter / property decl / assignment
```

Why a normalized fact and not candidate-layer parsing: the candidate builder
must not reopen PHP source — §7/§8 require receiver evidence to be part of
the normalized source-fact artifact so provenance, ranges and scope identity
are auditable and the fact digest changes when evidence changes.

Parser emission (src/parser/php.rs):
- `emit_parameter_type_annotations` inside each pushed callable scope
  (function/method/closure/arrow) — every `type` field on
  `simple_parameter`/`variadic_parameter`/`property_promotion_parameter`.
- Property declarations: `property_declaration.type` → `$this-><name>` per
  property element; promoted parameters also emit a class-scoped
  `property_type_hint`.
- `assignment_expression` → `$x`/literal `$this->name` receivers only;
  literal `new` RHS → `*_literal_new` with the written class name, any other
  RHS → `*_opaque_write`.

Candidate use (src/candidates/rule_php.rs `typed_receiver`): see
RECEIVER_TYPE_CONTRACT.md. Nothing is re-parsed; all lookups consume the
normalized fact list, scopes and import tables.
