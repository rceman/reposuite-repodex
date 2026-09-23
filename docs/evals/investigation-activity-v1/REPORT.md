# InvestigationEpisode + SourceExposure + AgentActivity — Report

```text
INVESTIGATION_ACTIVITY_FOUNDATION_COMPLETE
INVESTIGATION_MEMORY_FOUNDATION_READY
```

Base `91bba9a`. Adds `src/derived/` — a rebuildable derived layer over the
canonical Agent Event Store. No AgentEvent/ranking/temporal/Jev/System One
changes; replay of existing canonical events only.

## Schemas

- **InvestigationEpisode** `reposuite.investigation-episode.v1`: investigation
  id, `session_ids[]`, repo/heads, lifecycle `state`, `query_text`/
  `normalized_query`/`query_digest`, actual model usage (total + per-model),
  tool totals (category + native), `source_exposures[]`, unique-file/byte
  summaries, `final_answers[]`, `final_answer_path_mentions[]` (joined to
  observed/read), `evidence_path_mentions[]` (structured EVIDENCE),
  `unresolved_path_mentions[]`, session/task outcomes, reserved
  `quality_outcome`.
- **SourceExposure** = per-path `PathExposure`: seq/time first/last, per-kind
  counts, bytes, merged bounded line_ranges.
- **AgentActivity** = per-(repo,path) `PathActivity`: lifetime counters,
  distinct session/investigation/mention/evidence sets, first/last, bounded
  daily buckets (400-day window).

## Semantics implemented

- **Query**: authoritative user-role `agent_message`; `session_started.task`
  only a fallback. `normalized_query` deterministic (ws+case). `query_digest`
  sha256.
- **Tokens**: sum of per-call actuals only; `usage_is_actual` tracks estimation;
  no session-summary double-count.
- **Mentions**: exact canonical repo-path match vs `git ls-files` (build time);
  `EVIDENCE:` block → stricter `evidence_path_mentions`; non-matching path-like
  → `unresolved`. Mentions ≠ citations.
- **Checkpoint/incremental**: per-session folded count + store digest; only
  tail events folded (add-only); only changed investigations rebuilt; late
  `task_outcome` and new session joining an investigation both work.

## Real replay (240 sessions → 240 episodes, 685 activity paths)

```text
EPISODE_INPUT_TOKEN_MISMATCH  = 0
EPISODE_OUTPUT_TOKEN_MISMATCH = 0
EPISODE_TOOL_CALL_MISMATCH    = 0
FULL_VS_INCREMENTAL_STATE_MISMATCH = 0
RUNTIME_SPECIFIC_DERIVED_DEPENDENCY = 0
reconstructed: input=92,450,895 output=939,108 tools=6,134
source_observation_events=15,256  unique exposed paths=685
```

Per-treatment canonical source exposure (content-delivered only):

| T | files/run | explicit reads/run | src events/run | bytes/run |
|---|-----------|--------------------|----------------|-----------|
| E0 | 56.3 | 13.8 | 86.2 | 162k |
| E1 | 35.6 | 12.2 | 58.4 | 141k |
| E2 | 32.8 | 12.7 | 52.2 | 132k |
| E3 | 34.7 | 9.8  | 57.4 | 119k |

(§48: prior benchmark `files observed` counted filename discovery too — E0
193.8 etc.; canonical `SourceObserved` is the tighter, correct definition.)

## Final-answer mentions (§49)

```text
answers with path mentions   239/240
total path mentions          1,197   unique paths 107
mentions observed            1,196   (98.5%)
mentions explicitly read     1,176
mentions NOT observed        1   — scripts/test-fast.py (L3-Q5__E0__r2):
                                   real repo path cited but never tool-read;
                                   likely inferred / initial-packet. Flagged.
unresolved path-like         44   — symbol refs (internal/query.Parse),
                                   template fragments (<Hub.Branch), one
                                   fabricated path (durable_mutation_mutation…)
```

## Offline gold analysis (§54-§55, eval only)

18/18 gold-primary paths present in AgentActivity. Gold-primary vs all other
paths (per-path means): observations 105 vs 20, explicit reads 35.6 vs 3.9,
final-answer mention-sessions 21.6 vs 1.2 — a real signal. **Leakage caveat**
(§55): the corpus repeats 20 questions ×3 reps ×4 treatments, so counts reflect
benchmark repetition, not universal usefulness. Schema/behavior validation
only — not a ranking benchmark, no memory simulation (§56).

## Performance (§88)

```text
full rebuild   0.37s / 31,814 events (~86k ev/s)   peak RSS 41.6MB
incremental    50 late events -> 50 sessions, 50 episodes rebuilt, 0.34s, 44MB
               no-change re-derive: 0 folded, 0 rebuilt
lookup         episode ~70-95ms/CLI-invocation (process spawn + store load;
               in-process map get sub-ms); activity similar (index ~1MB)
storage        raw canonical 30.0MB; derived 34.1MB (1.13x)
               bytes/investigation 81KB   bytes/path 1.4KB
               (episodes embed full per-path exposures = dominant cost;
                a leaner variant could reference the activity index)
scale est      1M sessions -> ~126GB raw events, ~81GB episodes, ~2.8GB activity
               (planning data, not a gate)
```

## Anti-pattern audit (§71) — all absent

No full raw store loaded for a normal update (per-session tail); no raw scan
for an activity lookup (persisted index); no ATIF parse / repo scan / git at
lookup (git only at build for `ls-files` known-path set); no ranking/System One
changes. Runtime-neutral: no `devin`/`atif`/etc. in `src/derived/` (§70).

## §90 answers

1. **Episode from AgentEvent alone?** Yes — fully rebuilt from canonical events.
2. **Incremental update?** Yes — tail-only fold + per-investigation rebuild.
3. **Late task_outcome on old investigation?** Yes — appends to session tail,
   reprocesses, rebuilds that episode (test `late_event_incremental`).
4. **Multi-session no double-count?** Yes — distinct session/investigation
   sets; per-path counters additive (test `multi_session_investigation`).
5. **Search vs read distinguished?** Yes — separate per-kind counters.
6. **Activity without raw scans?** Yes — persisted per-path index, map lookup.
7. **Conservative mention→observed join?** Yes — exact path match + join to
   observed/read state + unresolved bucket.
8. **Missing for usefulness judgment?** Still need: which RepoDex results were
   *surfaced* (ContextArtifactExposure — the RDX1 packet isn't in ATIF, §51
   gap), an external quality/correctness outcome source, and per-path
   staleness binding to current code.
9. **Ready for Investigation Memory?** Yes — all factual evidence (surfaced/
   observed/read/mentioned/outcome/cost/staleness) is derivable and stored.
10. **One next experiment?** A bounded, leakage-controlled memory-prior probe:
    hold out one repetition of each question, build a prior from the rest, and
    measure whether `mentioned+observed` evidence predicts correct answers —
    offline only, before any ranking use.

## Artifacts

`docs/evals/investigation-activity-v1/` (spec, EPISODE_REPLAY, SOURCE_EXPOSURE,
AGENT_ACTIVITY, PERFORMANCE, SUMMARY, REPORT). Spec `docs/INVESTIGATION_
EPISODE_V1.md`. Derived replay store (outside Git): `/tmp/derived`.

```text
SOURCE_PROMPT_ID: REPODEX-INVESTIGATION-EPISODE-AGENT-ACTIVITY-FOUNDATION-V1
PREVIOUS_PROMPT_ID: REPODEX-AGENT-EVENT-TELEMETRY-FOUNDATION-V1
REPORT_DATE_TIME: 2026-09-23 08:14:20 Europe/Riga
```
