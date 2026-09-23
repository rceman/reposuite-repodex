# Investigation Memory v1

Rebuildable historical navigation prior derived from InvestigationEpisodes.
**Observational/navigation aid only** — never promotes an entity to source
truth (§2), never a query->answer cache (§17), never feeds production ranking.

```text
AgentEvent -> InvestigationEpisode -> MemoryEntry (per investigation)
   + postings index (term/identifier/path -> investigation_ids)
```

## ContextArtifactPresented (§3-§9)

Canonical `context_artifact_presented` event records external/precomputed
context supplied to the Agent — RepoDex RDX packet, retrieval bundle, planner
context. Harness-neutral fields: `artifact_id`, `artifact_kind`, `producer`,
`content_digest`, `content_bytes`, `presented_at`, `references[]`
(`reference_kind`, `path`, `entity`, `rank`, `relation`). A surfaced path is
NOT observed/read (§7) — three facts stay separate. Reconstructed for E1/E2/E3
from the structured `<RETRIEVAL_PACKET>` block (RDX1 = RepoDex's own format) by
the external adapter; appended as canonical events. E0 = absent.

## QuerySignature (§18-§19)

Versioned deterministic representation: `raw_digest` (sha256 of raw query),
`terms` (normalized lexical, stopword-filtered), `identifiers` (camelCase/
snake/`::`/digit code tokens), `paths` (known-repo-path tokens). Built from the
`QUESTION:` body when present. No embeddings, no Jev.

## Similarity (§20) — fixed, explainable

`sim = 3.0*jaccard(identifiers) + 1.0*jaccard(terms) + 2.0*jaccard(paths)`.
Bounded candidates gathered via postings (term/id/path -> investigation), then
only candidates scored — never a full scan (§21). Components exposed per match
(§46). Weights fixed, not benchmark-tuned.

## Memory evidence (§22-§23)

Per (investigation, path) factual record: `surfaced`/`surfaced_rank`,
`observed`/`first_observed_sequence`, `explicitly_read`/`explicit_read_count`,
`mentioned_in_final_answer`, `structured_evidence_mention`, `task_outcomes`,
cost (`input/output_tokens`, `tool_calls`), provenance (`repository_id`,
`repo_head`), `freshness`. `strongest()` class: surfaced_only < observed <
explicitly_read < final_answer_mention < structured_evidence_mention <
outcome_confirmed. Ignored-surfaced is neutral (§24); negatives need explicit
evidence (§25) — none derived here.

## Staleness (§29-§33)

`freshness`: path still in current snapshot -> `fresh`; absent -> `removed`;
no snapshot -> `unknown`. `changed` (GitActivity/digest evidence) is a future
refinement — never claim fresh from a mere name match. `removed` excluded from
normal output.

## Store + update (§34-§38)

`memory/entries/<inv>.json`, `postings.jsonl`, `checkpoint.json`,
`manifest.json`. Incremental: per-investigation episode digest in checkpoint —
only changed investigations re-indexed (late outcome, new session, new
context artifact all handled). `build --full` rebuilds; deterministic.

## CLI (§69-§71)

```text
memory build --store <derived> --output <memory> [--full] [--repo-root]
memory verify|stats
memory query <text> --limit N --json     # explains matched invs + components + evidence + freshness
memory show <investigation_id>
```
