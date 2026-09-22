# Jev × RepoDex GTW Benchmark V1 — spec

- Corpus: `gpt-tunnel-gateway` @ `f09d0834d125362d63e1d12b361ad34c63e50d8d`
  (detached worktree `/tmp/gtw-bench`, read-only; active checkout untouched).
- Pipeline (built once, reused for all modes): snapshot 9056 decls / 781 Go
  files (~4.36 MB) in ~2.7 s; links ~4.8 MB in ~1.1 s; candidates 42503
  records in ~1.8 s; graph 55549 nodes / 111909 edges / 34.4 MB in ~2.3 s.
- Graph SHA `25911db2…`, links fingerprint `0092870a…`.
- Model: configured `jev-latest`, resolved **`jev-1.13.0`** on every one of
  600 calls (no mid-run drift).
- Protocol: `system-one-v1`, `https://api.typesafe.ai/v1/systemone`, bearer.
- Scored run: ranked mode, `max_results=20`, no token truncation, **no
  explicit intent** (so the query role is exercised). 5 reps × 4 modes × 20 Q
  = 400 RepoDex executions.
- Modes configured via `repodex config` (real product config, not faked):
  A `enabled=false`; B `roles.query=jev` only; C `roles.rerank=jev` only;
  D both. Verified via `system-one status` before each mode's samples.
- Tracing: `REPODEX_SO_TRACE` per run → `system_one_calls.jsonl` (role, batch,
  configured+resolved model, wall ms, input/output tokens, questions, status,
  fallback). No auth/token recorded.
- Grading (fixed before results): 3 = primary symbol in primary file; 2 =
  primary file; 1 = supporting path/symbol; 0 = unrelated. Jev scores never
  become ground truth.
- Gold labels: `questions.json` + `ground_truth.json` derived from the pinned
  source before inspection; frozen.

Raw traces (large, not committed): `/tmp/gtw-eval/raw/` —
`runs.jsonl`, `system_one_calls.jsonl`, `results/<mode>-<q>-r<rep>.json`,
`calls/<run>.jsonl`.
