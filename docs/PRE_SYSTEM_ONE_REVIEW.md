# PRE-SYSTEM-ONE Review

RepoDex is a **deterministic, model-free** repository investigation engine. It
is fully usable with **no API keys, no network, no models**. A future System
One model is an optional navigation accelerator only — it must never become
repository truth.

## Architecture layers

```text
source -> language detect -> Tree-sitter parse -> normalized facts   (TASK 3A)
       -> structural links (module/package topology)                  (TASK 3B)
       -> lexical bindings                                            (TASK 3C)
       -> bounded call candidates (Rust T3E, Go T4A-D)                 (frozen)
       -> investigation graph (nodes/edges, FACT vs CANDIDATE)        (TASK 5A)
       -> deterministic primitives (find/callers/callees/paths)       (Phase A)
       -> deterministic query engine (QueryPlan/rank/exhaustive)      (Phase B)
       -> RDX1 compact agent protocol                                 (Phase C)
       -> hardened, integrity-checked, no-network CLI                 (Phase D)
```

## Language / structural / candidate support

| language | facts | structure | call candidates |
|----------|-------|-----------|-----------------|
| Rust     | yes   | modules/use | package-local + qualified-path (frozen) |
| Go       | yes   | module/package + local bindings | package-local + imported `pkg.Func` (frozen) |
| Python/PHP | facts + structural nodes only | — | none (not invented) |

No cross-language call inference. No method/interface/external-package
resolution — documented out of scope.

## Graph capabilities

Nodes: `file`, `entity` (package/module), `declaration` (typed: function/method/
type/const/var), `import`, `call`. Edges: `contains` (file->fact and
function/method->call), `member_of`, structural-link kinds, `call_candidate`.
Every edge is `FACT` or `CANDIDATE`; a candidate call is never an exact call.

## Query capabilities

Deterministic `query "<text>"`: typed inspectable `QueryPlan`, lexical seed
lookup over a term index (decl/entity/file-path names), bounded neighborhood
expansion, integer ranking, `ranked`/`exhaustive` modes, `--explain`,
intents (`find`/`callers`/`callees`/`related`/`paths`). `graph` subcommands
expose find/callers/callees/paths/neighborhood directly.

## Exact vs candidate / exhaustive vs ranked / completeness

FACT and CANDIDATE are disjoint and mandatory on every edge (RDX1 `f`/`c`).
`exhaustive` never silently drops a match; truncation reports `complete=0` +
`total`. `ranked` orders by documented integer factors. Relevance score is
ordering metadata, never certainty. Candidate sets keep their `cs=` identity.

## Artifacts

```text
Hugo   snapshot 56.6 MB  candidates 22.0 MB  graph 39.8 MB
tokio  snapshot ~?       candidates ~?       graph 23.6 MB
```

Graph ~0.48x upstream; nodes reference upstream locators, not embedded records.
No separate query artifact — the term index is in-memory (~0.29 s cold
load+index+query, ~107 MB RSS on Hugo).

## Performance

```text
graph build  Hugo ~2.6 s (243 MB peak RSS)   tokio ~1.5 s
graph verify Hugo ~0.35 s
query        Hugo ~0.29 s cold / ~1.5 ms warm primitives
```

## Determinism / correctness

Byte-identical graph + query output across rebuilds and absolute roots;
update-vs-fresh equal; stale upstream rejected via manifest digests; corrupt
graph rejected on load. Independent audits (re-derive, no builder code) on
Hugo + tokio: `MISSING_NODE/EXTRA_NODE/WRONG_EDGE_TARGET/WRONG_EDGE_CLASS/
CANDIDATE_PROMOTED_TO_FACT/BROKEN_PROVENANCE = 0`. 586 tests pass.

## Storage debt / query limitations

JSONL is verbose (a future interned/binary compaction is documented, deferred).
No persisted query index yet — justified by the fast in-memory build; revisit
if corpora grow. No comment/docstring/content search (no persisted index for
it). No semantic intent inference — intents are explicit flags, not an NLP
classifier.

## Remaining BLOCKER/HIGH

None.

## Future System One boundary (NOT implemented)

A later `[system_one]` config may define named models — `protocol`, `url`,
`model`, `timeout_ms`, `auth.{type:none|bearer|header,token,header}` — and
independently-assignable roles `query` and `rerank` (a future `expand` is
possible). Jev would be the first hosted implementation, not a runtime
dependency; no model code is hardcoded into RepoDex core.

- `query` role: may turn user language into the existing typed `QueryPlan`;
  model output is validated against the plan schema and can never create
  repository facts/edges.
- `rerank` role: may order/group/prioritize already-retrieved entities; for
  `exhaustive` queries it must NOT reduce the set then claim completeness.
- Failure fallback: disabled / bad key / timeout / HTTP failure / unavailable /
  malformed -> deterministic RepoDex behavior. Model availability is never a
  prerequisite for correctness. RepoDex owns enumeration, completeness and
  uncertainty; the model owns optional presentation/navigation.
