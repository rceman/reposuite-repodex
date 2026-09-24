# Guarded Evidence Recipes V1 — spec

A recipe is a bounded, version-safe plan that reconstructs current evidence for
a recurring investigation shape — never a cached answer. Executed against the
current validated view via typed selectors; output feeds the Context Compiler.

Pipeline: `recipe store (project-scoped) -> match_recipe (term+intent+scope) ->
execute (anchor resolve + selector sub-queries on current engine) -> produced
witnesses -> Context Compiler -> EvidenceProjection`.

Policy: `recipes = off | auto | force`. Deterministic query is independent —
recipes are never a required dependency.
