# Benchmark Harness & Trace Correction V1

Fixes the review-identified harness defects. Executor mode `--permission-mode
dangerous` scoped to the isolated benchmark workspace gives zero tool
rejections; traces preserve every observable event (full tool results, per-call
tokens, timing); validation is obligation-based; code tasks validated from the
actual disposable worktree.
