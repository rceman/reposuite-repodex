# Adaptive evidence navigation architecture

query text
  -> NavIntent classification (deterministic cues, no LLM)   [src/query/adaptive.rs]
  -> evidence obligations (seeds/related/path counts + relevance) 
  -> bounded selection over the canonical EvidenceProjection
  -> semantic dedup (entity identity, relation key)
  -> compact adaptive RDX + explicit gap lines

Public surface stays `reposuite-repodex query` / `repo_query`. `query --nav
adaptive` selects the adaptive packet; default remains full RDX. Same underlying
query engine for CLI/service (parity).
