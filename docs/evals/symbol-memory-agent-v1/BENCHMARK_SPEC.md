# Symbol-Aware Memory Zero-Context Agent Benchmark V2 — Spec

Causal measurement: does file-level memory help, does symbol/range memory help
further, and does selective Jev still add value — on fresh zero-context Agents.

## Corpus & questions

- Repo: `gpt-tunnel-gateway` @ `f09d0834d125362d63e1d12b361ad34c63e50d8d`, pinned
  read-only worktree `/tmp/gtw-bench`. No internet.
- Questions: the **frozen 20-question paraphrase set** from the Investigation
  Memory evaluation, reused verbatim (§3) — `/tmp/paraphrases.json`.
- Gold: evaluator-side `answer_gold.json` (required_facts, primary_paths,
  symbols) — never shown to Agents (§51, §75).

## Treatments (§7)

| | packet | memory |
|--|--------|--------|
| T0 | none | none |
| T1 | `packets/T1-{q}.rdx` (deterministic RDX1) | none |
| T2 | same T1 packet | file-memory top-10 paths (`H` lines) |
| T3 | same T1 packet | **same paths+order** + `S name range kind [tags] id=` symbol lines (≤2/path) |
| T4 | `packets/T2-{q}.rdx` (selective Jev rerank) | **byte-identical T3 memory block** |

Primary isolation: **T3 vs T2** (identical paths, +symbols → symbol value).
T4 vs T3 isolates Jev after symbol memory.

## Frozen context (§23, §29-§37)

- `prompts/{q}__{T}.txt` — 100 prompts, byte-identical across the 3 reps.
- Memory path limit: top-10 (fixed, not tuned). Symbol limit: ≤2/path.
- Symbol selection rule (§31, frozen pre-run): structured-evidence mention >
  final-answer mention > declaration_occurrence+explicit_read > innermost
  enclosing+explicit_read > none; tie-break earliest seq then symbol id.
- Memory budget ~1500 tokens; deterministic truncation by path order + priority.
- Neutral wording (§39): "precomputed repository navigation hints — a navigation
  aid only — verify against source". No gold/answer prose/labels in hints.

## Execution (§13-§21)

- **Matched parallel waves**: 60 waves = (question × rep); each wave launches all
  5 treatments concurrently via `devin -p --export` (fresh zero-context session,
  `context_inheritance=false`, same model `gpt-5-6-luna-xhigh-priority`).
- 20 questions × 3 reps × 5 treatments = **300 scored runs**.
- One unscored 5-treatment probe wave first (§17) — validated isolation,
  telemetry, exports; excluded from the 300.
- Driver `driver.py` owns scheduling/retries; transient failures re-run that
  treatment with a new session (§18); invalid attempts preserved separately.
- No online learning — memory snapshot frozen; scored telemetry → separate store.

## Telemetry & analysis (§40-§55)

Per run: `devin-atif.py` → canonical `reposuite.agent-event.v1` → metrics.
- Tokens: `final_metrics` (actual provider usage; verified == canonical
  `model_call_completed` sum).
- Tools: `tool_call_started` by category (search/file_read/dir_list/git/other).
- SourceObserved: files observed/read, snippet/read events, bytes.
- SymbolExposure: `symbol-exposure derive` over the shared store (all 300 runs) →
  per-run symbol metrics + hinted-symbol follow-through.
- Quality: required-fact recall, primary path/symbol coverage, invalid-path,
  unsupported-claim counts vs frozen gold.
- Timing: tool_calls/files/bytes to first primary file.
- Pairwise deltas over (question × rep) paired units (60 pairs) + deterministic
  bootstrap 95% CIs (seed fixed).

## Reproduce

```bash
# context (frozen — regenerate only from the same inputs)
python3 gen_sym_hints.py   # memory_blocks + symbol_hints + selection report
python3 gen_prompts.py     # 100 prompts, asserts T2==T3==T4 invariants

# execute (resumable; probe first)
python3 driver.py --probe  # 1 unscored 5-treatment wave
python3 driver.py          # 60 scored waves @ concurrency 5

# analyze
python3 collect.py         # canon/ + runs_collected.json
cat canon/*.jsonl | reposuite-repodex agent-events ingest --output sexp-store -
reposuite-repodex symbol-exposure derive --store sexp-store \
    --snapshot /tmp/gtw-eval/snap --output sexp --json
python3 enrich_symbol.py   # symbol metrics + follow-through
python3 analyze.py         # pairwise + bootstrap -> analysis.json
python3 report.py          # docs/evals/symbol-memory-agent-v1/*
```

## Scope limits honoured

No production ranking/memory-weight/symbol-weight/Jev tuning; no question/gold
changes; no RelationExposure; no Relay changes; no push; no seed-first/query-role
Jev; benchmark-only context serialization + orchestration under `/tmp`.
