# Investigation Memory — spec/eval

Build a rebuildable historical-navigation memory over InvestigationEpisodes:
QuerySignature + bounded postings index + factual per-path evidence +
staleness. Offline eval only — no production ranking change.

## Tracks (all offline, no Agents rerun)
- A exact-repeat: memory = earlier reps of the same question.
- B leave-question-out: memory = other 19 questions only.
- C paraphrase: frozen reworded questions -> original-question memory.
- Union: RepoDex packet set A vs A+memory (no ranking weight tuned).
- Baseline: global AgentActivity popularity (query-independent).

## Harness-neutral boundary
RepoDex ingests canonical AgentEvent v1 only; ATIF->canonical via
scripts/adapters/devin-atif.py (prototype/test tooling).
