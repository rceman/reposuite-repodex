# Adaptive Context Compiler V1 — spec

Goal: produce the smallest evidence representation that removes necessary
agent investigation work without hiding uncertainty or weakening correctness.

## Design

```
QueryResult + EvidenceProjection
  -> shape::classify(plan.intent, result)  (deterministic, no model)
  -> compiler::compile_into(proj, shape, memory_mode, budget)
       per-shape prune optional evidence -> obligation ledger
       -> serialized-byte budget enforcement -> context block
  -> CompiledPacket (trimmed projection + ledger + trace)
```

`context_policy` = `static` (faithful, default) | `adaptive`. Same path in
direct `--json`, flag-mode, and `POST /v1/query`. Trace is debug-only.

## Safety invariants

Mandatory protocol fields (rank, FACT/CANDIDATE, endpoints, ambiguity state,
current identity) never dropped from emitted records. `budget_limited` sets
`complete=false`. `unknown` obligation state never becomes `not_applicable`
without an objective reason. Source exposure semantics unchanged.
