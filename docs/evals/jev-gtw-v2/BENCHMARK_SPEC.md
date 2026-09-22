# Jev × RepoDex GTW Benchmark V2 — spec

- Corpus: `gpt-tunnel-gateway` @ `f09d0834d125362d63e1d12b361ad34c63e50d8d`,
  detached read-only worktree `/tmp/gtw-bench` (944 files, 781 Go, ~4.17MB).
- Reuses V1 graph artifacts (`/tmp/gtw-eval/graph`, 55549 nodes / 111909 edges).
- Model `jev-latest` → resolved **`jev-1.13.0`** on every call (no drift).
- `system-one-v1`, `https://api.typesafe.ai/v1/systemone`, bearer, timeout 60s.
- 32k context model, soft budget 28k — max observed request 1861 in-tok (~5.7%).
- Scored: ranked, `max_results=20`, no truncation, no explicit intent.
  20q × 4 modes × 5 reps = 400 executions, sequential.
- Modes via `repodex config`: A `enabled=false`; B query=jev only; C rerank=jev
  only; D both. Verified via `system-one status` before each mode.
- Tracing `REPODEX_SO_TRACE` per run → `system_one_calls.jsonl` (role, batch,
  configured+resolved model, wall ms, in/out tokens, questions, status,
  fallback, `detail` = offered plans + answer for query role).
- Grading fixed before results: 3=primary symbol in primary file; 2=primary
  file; 1=supporting; 0=unrelated. Frozen `questions.json`/`ground_truth.json`.

## V2 architecture changes (this run)
- Query role: seed-first — deterministic top-5 seeds → bounded target-bound
  plans (find / related|callers|callees bound to each decl seed) → Jev `choice`
  of opaque `pN`. `not_needed` when ≤1 plan. (commits 76b20ce, ddf4520)
- Rerank: eligibility guard — skip on 0/1 results, exhaustive, or lone
  exact-identifier match at rank #1 (`not_needed_*`). Batch 20/request.
- Selective-rerank exact-match guard fixed V1 `TaskExecutionDispatch` 1→6.

## Real-Agent phase (Part VI)
- True zero-context `run_subagent` agents (`subagent_explore`: read-only
  grep/glob/read, cannot invoke RepoDex) on `/tmp/gtw-bench` only.
- Treatments: E0 native (no packet), E1 = A-mode RDX1, E2 = C-mode RDX1,
  E3 = D-mode RDX1 (packet read from `/tmp/agent-pkts/nav-<mode>-<q>.rdx`).
- **Reduced scale**: 6 questions (L1-Q4,L1-Q5,L2-Q1,L2-Q3,L3-Q2,L4-Q1) × 4
  treatments × 1 rep = 24 agents. Single rep; tool metrics self-reported by the
  agent (no harness tool-trace or token usage is exposed by run_subagent).

Raw traces (not committed): `/tmp/repodex-jev-gtw-v2/raw/` —
`retrieval_runs.jsonl`, `system_one_calls.jsonl`, `query_plans.jsonl`,
`agent_runs.jsonl`, `batching_experiment.jsonl`, `results/`, `calls/`.
