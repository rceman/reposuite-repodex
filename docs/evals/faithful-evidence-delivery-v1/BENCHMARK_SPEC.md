# Faithful Bounded Evidence Delivery — Benchmark Spec

Hypothesis under test (§3): reducing evidence loss at the serving boundary may
eliminate Agent search/read work more cheaply than a better retriever.

## Canonical projection

One `EvidenceProjection` (`src/query/projection.rs`) is built from `QueryResult`
+ the validated view's snapshot; every transport (machine JSON, service JSON,
RDX, human) renders from it — no parallel renderer reinterprets `QueryResult`.

## Treatments (Agent pilot)

- **E0 control**: the prior lossy projection — `kind` + `label` only (no rank,
  path, locator, range, endpoints, via, candidate-set, or completeness scope).
- **E1 faithful**: full bounded projection — explicit `rank`, repo-relative
  `path`, `key`, `disposition`, current `declaration_range`/`body_range`,
  relations with direction-resolved `from_*`/`to_*` + `via` + `rule_id` +
  `candidate_set`, scoped completeness, and a bounded connector packet.

Same underlying query/result; only the delivered representation differs.

## Question set (frozen, 8)

2 lookup, 2 same-name disambiguation, 2 relationship, 2 bounded connector —
see `questions.json`. Repo: reposuite-repodex (Rust).

## Schedule (§46)

2 treatments x 8 questions x 4 reps = 64 sessions, concurrency 1 (serial),
counterbalanced order per (question,rep). One frozen harness/model/policy.

## Measurements (§49-§52)

correctness (gold-evidence recall), input/output tokens, tool calls
(search/read breakdown), wall time, time-to-evidence — from the ATIF export.
