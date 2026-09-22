# Jev V2 + Instrumented Native-Agent GTW Benchmark — Report

```text
INSTRUMENTED_NATIVE_AGENT_BENCHMARK_COMPLETE
REPODEX_SYSTEM_ONE_PRODUCT_DECISION_READY
ZERO_CONTEXT_AGENT_ISOLATION = PASS
```

RepoDex HEAD `abd7647`. GTW `f09d083` worktree `/tmp/gtw-bench`. Agent model
`gpt-5-6-luna-xhigh` (identical all treatments). Jev resolved `jev-1.13.0`.

## Isolation provenance (§7)

- expected_sessions = 240 · valid_sessions = 240 · context_inheriting = 0 ·
  reused_sessions = 0 · distinct session_ids = 240 · invalid/rerun = 0.
- Each run = fresh `devin -p` zero-context process (new session, no parent
  context). Counterbalanced order, conc=8.
- Validity audit: across all 240 runs the only tools used were `grep`,
  `find_file_by_name`, `read` — **0 exec / web / repodex / subagent calls**.
  E0 never touched RepoDex; E1–E3 made no live RepoDex calls. No gold leaked.

## Telemetry source

`devin -p --export` ATIF trajectory: `final_metrics` real prompt/completion/
cached tokens; `steps[].tool_calls` + `observation` real trace/output bytes;
`steps[].metrics` per-call tokens; `steps[].timestamp` timing. Harness-measured,
not self-reported.

## Headline results (means over 60 runs each)

| metric | E0 native | E1 RepoDex | E2 +rerank | E3 +full |
|--------|-----------|------------|------------|----------|
| RequiredFactRecall | 0.99 | 0.99 | 0.99 | 0.98 |
| PrimaryEvidenceCov | 0.93 | 0.93 | 0.93 | 0.92 |
| tool calls | 31.3 | 24.7 | 24.1 | 22.2 |
| files read | 15.3 | 13.7 | 14.5 | 11.1 |
| files observed | 193.8 | 75.4 | 76.0 | 87.3 |
| **input tokens** | **471,852** | **391,013** | **356,234** | **321,750** |
| output tokens | 4,199 | 4,158 | 3,805 | 3,490 |
| repo out bytes | 206,047 | 162,893 | 152,163 | 143,025 |
| wall ms | 42,857 | 40,599 | 37,049 | 38,874 |

Totals: input tokens E0 28.3M → E3 19.3M; output ~0.25M each.

## Deltas

- **E1 vs E0**: input −80,839 (−17.1%), tool calls −6.6 (−21.0%), files observed
  −118 (−61%), recall Δ0.00. **RepoDex pays for itself.**
- **E2 vs E1**: input −34,779 (−8.9%), calls −0.7 — selective Jev rerank adds real
  incremental savings.
- **E3 vs E0**: input −150,101 (−31.8%), calls −9.1 (−29%), recall −0.01 —
  narrowest packet → biggest token cut, tiny recall cost.
- E0 input-token variance (6.2e10) is ~3× the assisted modes — RepoDex also
  **stabilizes** investigation.

## Per-level (input tokens)

| level | E0 | E1 | E2 | E3 |
|-------|----|----|----|----|
| L1 lookup | 127k | 136k | 171k | 126k |
| L2 multi-hop | 389k | 318k | 247k | 218k |
| L3 architectural | 685k | 663k | 583k | 501k |
| L4 deep | 686k | 447k | 423k | 442k |

RepoDex helps on **L2/L3/L4**; on trivial **L1** lookups the packet is pure
overhead (E2 worst: 171k) — it can't beat an already-cheap native lookup.

## RDX1 context ROI (agent tokens saved / packet tokens)

- E1 ≈ **7.4×**, E2 ≈ **10.6×**, E3 ≈ **38.4×** (D packet is ~3.9k tok vs ~10.8k
  for A/C). The packet decisively pays for itself in input tokens.

## Jev incremental ROI (E2/E3 over E1)

Jev retrieval phase: 331 calls, 446,002 input tok, ~$0.019 total (~$0.00006/q).
Per assisted run Jev saves a further ~34–70k agent input tokens — an extreme
token ROI, though Jev *cost* is negligible either way.

## Retrieval-proxy validity (§48/Q10)

Pearson between retrieval `rank-of-first-primary` proxy and actual agent
`observed-files-to-first-primary` = **0.078** — no meaningful correlation.
**Cheap retrieval proxies do NOT predict real downstream Agent cost**; the
240-agent instrumented benchmark cannot be replaced by retrieval metrics alone.

## §56 product decisions

1. RepoDex reduce real input tokens? **YES** −17.1%.
2. RepoDex reduce tool calls? **YES** −21%.
3. RepoDex reduce files/source observed? **YES** −61%.
4. Correctness preserved? **YES** (recall 0.99, −0.00).
5. Selective Jev rerank beyond RepoDex? **YES** −8.9% further tokens.
6. Seed-first query role E2E? **reduces tokens most** (−32%) but −1% recall —
   token-efficient yet slightly less accurate.
7. RDX1 cost more than it saves? **NO** — ROI 7–38×.
8. Levels it helps: **L2, L3, L4**.
9. Levels it hurts: **L1** (packet overhead).
10. Proxies predictive? **NO** (r=0.078).
11. RepoDex justified vs native? **YES** on non-trivial questions.
12. Jev on top of RepoDex? **YES** — rerank adds savings at negligible cost.
13. Best tradeoff: **E2 (RepoDex + selective Jev rerank)** — best recall (0.99)
    with −24.5% tokens / −23% calls vs native. E3 saves more tokens but costs
    recall; E1 is the Jev-free floor.

## Verdict

The earlier 24-run pilot (self-reported) suggested RepoDex didn't help — that
was a measurement artifact. With real telemetry, **deterministic RepoDex
clearly reduces real Agent investigation cost** (−17% input tokens, −21% tool
calls, −61% files observed) while preserving correctness, and **selective Jev
rerank adds a further real gain**. RepoDex is justified; keep System One
available (rerank is the productive role); the seed-first query role trades a
little correctness for token savings.

Raw: `raw/exports/*.json` (240 ATIF), `agent_run_index.jsonl`,
`instrumented_agent_{runs,tool_calls,usage,answers,scoring}.jsonl`.
Orchestration: `driver.py`, `collect.py`, `analyze_agent.py`.
