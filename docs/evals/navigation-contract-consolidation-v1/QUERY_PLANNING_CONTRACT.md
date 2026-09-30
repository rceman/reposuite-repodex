# Query planning contract

- Explicit --intent/--target/--to ALWAYS win (plan() takes them first).
- With --nav adaptive and no explicit intent, the question is classified and
  mapped to a typed operation BEFORE retrieval:
    Callers->Callers(target), Callees->Callees(target),
    PathOrFlow->Paths(target,to when two anchors),
    Relationship->Related, Locate/Definition/ManifestConfig/TestEvidence/
    GeneralStructural/Ambiguous -> Find.
- Anchor extraction is bounded+deterministic (identifier tokens minus a fixed
  stopword list). No model, no embeddings, no NLP.
- Anchors only feed typed ops; a generic Find is text-driven, never
  anchor-guessed. Zero anchors for a typed intent -> Find fallback (honest).
