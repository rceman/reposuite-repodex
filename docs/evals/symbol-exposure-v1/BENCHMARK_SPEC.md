# Symbol Exposure V1 — Benchmark / Replay Spec

This document describes how the SymbolExposure foundation was exercised on the
240-session historical corpus. **No Agents were run** — this is offline
derivation + replay of already-collected `source_observed` telemetry.

## Inputs

| Input | Location | Notes |
|-------|----------|-------|
| ATIF exports (240 sessions) | `/tmp/repodex-jev-gtw-v2/raw/exports/*.json` | Frozen zero-context GTW runs (E0–E3 × L1–L3 × reps) |
| Pinned corpus worktree | `/tmp/gtw-bench` | `gpt-tunnel-gateway` @ `f09d0834` — read-only |
| RepoDex index snapshot | `/tmp/gtw-eval/snap` | `manifest.json` + `files/<object_key>.json` FileAnalysis per file |
| Canonical AgentEvents | `/tmp/repodex-symbol-exposure-v1/canon/*.jsonl` | Regenerated with `file_content_digest` |
| Gold answers (eval only) | `/tmp/repodex-jev-gtw-v2/answer_gold.json` | `primary_paths` + `symbols` per question |

## Pipeline

```text
ATIF export
  └─ scripts/adapters/devin-atif.py            (harness-side, prototype)
       emits canonical source_observed with path, line range,
       fragment digest AND whole-file sha256 (file_content_digest)
  └─ agent-events ingest                       (canonical store)
  └─ symbol-exposure derive
       --store <events> --snapshot <index> --output <dir>
       for each source_observed:
         resolve (path, file_content_digest) -> symbol map
           · digest == indexed file digest  -> reuse indexed FileAnalysis (§9)
           · unseen digest + bytes          -> parse once, cache by digest (§10)
           · unseen digest, no bytes        -> unresolved_missing_source_version
           · no digest                      -> assumed_indexed (read-only corpus)
         overlap observed line range with declaration/reference/call ranges
           -> SymbolExposure { enclosing_declaration | declaration_occurrence
                              | reference_occurrence } + FACT/CANDIDATE
  └─ memory build --symbol-exposure <dir>      (enrich entries with symbol evidence)
```

## Exact source-version binding (§46)

The GTW corpus is a **pinned, immutable** worktree, so the whole-file digest of
each observed file equals the indexed `IndexedFile.content_digest` for every
supported file. The adapter computes the digest at conversion time against that
pinned tree. This is valid **only** for the immutable replay corpus — a mutable
harness must capture `file_content_digest` at observation time (the adapter
documents this limitation).

## Treatments (§48 breakdown dimension)

- `E0` — no memory/context (native exploration)
- `E1` — deterministic RDX1 retrieval packet
- `E2` — Jev-reranked packet
- `E3` — retrieval + memory hints (file-level)

## Reproduce

```bash
# canonical events (adapter emits file_content_digest)
for f in raw/exports/*.json; do
  python3 scripts/adapters/devin-atif.py --input "$f" \
    --investigation-id "$(basename "$f" .json)" \
    --repo-root /tmp/gtw-bench --repository-id gpt-tunnel-gateway \
    --repo-head f09d0834d125362d63e1d12b361ad34c63e50d8d > canon/$(basename "$f" .json).jsonl
done
cat canon/*.jsonl | reposuite-repodex agent-events ingest --output events -

# symbol exposure + symbol activity
reposuite-repodex symbol-exposure derive \
  --store events --snapshot /tmp/gtw-eval/snap --output derived --json

# symbol-enriched memory (file-level preserved, symbols added)
reposuite-repodex memory build --store /tmp/derived2 \
  --output /tmp/sym-memory --repo-root /tmp/gtw-bench \
  --symbol-exposure derived --full --json
```

## Inspection CLI

```bash
reposuite-repodex symbol-exposure investigation <inv> --output derived --json
reposuite-repodex symbol-exposure source <session:seq> --output derived --json
reposuite-repodex symbol-exposure activity [symbol] --output derived --json
reposuite-repodex symbol-exposure stats --output derived --json
reposuite-repodex memory query --output /tmp/sym-memory --repo-root /tmp/gtw-bench --json "<q>"  # shows symbols
reposuite-repodex memory show <inv> --output /tmp/sym-memory --json
```

## Scope limits honoured

- No 300-Agent benchmark run.
- No production ranking / Jev / memory-weight / RDX1 change.
- No RelationExposure. No Relay changes. No push.
- Symbol enrichment is additive debug/offline evidence — not a ranking input.
