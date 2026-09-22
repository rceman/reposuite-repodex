# Git Temporal Activity Foundation V1 — spec

Build and evaluate a cheap, persisted, incrementally-updated Git-derived
temporal/activity layer. Goal: determine whether repository history carries a
useful navigation signal BEFORE letting it influence retrieval.

## What is built

`src/temporal/` — a path-centric per-file change-activity index.

- **Collector** (`collect.rs`): ONE streaming
  `git log --no-renames --name-status` subprocess, stdout parsed line-by-line.
  Memory = O(tracked files), never O(commits×files); no per-file `git log`.
- **Record** (`FileTemporal`): `first_seen_commit/at`, `last_changed_commit/at`,
  `change_commit_count`, `change_ts_tail` (≤128 most-recent unix ts) for exact
  30/90/365-day window counts.
- **Artifact** (`artifact.rs`): `manifest.json` + `files.jsonl`, staging→verify→
  atomic-publish; schema/policy/repo-identity/HEAD/digest provenance.
- **Update** (`build.rs`): `OLD..NEW` only when old head is an ancestor; else
  `history_diverged`/`rebuild_required`. Wrong-repo is rejected by identity.
- **Index** (`index.rs`): in-memory `BTreeMap` over the artifact; lookups run
  ZERO Git.
- **CLI**: `temporal build|update|verify|stats|file`, `--json` output.

## Git semantics (documented, deterministic)

- change event = non-merge commit `name-status` diff vs first parent. Merge
  commits emit no file lines → no double-count ("development change events").
- timestamp = committer `%ct`; ages anchored to indexed HEAD committer time →
  deterministic given (repo, HEAD, schema). No wall-clock, no filesystem mtime.
- rename = `--no-renames` → `D old`+`A new`, path-centric; history is NOT
  carried across a rename (explicit V1 limitation).
- delete/recreate = one path history (D and re-A are ordinary change events).
- recent-window counts from the capped timestamp tail — exact while a file has
  ≤128 changes inside the window; hotter files still classify as very active.

## Evaluation

- Corpora: GTW `f09d083` (969 commits) primary; `3dpixels` (455), `bugbounty`
  (237) as larger real repos. No new downloads.
- Perf: build/update wall+RSS, artifact bytes, lookup latency, git-subprocess
  counts.
- Signal: frozen 20-question GTW gold + deterministic (mode-A) candidate set →
  per-file temporal features → grade distributions, Spearman, ROC-AUC,
  per-level, counterexamples, offline naive recency/activity ranking sims.
- No ranking changes; no Jev; no Agent benchmark; no co-change graph.
