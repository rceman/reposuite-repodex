# Structured disposition contract (graph schema 3)

```text
CandidateOutcome
    -> GraphNode { disposition (legacy string),
                   disposition_kind, disposition_reason }
    -> SeedOut/RelOut { same trio }
    -> RepoQueryObservation { gap_signatures[{family, reason_code}] }
```

Kinds: single_candidate, multiple_candidates, no_candidate,
out_of_scope, no_candidate_rule.

`reason` = the producer's stable code verbatim (e.g.
`receiver_type_unavailable`, `late_static_binding`,
`no_indexed_class_for_receiver_type`); it maps 1:1 into
`GapSignature.reason_code` — unknown future codes pass through, never
collapsed to `no_result`/`unknown`.

Observation schema stays `reposuite.repodex.query-observation.v1`
(additive gap content only — no field shape changed).
