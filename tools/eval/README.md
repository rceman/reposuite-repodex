# RepoDex benchmark evaluation harness

Authoritative, committed evaluation tooling for RepoDex Agent benchmarks.
Reproduces the smoke campaign deterministically. See BENCHMARK_SPEC.md.

## One-command reproduction

    python3 tools/eval/run_smoke.py --binary target/debug/reposuite-repodex \
        --workdir <scratch-dir>

This reads the committed TASK_CORPUS + GOLD_CONTRACT + SESSION_SCHEDULE,
runs each scored session in a disposable benchmark worktree under
`--permission-mode dangerous`, writes full-fidelity traces + artifacts.

## Files

- `run_smoke.py`   — runner: schedule, invoke devin, worktree setup/reset
- `tracer.py`      — full-fidelity trace export (tool results, per-call tokens)
- `validator.py`   — obligation-based + worktree validation, status model
- `metrics.py`     — token/tool/timing accounting + artifact generation
- `gold.py`        — gold loader + source-proof audit
- `classify.py`    — exec-content tool classification (repodex vs search vs build)
