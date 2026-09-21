# Autonomous mission findings — deterministic query -> RDX1 -> pre-System-One

`M-F001`….

## M-F001 — GRAPH_MODEL — `decl -> call` containment added (policy v2)

`callees(fn)` needs the call sites *inside* a function. A
`function/method -> call` `contains` FACT edge was added via innermost
range-containment, raising `GRAPH_POLICY_VERSION` to 2. This is a projection of
existing declaration ranges + call ranges — no new resolution semantics.

## M-F002 — IDENTITY — digest coverage gap found and fixed (schema v2)

The original canonical graph digest covered only `(node_id, kind, key,
disposition)` for nodes and `(edge_id, kind, evidence, src, tgt)` for edges —
a tampered node `label`/`path` or edge `rule_id`/`candidate_set_id` would not be
detected. The digest now covers all of these (`GRAPH_SCHEMA_VERSION` = 2), and
`GraphIndex::load` runs a self-integrity check so a corrupt graph cannot be
silently queried.

## M-F003 — TRAVERSAL — bounded primitives over adjacency maps

`find`/`callers`/`callees`/`paths`/`neighborhood` use the loaded index's
adjacency maps — O(edges-incident), no repository-wide scan per query. `paths`
is bounded BFS (depth/paths/nodes caps, visited-set, no repeated edge per path)
that terminates on cycles. Hugo callers/callees/paths each run in ~1.5 ms warm.

## M-F004 — QUERY — deterministic lexical+graph contract, not an LLM

`QueryPlan` is a typed inspectable structure (terms, ranked|exhaustive, intent,
target, depth, budget). Seed lookup uses the deterministic identifier splitter
over a term index — never source reparsing, comments or fuzzy matching.
Ranking is documented integer factors; relevance is separate from evidence
class. `--explain` exposes terms/mode/intent/seed factors/completeness.

## M-F005 — UNCERTAINTY — completeness is honest

`exhaustive` never silently drops a match; a budget/limit truncation emits
`complete=0` + `total`. `ranked` may cap by score/budget. No fake
unknown-target nodes; `NoCandidate`/`OutOfScope` calls carry dispositions.

## M-F006 — RDX1 — compact protocol, ~0.5–0.6x JSON bytes

RDX1 assigns small canonical-order local ids so `R` lines stay short; `f`/`c`
encodes FACT/CANDIDATE and `cs=` preserves candidate sets. Hugo callers query:
RDX1 2,810 B vs JSON 4,717 B vs human 1,484 B (human omits cs ids/keys). On a
larger result RDX1 is ~0.47x JSON. Emit->parse round-trips; byte-deterministic.

## M-F007 — STORAGE — graph ~0.48x upstream, no query artifact

Hugo: graph 38 MB vs snapshot 56.6 + candidates 22 MB. The query layer adds no
persisted artifact — its term index is built in memory per query (~0.29 s cold
load+index+run, ~107 MB RSS on the 71k-node Hugo graph). Persisting the term
index was evaluated and deferred: at this scale the in-memory build is cheap
enough that a durable index artifact is not justified (a future compaction
task could intern strings / binary-encode if corpora grow).

## M-F008 — IDENTITY — cross-root + update-vs-fresh hold

Graph digest is root-independent; identical upstream bytes -> identical graph
and identical query output (determinism + update-vs-fresh fixtures pass).

## M-F009 — BOUNDARY — System One not implemented; documented only

No Jev client, HTTP provider, model config or inference exists. The future
`[system_one]` named-model config (protocol/url/model/timeout/auth + query/
rerank roles) and the failure-fallback + exhaustive-model invariants are
documented in `docs/PRE_SYSTEM_ONE_REVIEW.md`, not implemented.
