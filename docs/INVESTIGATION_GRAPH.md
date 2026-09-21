# Investigation Graph

A deterministic, source-grounded projection of RepoDex's facts, structural
links and bounded candidates into a traversable node/edge form. It is the
retrieval foundation for a future semantic query layer.

```text
TASK 3A snapshot   -> file / declaration / import / call nodes, `contains`
TASK 3B links      -> entity nodes, `member_of`, structural-link edges
candidate artifact -> `call_candidate` edges + call dispositions
```

The graph is a **projection of existing evidence** — it never reparses source,
never resolves anything itself, and never collapses candidate evidence into
semantic truth.

## Artifact

```text
<graph>/
  manifest.json     # versions + upstream digests + counts + graph digest
  nodes.jsonl       # GraphNode per line
  edges.jsonl       # GraphEdge per line
```

## Evidence classes (mandatory)

Every edge is `fact` or `candidate` (`EvidenceClass`). A `call_candidate` edge
is **never** an exact call edge. `FACT` covers containment, membership and the
resolved TASK 3B structural links. `CANDIDATE` covers bounded call candidates.

## Nodes

| kind         | key form            | label                    |
|--------------|---------------------|--------------------------|
| `file`       | `file:path`         | the path                 |
| `entity`     | `ent:{entity_id}`   | package/module key       |
| `declaration`| `decl:path#id`      | declaration name         |
| `import`     | `imp:path#id`       | the import statement     |
| `call`       | `call:path#id`      | the written callee       |

A call site is a node **distinct** from its candidate targets. Its `disposition`
records the bounded outcome (`single_candidate`, `multiple_candidates`,
`no_candidate:<reason>`, `out_of_scope:<reason>`, or `no_candidate_rule` for
languages with no rule) — so a `NoCandidate`/`OutOfScope` call stays visible
without fabricating a target edge.

## Edges

- `contains` (fact): `file -> declaration|import|call`.
- `member_of` (fact): `file -> entity` (package/module membership).
- `<link.kind>` (fact): every resolved TASK 3B link (`import_path`, `use_path`,
  `package_membership`, ...) projected onto its source/target nodes.
  Unresolved/out-of-scope links produce no edge — their outcome is preserved on
  the source node.
- `call_candidate` (candidate): `call -> declaration`. `SingleCandidate` → one
  edge; `MultipleCandidates` → one edge per target, all sharing the record's
  `candidate_set_id`.

Edges carry `rule_id`, `upstream_id` (the `lnk-`/`cand-` record), `outcome`,
and `candidate_set_id` where applicable.

## API / CLI

```rust
GraphIndex::load(dir); g.node(id_or_key); g.outgoing(id); g.incoming(id);
g.neighborhood(id, filter); g.traverse(id, opts); g.stats();
```

```text
repodex graph build|verify|stats|node|outgoing|incoming|neighborhood
```

Traversal is bounded (`max_depth`/`max_nodes`) and deterministically ordered.
`neighborhood` returns one-hop relations separated by direction and filterable
by edge kind / evidence class / language — the foundation for "show me callers".

## Boundaries

- No semantic model, Jev, embeddings or natural-language query (§3).
- No cross-language inference (§17). No fake unknown-target nodes (§14).
- A candidate path `A -[candidate]-> B` is a *possible* relationship path, not a
  confirmed runtime route (§24).
- The manifest records exact snapshot/link/candidate digests — a stale upstream
  makes the graph fail verification (§28/§51).

## RDX1 design note

Graph ids are content-derived (`gn-…`/`ge-…`) and stable across roots, so a
future RDX1 projection can emit `F <node-id>` / `R <node-id> <kind> <node-id>`
lines without reparsing. Not implemented here.
