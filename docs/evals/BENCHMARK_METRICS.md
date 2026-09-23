# RepoDex Benchmark KPI Contract

Canonical reporting standard for RepoDex experiments. Every Agent benchmark
**must** report four first-class dimensions separately:

```text
1. CORRECTNESS
2. COST
3. WORK
4. TIME
```

Correctness is a gate: a cheaper system that materially loses correctness must
not be declared better solely because it consumes fewer resources (§47).

---

## 1. CORRECTNESS (§47)

For Agent investigation benchmarks, report where applicable:

```text
RequiredFactRecall
PrimaryEvidenceCoverage
UnsupportedClaimCount
ContradictedClaimCount
InvalidPathOrSymbolCount
```

- Recall/coverage are scored against evaluator-side frozen gold only.
- `PrimarySymbolEvidenceCoverage` may be added where exact-symbol gold exists
  (do not fabricate it where ambiguous).
- Correctness dominates: report it first and treat regressions as disqualifying
  even when cost improves.

## 2. COST (§48-§54)

Input tokens alone are **not** sufficient. Report all provider-actual token
categories that exist:

```text
agent_input_tokens          (provider-reported)
agent_output_tokens         (provider-reported)
agent_total_tokens          (= input + output, or provider total)
```

Do NOT headline only `input_tokens`.

### Output tokens (§49-§50)

`output_tokens` is the authoritative provider-reported output figure. Depending
on provider/runtime semantics it may bundle reasoning, tool-call construction,
intermediate assistant output, and the visible answer. **Do not** infer a
breakdown the provider does not expose.

- If `reasoning_tokens` is authoritatively supplied, record it separately.
- If unavailable, report `reasoning_tokens = unavailable`. Never estimate
  hidden reasoning from residual counts.

### Cached input (§51)

If telemetry distinguishes `cached_input_tokens` / `uncached_input_tokens`,
preserve the split — it is billing-relevant. Do not flatten it away.

### Monetary / weighted cost (§52-§53)

- When an explicit **pricing snapshot** is available, compute
  `estimated_model_cost` using the exact token categories that pricing model
  supports, and persist the snapshot metadata (model, currency, rates, date).
- Never silently hard-code a model price into generic RepoDex semantics.
- If no pricing snapshot exists, always report the raw token categories.
  Optionally report a model-independent weighted expression **only** when the
  weighting ratio is explicitly supplied and documented — do not invent one.

### Cost per correct investigation (§54)

Where correctness is binary or thresholded, report
`ModelCostPerCorrectInvestigation` (or tokens-at-preserved-correctness). Do not
hide correctness failures behind aggregate cost savings.

## 3. WORK (§55)

Report where available — these quantify actual investigation work, not just
token spend:

```text
model_calls
tool_calls_total
search_calls
file_read_calls
directory_list_calls
git_inspection_calls
unique_source_files_observed
unique_source_files_explicitly_read
source_exposure_bytes
unique_symbols_exposed
```

## 4. TIME (§56-§60)

Time is first-class — do not bury a single `wall_ms`. Distinguish:

```text
preprocessing_ms
  = repodex_retrieval_ms + memory_lookup_ms + symbol_annotation_ms
    [+ optional_jev_ms]
time_to_first_primary_file_ms        tool_calls_to_first_primary_file
time_to_first_primary_symbol_ms      tool_calls_to_first_primary_symbol
time_to_sufficient_evidence_ms       tool_calls_to_sufficient_evidence
agent_investigation_wall_ms
combined_end_to_end_ms = preprocessing + agent_investigation_wall (serial)
```

- **First-evidence** (§58): earliest trace point at which the first primary
  file/symbol was exposed, where evaluator gold permits reliable mapping.
- **Sufficient-evidence** (§59): earliest trace point where all required
  primary evidence has been exposed — computed only when objectively
  determinable, else `unavailable` (do not guess).
- **End-to-end** (§60): `preprocessing + agent wall` for serial execution.
  For concurrent-treatment benchmarks, latency is SECONDARY — document
  contention caveats.

## Canonical headline (§61)

| Treatment | Correctness (recall/cov) | Cost (in/out/total tokens, est cost) | Work (tools/searches/reads/files) | Time (to-primary, to-sufficient, wall) |
|-----------|--------------------------|---------------------------------------|-----------------------------------|----------------------------------------|

Report mean/median/p25/p75/p95/min/max where meaningful, paired
question×repetition deltas, and deterministic bootstrap 95% CIs with a fixed
documented seed. Never make input tokens the sole headline resource metric.
