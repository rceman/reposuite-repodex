# TASK 5A — Findings

Investigation-graph foundation. `T5A-F001`….

## T5A-F001 — GRAPH_MODEL — the graph is a pure projection

Nodes/edges are derived only from persisted artifacts (snapshot facts, TASK 3B
links, candidate records). The builder never reparses source, never runs a
compiler/analyzer, and never resolves names itself — it only re-expresses
already-decided relationships. `T5A` adds no resolution semantics.

## T5A-F002 — UNCERTAINTY — FACT and CANDIDATE are disjoint and total

Every edge is exactly one `EvidenceClass`. `call_candidate` edges are always
`candidate`; containment/membership/structural links are always `fact`. The
independent audit found `CANDIDATE_PROMOTED_TO_FACT = 0` on both corpora. A
candidate path is a *possible* path, never a confirmed route.

## T5A-F003 — IDENTITY — content-derived, root-independent ids

`node_id`/`edge_id` are `gn-`/`ge-` + a 16-hex sha256 prefix over a canonical
key (`file:path`, `decl:path#id`, `ent:entity_id`, ...). No timestamps, no
absolute paths, no randomness — identical upstream bytes give identical ids and
an identical `graph_digest` across roots and output dirs.

## T5A-F004 — UNCERTAINTY — the call site is its own node

`NoCandidate`/`OutOfScope` calls carry their reason as a node `disposition` and
emit no target edge (§14/§39 — no fake unknown-target nodes). On Hugo, 45,172
calls (7,421 no_candidate + 37,751 out_of_scope) are present as nodes with a
disposition but no candidate edge; 0 dangling targets.

## T5A-F005 — PROVENANCE — every edge names its upstream record

Structural edges carry `upstream_id` = the `lnk-` id and the link `outcome`;
candidate edges carry `upstream_id` = `candidate_set_id` = the `cand-` record
id and the cardinality. `BROKEN_PROVENANCE = 0` in the audit.

## T5A-F006 — GRAPH_MODEL — package_membership kept, member_of added

TASK 3B `package_membership` links (package-clause decl -> entity) are projected
as edges; a separate `member_of` edge (file -> entity) is added from entity
`files`. They are distinct relationships (the clause *declares* the package;
the file *belongs to* it) — both preserved.

## T5A-F007 — TRAVERSAL — bidirectional adjacency, bounded DFS

`GraphIndex` builds outgoing/incoming adjacency maps at load — `outgoing`,
`incoming`, `neighborhood` and `traverse` are O(edges-incident). Traversal is
bounded (`max_depth`/`max_nodes`) and visited-set based; cycles terminate. All
output is canonically ordered.

## T5A-F008 — ARTIFACT — ~0.39× snapshot size, JSONL retained

Hugo graph = 30.9 MB vs 59 MB snapshot + 23 MB candidates (nodes reference
upstream locators, not embedded records). bytes/node ≈ 434, bytes/edge ≈ 375.
JSONL remains acceptable; interned strings / binary encoding is a documented
future compaction direction, not done here (§41/§51).

## T5A-F009 — PERFORMANCE — build ~1.9 s, verify ~0.35 s on Hugo

Hugo: 71,119 nodes / 82,321 edges in ~1.9 s, 243 MB peak RSS; verify 0.35 s /
88 MB. tokio: 44,839 nodes / 45,462 edges in ~1.05 s. No hard budget; the
per-file snapshot load dominates build time.

## T5A-F010 — RDX1_DESIGN / SEMANTIC_QUERY_BOUNDARY

`gn-`/`ge-` ids and `candidate_set_id` grouping are RDX1-projection-ready.
Semantic query planning, Jev ranking and `query "…"` are explicitly future
layers — this task implements only deterministic graph retrieval.
