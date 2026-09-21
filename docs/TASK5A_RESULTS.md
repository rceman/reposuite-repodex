# TASK 5A — Results

Deterministic investigation graph over facts + links + candidates.

## Hugo (gohugoio/hugo — Go)

```text
nodes            71,119   (file 912, declaration 16,676, import 874, call 52,379, entity 278)
edges            82,321
  fact:contains        69,929
  fact:import_path      3,345
  fact:member_of          912
  fact:package_membership 912
  candidate:call_candidate 7,223
candidate sets    7,207   (3,918+3 package-local + 3,273+13 imported — reconciles T4C+T4D)
artifact          30,892,659 B   (≈0.39× snapshot, ≈1.34× candidates)
build             ~1.9 s   (peak RSS ~243 MB)
verify            ~0.35 s  (peak RSS ~88 MB)
bytes/node ≈ 434   bytes/edge ≈ 375
```

Go candidate-edge reconciliation (§44): `call_candidate` edges 7,223 =
7,191 single + 16 multiple-sets×2 edges. Candidate-bearing calls 7,207 =
3,921 (T4C package-local) + 3,286 (T4D imported) — exact.

## tokio (Rust)

```text
nodes            44,839   (file 563, declaration 8,666, import 3,277, call 32,082, entity 251)
edges            45,462
  fact:contains        44,025
  fact:member_of          378
  fact:module_file         79
  fact:use_path            11
  candidate:call_candidate 969
candidate sets      968   (967 single + 1 multiple)
artifact          18,138,444 B
build             ~1.05 s
```

tokio note: only 11 `use_path` edges — most Rust `use_path` links are
`unresolved`/`out_of_scope` (external crates, macro-generated), so they carry
a disposition rather than an edge. Correct per §31.

## §46–48 independent audit

`scripts/task5a_audit.py` re-derives the projection from upstream artifacts
(no graph-builder code): samples 500 decl + 500 import + 500 call nodes, 500
link edges, and checks **every** candidate record's edge count + target.

```text
Hugo:   CORRECT_NODE 1,500 · CORRECT_FACT_EDGE 2,000 ·
        CORRECT_SINGLE 7,191 · CORRECT_MULTIPLE_SET 16 ·
        CORRECT_NO_TARGET_EDGE 45,172
tokio:  CORRECT_NODE 1,500 · CORRECT_FACT_EDGE 1,590 ·
        CORRECT_SINGLE 967 · CORRECT_MULTIPLE_SET 1 ·
        CORRECT_NO_TARGET_EDGE 31,114

MISSING_NODE=0  EXTRA_NODE=0  WRONG_EDGE_TARGET=0  WRONG_EDGE_CLASS=0
CANDIDATE_PROMOTED_TO_FACT=0  BROKEN_PROVENANCE=0   (both corpora)
```

## Fixture / traversal

12 tests: file→decl/call contains, member_of, structural `import_path` FACT
edge, single candidate edge, multiple candidates sharing `candidate_set_id`,
NoCandidate/OutOfScope disposition with no target edge, `incoming(F)` reverse
callers, bounded+deterministic forward traversal, FACT/CANDIDATE-separated
neighborhood, rebuild determinism, update-vs-fresh, stale-candidate rejection.

## Verification

```text
cargo fmt / check / clippy -D warnings / release build   PASS
cargo test --locked                                       564 / 0 fail
graph fixtures (12)                                       PASS
independent audit (Hugo + tokio)                          PASS, 0 errors
determinism / update-vs-fresh / stale rejection           PASS
Rust + Go candidate audits                                unchanged
```
