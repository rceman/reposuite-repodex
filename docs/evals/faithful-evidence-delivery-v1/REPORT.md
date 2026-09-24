# Faithful Bounded Evidence Delivery + Connector Packets V1 — Report

- `SOURCE_PROMPT_ID: REPODEX-FAITHFUL-BOUNDED-EVIDENCE-DELIVERY-V1`
- `PREVIOUS_PROMPT_ID: REPODEX-REPOSITORY-VIEW-DEPENDENCY-VALIDITY-GATE-V1`
- `REPORT_DATE_TIME: 2026-09-24 14:51:58 Europe/Riga`

## Housekeeping (§1)

Previous-mission precheck: HEAD was `af372e5` (the committed validity-gate eval
artifacts, on top of `c2bd73a`), clean, on `main`. The committed
`PRE_FIX_REPRODUCTIONS.json`/`REPORT.md` are accurate (7 REPRODUCED +
1 NOT_APPLICABLE); the "Five" wording was a transcription typo in the
conversational summary only — **no committed doc change needed**.

## What was lost before (§4-§5)

- service-routed compact output emitted `kind`+`label` only — path/locator/
  evidence/via all dropped (LOSSY_AGENT_RELEVANT / BUG).
- No explicit seed `rank` on any machine transport; RDX `F<n>` ids were
  key-sorted and could be misread as rank.
- `candidate_set`, `disposition`, `rule_id` (provenance) dropped from machine/
  service/human (candidate_set survived only as RDX `cs=`).
- `intent=paths` degraded to one-hop neighborhood and the request had no
  `target`/`to` — a real two-endpoint path query was unreachable (BUG).
- Current declaration/body ranges existed in `FileAnalysis` but were never
  surfaced; `complete` was ambiguous about scope.

## Implementation

- `src/query/projection.rs`: the ONE `EvidenceProjection` all transports render
  from. Seeds carry `rank`/`key`/`kind`/`label`/`path`/`language`/`disposition`/
  `score`/`factors`/`declaration_range`/`body_range`; relations carry
  direction-resolved `from_*`/`to_*` + `via` + `rule_id` + `candidate_set` +
  `disposition`; scoped completeness + `bounds`; connector packet for `paths`.
- `engine`: `RelatedHit.rule_id` surfaced; `QueryResult.paths` for real
  two-endpoint traversal; related hard-capped at 256.
- Range materialization reads the snapshot's stored `FileAnalysis` (no reparse);
  ranges omitted when unresolvable (never invented).
- RDX `#RDX1 v2`: `D` (rank+path+ranges), `R` (+`rule=`), `P` (connector routes);
  `F<n>` stays a non-rank local id; v1 still decodes.
- `QueryRequest`/`ViewQueryParams` carry `target`/`to`/`depth`.
- Service `/v1/query` and CLI service-compact render the same projection.

## Performance (PERFORMANCE.json)

Warm real-repo `ensure_ms` ~9-17ms; faithful bytes ~3x the old machine shape but
bounded (seeds<=50, related<=256, ranges<=seeds, routes<=2); range
materialization does 0 reparses.

## Agent pilot (AGENT_PILOT.json) — `AGENT_PILOT_RUN=true`

64 serial `devin -p --export` zero-context sessions (E0 control vs E1 faithful),
same retrieval, on reposuite-repodex. Medians over 32 sessions each:

| metric | E0 control | E1 faithful | delta |
|---|---|---|---|
| correctness (gold recall) | 28/32 | 29/32 | non-inferior (+3.1pp) |
| input tokens | 61,270 | 48,916 | **−20%** |
| output tokens | 1,257 | 778 | **−38%** |
| tool calls | 4.0 | 2.5 | **−38%** |
| searches | 2.0 | 1.0 | **−50%** |
| reads | 2.0 | 2.0 | unchanged |
| wall time | 30.3 s | 17.6 s | **−42%** |

Biggest win: `C2-connector` (bounded no-route) — E1 cut tool calls 27→12.5,
input tokens 545K→186K, wall 152s→93s because the explicit "no route within
depth≤3, not global absence" stops exhaustive searching. Only regression:
`L2-lookup` cost +15K input tokens (faithful packet larger than a trivial
single-match lookup needs). **Hypothesis confirmed**: faithful delivery reduces
Agent work cheaper than a better retriever.

## Classification

`FAITHFUL_BOUNDED_EVIDENCE_DELIVERY_READY` → next: `REPODEX_REPOSITORY_VIEW_BOUND_SYMBOL_MEMORY_V1` (not implemented).

## Final status

`REPODEX_FAITHFUL_BOUNDED_EVIDENCE_DELIVERY_COMPLETE`
