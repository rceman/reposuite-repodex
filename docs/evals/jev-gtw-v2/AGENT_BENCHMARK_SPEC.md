# Instrumented Native-Agent Benchmark — spec

Real-Agent phase of the V2 GTW benchmark. Frozen corpus + packets; this phase
only adds genuine zero-context instrumentation.

## Harness

Each scored run is a fresh **zero-context** `devin -p` (non-interactive print)
process — a brand-new agent session (`session_id` minted per run, no parent
context, no conversation inheritance). Not a fork of the orchestrating Worker.

```bash
cd /tmp/gtw-bench
devin -p --prompt-file <run>.txt --export <run>.json --respect-workspace-trust false
```

Telemetry comes from the **ATIF-v1.7 export** (real harness/provider data):
- `final_metrics.total_prompt_tokens`   → `agent_input_tokens`   (real)
- `final_metrics.total_completion_tokens`→ `agent_output_tokens` (real)
- `final_metrics.total_cached_tokens`   → cached subset
- `steps[].tool_calls[].function_name/arguments` + `steps[].observation.results[].content`
  → real tool trace + real tool-output bytes + file paths
- `steps[].timestamp` → wall/step timing; `steps[].metrics` → per-call tokens

So agent input/output tokens and the tool trace are measured at the harness
layer — no Agent self-report is used for primary metrics.

## Design

- 20 questions × 4 treatments × 3 reps = **240** runs, deterministic
  counterbalanced order (treatment rotated by question index), modest
  concurrency (4 workers) — sequential-ish, not a throughput test.
- Corpus: GTW `f09d083` read-only worktree `/tmp/gtw-bench` only. No web.
- Frozen packets (single packet per q×treatment, sha recorded):
  E1=`A-*.rdx` deterministic; E2=`C-*.rdx` selective rerank; E3=`D-*.rdx` full.
  E0 gets no packet. Packets are inlined in the prompt → counted in input tokens.
- Same model (`gpt-5-6-luna`), same instruction, same answer format for all
  treatments; only the packet differs. Agent is not told its treatment.

## Common instruction (abridged)

Investigate the pinned repo, cite real paths/symbols, no guessing. Prohibited:
`exec`, `web_search`, `webfetch`, `run_subagent`, `ask_user_question`, any
repodex/reposuite tool. Output `ANSWER:` + `EVIDENCE:`.

Assisted runs add neutral wording: "a precomputed repository retrieval packet
is provided as a navigation aid… verify against source."

## Validity audit (per run)

Invalid → rerun in a new zero-context session:
- any banned tool (`web_search/webfetch/run_subagent/mcp/ask_user_question`)
- any `exec` invoking repodex/reposuite (E0 especially; E1–E3 must not re-query)
- timeout / missing export

## Scoring

Evaluator-side only (`answer_gold.json`): RequiredFactRecall (required fact
atoms), PrimaryEvidenceCoverage (primary paths cited), plus unsupported /
contradicted / invalid-path-symbol counts. Evidence-progress metrics
(tool-calls/reads/observed-files to first+full primary) computed from the real
tool trace.

Raw: `raw/exports/*.json`, `agent_run_index.jsonl`,
`instrumented_agent_{runs,tool_calls,usage,answers,scoring}.jsonl`.
