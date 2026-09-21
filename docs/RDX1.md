# RDX1 — RepoDex Exchange format, version 1

RDX1 is the compact, deterministic, line-oriented **agent-facing** projection
of a `repodex query` result. It is the default `query` output; `--json` and
`--human` remain available. It is designed to be cheap for a downstream model
to read while staying byte-deterministic and human-inspectable.

```text
#RDX1 v1
Q auth token
F 1 declaration decl:auth/token.go#0 ValidateToken
F 2 call call:app/run.go#1 auth.ValidateToken
R call_candidate 2 1 c cs=cand-9c2
S shown=1 total=1 complete=1 seeds=1 related=1 candidates=1
```

## Grammar

Each line is a record. Whitespace-separated fields; values containing
whitespace are `"`-quoted with `\"`/`\\` escapes.

| record | fields |
|--------|--------|
| `#RDX1 v1` | version header — always first |
| `Q <query>` | the raw query text (one) |
| `F <lid> <kind> <key> <label>` | define node `<lid>` |
| `R <rel> <src> <tgt> <ev> [cs=<set>]` | a relationship edge |
| `S <k>=<v> ...` | summary/completeness — always last |

### `F` — fact/entity definition

Maps a compact response-local integer id `<lid>` (1, 2, 3, ...) to a graph
node. `<kind>` is the node kind (`file`, `declaration`, `entity`, `import`,
`call`). `<key>` is the canonical graph key (`decl:path#id`, `file:path`, ...);
`<label>` is the human name. Local ids are assigned in canonical key order and
are stable for a given result — they are *not* the persistent `gn-` ids.

### `R` — relationship

`R <rel> <src> <tgt> <ev> [cs=<set>]`:

- `<rel>` — the graph edge kind: `contains`, `member_of`, `call_candidate`,
  `import_path`, `use_path`, `package_membership`, `module_file`, ...
- `<src>`, `<tgt>` — the `F` local ids, in true edge direction (a candidate
  caller is `call -> decl`, i.e. `R call_candidate <call> <decl> c`).
- `<ev>` — evidence class: **`f`** = FACT, **`c`** = CANDIDATE. This field is
  mandatory and is what keeps a candidate call from ever looking like an exact
  call.
- `cs=<set>` — present on `call_candidate` edges; the candidate-record id.
  Edges sharing a `cs` are one MultipleCandidates set — alternative targets,
  not independent confirmed calls.

### `S` — summary

Key=value fields describing coverage. Always present:

- `shown` — seeds emitted; `total` — total seed matches (the full logical
  enumeration before presentation truncation); `complete` — `1`/`0`.
- `seeds` — seed count; `related` — expanded relation count;
  `candidates` — number of CANDIDATE relations;
  `unresolved`/`out_of_scope` — related *call* nodes carrying a
  NoCandidate/OutOfScope disposition (they exist, with no target edge).
- On truncation: `S truncated=1 reason=<budget|limit>`.

`complete=0` means the *presentation* was truncated by a budget/limit — never
that the underlying enumeration is unknown. `total` stays available.

## Candidate-set atomicity

A `MultipleCandidates` set is **atomic under the output budget**: a `cs=` group
is emitted whole or not at all — the engine expands relations only from emitted
seeds, so a partial alternative can never be presented as if it were a lone
`SingleCandidate`. Truncation is reported via `complete=0`/`truncated=1`, not a
smaller-looking set.

## Field semantics (non-overlapping)

`seeds` = lexical seed matches; `related` = graph-expanded neighbors;
`candidates` = CANDIDATE relations among them; `shown` = emitted seeds;
`total` = all seed matches before any limit/budget. These never conflate.

## Determinism

Identical `(graph, query, options)` produce byte-identical output. Local ids
are assigned by canonical key order — no absolute paths, no timestamps.

## Forward compatibility

Consumers must ignore unknown record types and unknown `S` keys. The `#RDX1`
version header is bumped on incompatible grammar changes.

## Token efficiency

RDX1 vs the equivalent JSON output is measured in the benchmark script
(`scripts/rdx1_bench.py`); see `docs/TASK5B_RESULTS.md`. Local integer ids and
omitting repeated paths/`gn-` prefixes keep `R` records short.
