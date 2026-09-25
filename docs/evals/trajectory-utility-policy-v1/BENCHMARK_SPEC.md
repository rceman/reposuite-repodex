# Trajectory Utility Policy V1 — spec

Measure which evidence *representations* reduce downstream Agent work — strictly
association, not causation (§1). OBSERVED / OUTCOME_ASSOCIATED / CAUSALLY_VALIDATED
are kept distinct. `utility_policy = off|shadow|apply` (+ seeded randomized
slates for evaluation only). Apply reorders only the optional seed tail —
mandatory identity and FACT/CANDIDATE are untouched.

Pipeline: query -> eligible/optional slate -> DeliveryDecision (logged) ->
continuation metrics (AgentEvents) -> UtilityStats cells (shrunken followup
rates conditioned on scope|shape|repr|role|entity) -> shadow/apply -> trace.
