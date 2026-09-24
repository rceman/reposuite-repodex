# Repository vs RepositoryView

RepoDex separates the **logical repository** from the **exact source tree
being queried**:

```text
Repository / Project     logical source-repository identity
RepositoryView           the exact effective filesystem source tree
```

A `RepositoryView` may be a normal checkout, a Git worktree, a different
branch, a detached or dirty checkout, a temporary task checkout, or a future
sandbox. RepoDex **never** infers the authoritative view from the process cwd
or a session repository (§2-§3).

## Locator forms (§4-§9)

Exactly one locator names the view — `root` XOR `project`:

```bash
# Form A — exact filesystem root (the intentional escape hatch, §18)
reposuite-repodex query --root /abs/path/to/worktree --query "where is auth checked?"

# Form B — registered project + view
reposuite-repodex query --project GTW --view GTW-TSK573 --query "..."

# Form B default — registered root checkout (only when one exists, §7)
reposuite-repodex query --project GTW --query "..."
```

`--root` and `--project` are mutually exclusive (`LOCATOR_CONFLICT`); `--view`
without `--project` is invalid. An invalid locator **fails** — RepoDex does not
silently fall back to cwd or another checkout (§10).

Public concepts are `project` / `view` / `root`; `worktree`, `base index` and
`overlay` are internal implementation details, not the public abstraction (§8).

## Project registry (§11-§15)

Durable registry at `~/reposuite/repodex/projects.json` (override with
`REPODEX_STATE_DIR` for tests/isolation). Atomic writes (temp + rename) — a
concurrent reader never sees a partial registry.

```bash
reposuite-repodex project register GTW \
  --root /home/user/src/gpt-tunnel-gateway \
  --views-root /home/user/.local/share/gpt-tunnel-gateway/task-worktrees/gpt-tunnel-gateway
reposuite-repodex project show GTW
reposuite-repodex project list
reposuite-repodex project remove GTW
reposuite-repodex project resolve GTW --view GTW-TSK573   # canonical view metadata
```

A project has an `alias`, an optional default `root` checkout, and
`view_roots` that contain named views. Paths are canonicalized before storage
(no literal `~`).

## View resolution safety (§16-§18)

`--project GTW --view GTW-TSK573` resolves the alias → registered view roots →
`GTW-TSK573` → canonical filesystem view. A view name must be a simple relative
identifier contained inside its view root after canonicalization — `../foo`,
`/foo`, `foo/../../bar` are rejected (`VIEW_PATH_ESCAPE`), and a symlink cannot
escape the configured root. `--root /abs/path` remains the explicit escape
hatch.

## RepositoryView metadata (§19-§20, §29)

Resolution derives generic metadata:

```text
repository_id  view_id  canonical_root
head  branch?  detached  dirty  git_common_dir?
view_fingerprint   locator
```

`branch` is **metadata only** — branches move and views may be dirty. Source
identity is the `view_fingerprint` (digest over the effective path→content
manifest), never the branch name.

## Content-addressed correctness (§21-§28, §38)

Each view's source is fingerprinted by `path → content_digest`. The derived
index lives under `indexes/{fingerprint}/`, so:

- **Same path, different content** → different digests → different fingerprint
  → independent index. `CROSS_VIEW_CONTAMINATION = 0` (§21).
- **Dirty / added / deleted** → changed manifest → new fingerprint → fresh
  index for the new content (§22, §25).
- **Different branches** → different fingerprints → correct per-view results
  (§24).
- **Identical content across N worktrees** → same fingerprint → **one shared
  index**; incremental cost is a per-root manifest cache only — N worktrees do
  not multiply disk by N (§76).

`FileAnalysis` objects are content-addressed in a shared `objects/` store keyed
by `object_key(path, digest)` — identical files across different views/contents
are parsed once and reused (§27). A view is **not** assumed to be `main HEAD +
dirty overlay`; the model is an effective-manifest + content-addressed analysis
that is correct for arbitrary branches (§28).

Edit→visible latency is one manifest recompute (walk + stat; only changed files
rehashed) plus incremental rebuild — no polling daemon (§23).

SymbolExposure's exact-version rule is preserved: `SourceObserved + exact file
content digest → SymbolExposure from that exact source version`, and the
content-addressed objects guarantee a view never serves another view's symbol
ranges (§38).

## Machine query contract (§30-§33, §44-§45)

```bash
echo '{"schema":"reposuite.repodex.query.request.v1","root":"<abs>","query":"..."}' \
  | reposuite-repodex query --json
echo '{"project":"GTW","view":"GTW-TSK573","query":"..."}' | reposuite-repodex query --json
```

Reads one request object from stdin, resolves the view, runs the query, and
writes machine-readable JSON **only** to stdout (diagnostics → stderr). The
request needs no HEAD/branch/dirty/index path — RepoDex discovers those.
Response: `reposuite.repodex.query.response.v1` carrying `repository_view`,
`query`, and the canonical structured `result` (seeds/related — not an RDX
round-trip). Deterministic error codes on stdout + non-zero exit:
`INVALID_REQUEST LOCATOR_CONFLICT PROJECT_NOT_FOUND VIEW_NOT_FOUND
VIEW_PATH_ESCAPE ROOT_NOT_FOUND NOT_A_REPOSITORY QUERY_FAILED`.

`--root` and `--project --view` that resolve to the same path produce
semantically identical query results (§35).

## Future Gateway boundary (§40-§42)

```text
Agent → Gateway MCP task/code → Task authority → project + RepositoryView
      → RepoDex machine query → compact code intelligence
```

The ideal Agent-facing request is just `{"query": "..."}`; Gateway maps Task
authority → `project`/`view`. RepoDex stays harness-neutral — it never sees
`Task`, `Worker`, `Planner`, `task/code`, or `MCP`; it receives a generic
RepositoryView locator.

## Benchmark KPI contract

See `docs/evals/BENCHMARK_METRICS.md` — Correctness / Cost / Work / Time are the
four first-class dimensions; `output_tokens` and per-stage time are first-class
(input tokens are never the sole headline).
