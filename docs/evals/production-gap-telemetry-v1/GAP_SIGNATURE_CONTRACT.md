# Gap signatures

`{family, reason_code}` — family is a small closed set; reason_code is the
producer's stable identifier head (identifier prefix before `(`/whitespace),
never free prose.

| family | source |
|---|---|
| no_result | no seeds at all |
| ambiguous_result | intent unclassifiable (`no_confident_intent`) |
| result_limit | seed bound (`seed_bound`) or `relationship_limit` |
| evidence_truncated | `truncated_continuation` (byte budget / relevant bound) or upstream truncation reason |
| bounded_no_route | connector `found:false` |
| no_candidate | candidate node disposition `no_candidate` |
| out_of_scope | candidate disposition `out_of_scope` (e.g. `late_static_binding`, `dynamic_member_name`) |
| unsupported_evidence_class | any other explicit gap line |
| missing_anchor / ambiguous_anchor | reserved for anchor-resolution producers |

Language-specific codes (PHP `receiver_type_unavailable`, `late_static_binding`,
`dynamic_member_name`…) stay opaque reason_codes — no language fields exist in
the core model. Unknown future codes pass through unchanged.
