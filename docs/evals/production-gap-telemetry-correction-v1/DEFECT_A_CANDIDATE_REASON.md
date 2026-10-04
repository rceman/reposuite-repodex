# Defect A — candidate gap reasons were lost

## Root cause

`GraphNode.disposition` encoded family+reason in ONE string
(`no_candidate:receiver_type_unavailable`). The observation layer checked
`matches!(d, "no_candidate" | "out_of_scope")` — exact equality — so every
disposition carrying a reason silently dropped out of `gap_signatures`.

## Fix (structured, never RDX)

- `GraphNode` gained `disposition_kind` + `disposition_reason`
  (GRAPH_SCHEMA_VERSION 2 -> 3 — old graph artifacts do not silently satisfy
  the new schema).
- `SeedOut`/`RelOut` (EvidenceProjection) carry the same structured pair
  additively; the compact `disposition` string stays for RDX compatibility.
- `observation.rs` reads the structured pair; splitting the legacy compact
  string at the first `:` exists only as a pre-schema-3 fallback — it is our
  own producer vocabulary, not Agent-facing text.
- `dispositions` histogram now keys by `disposition_kind`.

## Verified against real fixtures (no handcrafted metadata)

`callers method save` on fixtures/phpnav ->
`out_of_scope:late_static_binding`, `no_candidate:no_indexed_parent_for_lexical_class`,
`no_candidate:no_method_on_lexical_class_or_declared_ancestors`

`callers method run` on fixtures/phpnav ->
`out_of_scope:receiver_type_unavailable`, `dynamic_member_name`,
`indirect_or_variable_callable`, `dynamic_scope_or_member`,
`dynamic_class_expression`
