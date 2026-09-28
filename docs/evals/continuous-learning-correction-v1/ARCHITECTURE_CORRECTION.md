# Corrections over V1
- Production generate_questions() now drives the curriculum (was hand-authored
  and mislabeled ordinary decls as 'entrypoint'; inventory() correctly finds
  only `main`).
- Real checkpoint snapshots derive from first-N investigations; C5=C10=C25=C50
  = 3 artifacts => honest LEARNING_SATURATES_AT_C5.
- LEARNED_NAV_HINT removed from Agent context; memory acts INTERNALLY via
  RouteBias (reorders current relations/seeds), invisible to the Agent.
- Dependency-aware rebind: validates stored dep identities, not whole-view
  digest equality.
- Bounded MemoryIndex (family+anchor intersect) — no per-query full scan.
- Real 100/1k/10k scale benchmark.
