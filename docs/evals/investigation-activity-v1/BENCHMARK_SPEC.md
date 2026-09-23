# InvestigationEpisode + SourceExposure + AgentActivity — spec

Build the first rebuildable derived-intelligence layer on canonical
`AgentEvent v1`: per-investigation episodes, per-path source exposure, and a
repository-scoped AgentActivity index. Observational only — no ranking.

## Components

- `src/derived/model.rs` — `InvestigationEpisode`, `PathExposure`
  (SourceExposure), `PathActivity` (AgentActivity), `SessionPart`,
  `DerivedCheckpoint`, manifests.
- `src/derived/session.rs` — fold one session's events → `SessionPart`
  (incremental unit); `merge_part` (add-only delta).
- `src/derived/episode.rs` — `merge_episode` (parts→episode), conservative
  `extract_path_mentions` (exact repo-path match + `EVIDENCE:` block + line
  refs).
- `src/derived/activity.rs` — `ActivityIndex` add-only per-event fold with
  distinct-id sets + bounded daily buckets.
- `src/derived/store.rs` — persistence (`sessions/*.part.json`,
  `episodes/*.json`, `activity/paths.jsonl`, `checkpoint.json`, `manifest.json`),
  incremental `derive()`, `verify()`.

## Incremental semantics

Sessions append-only; checkpoint = per-session folded count + store digest +
derived schema version. Update folds only new tail events into activity
(add-only) and rebuilds only investigations that changed. Late events and new
sessions joining an existing investigation both handled.

## Validation

- 8 contract tests (multi-session, late-event, repeated-read, search-vs-read,
  mention, incremental≡rebuild, determinism, corrupt-checkpoint).
- Replay: 240 sessions → 240 episodes + 685 paths; reconcile tokens/tools/
  exposure vs canonical totals; mention↔observed join; offline gold analysis.
