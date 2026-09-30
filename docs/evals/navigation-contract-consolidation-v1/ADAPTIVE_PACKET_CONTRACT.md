# Adaptive packet contract (defects C+D)

- Emission is record-aware: head -> seed F -> endpoint F -> D -> R -> P ->
  trailer. A record is emitted whole or not at all. No String byte-slicing;
  UTF-8 scalars, escapes, and records are never cut (zero panics).
- The byte budget covers the WHOLE packet incl. gaps+summary; trailer space
  (512B bound) is reserved before body fill.
- R lines emit only when both endpoint F ids were emitted -> no dangling refs.
- S trailer counts EMITTED records: seeds, related, candidates, and bytes=
  equals the actual final packet size (fixpoint).
- RESULT_LIMIT (selection bound) is distinct from AMBIGUOUS_RESULT (identity
  ambiguity); upstream truncation + relationship_limit + bounded NO_ROUTE are
  preserved as explicit gaps.
- validate_packet(rdx,budget) natively verifies all of the above.
