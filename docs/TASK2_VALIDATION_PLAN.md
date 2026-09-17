# TASK 2 Validation Plan (frozen)

```text
status:              FROZEN
frozen before:       any TASK 2 validation measurement was observed
TASK 1 commit:       4a6e8e3fe88be068932279b3bf896c731c812859
TASK 1 status:       FOUNDATION_SPIKE_COMPLETE
plan author:         autonomous TASK 2 execution
```

This plan was written and frozen **before** any corpus was scanned, any audit
region was annotated, and any TASK 2 performance measurement was taken. Nothing
in this document was derived from a TASK 2 result. If an acceptance criterion is
ever changed, the change is recorded in section 12 with its justification and
kept separate from the original criterion.

---

## 1. What is being validated

TASK 1 asserts that RepoDex turns source bytes into **normalized unresolved
syntax facts** deterministically and honestly:

```text
source bytes
  -> Tree-sitter
  -> language adapter
  -> normalized unresolved syntax facts
```

TASK 2 asks whether that remains trustworthy and practical on real repositories.
It does **not** validate semantic resolution, and it must not add any.

## 2. Scope boundary (what TASK 2 will not do)

Not implemented, not added, not depended on:

```text
persistent index / database
cross-file semantic resolver
resolved call graph / resolved symbol graph
repository knowledge graph / navigator / investigation memory
LLM / embedding / vector DB
gopls / rust-analyzer / Pyright / PHPStan / Psalm
framework intelligence
daemon / watcher / MCP / HTTP / RPC / GUI
TypeScript / JavaScript / Svelte
parallel scanning
```

Baseline scanning is single-threaded. All measurements use release builds.

---

## 3. Mandatory corpora and workloads

### 3.1 Corpus selection rules

Public corpora only, one or more substantial production repositories per
language, each pinned to an exact commit SHA. Preference for repositories with
multiple packages/modules, tests, nested layout, modern syntax, non-trivial
imports and calls, and substantial volume.

Local user-owned repositories are used **in addition** where readily available,
and are recorded with commit SHA **and** dirty state, because a pinned SHA alone
does not describe local modifications. Third-party repositories are never
committed into RepoDex. The user's working checkouts are never modified:
experiments use clean isolated checkouts or in-memory edits.

### 3.2 Mandatory workload classes

```text
W1  synthetic size matrix (TASK 1 harness), all four languages
W2  real repository, per language, full scan
W3  combined real-code collection reaching >= ~150,000 supported source LOC
W4  deeply nested / pathological workload
W5  large-file behaviour around the 8 MiB default guard
W6  incremental workloads on real files, all four languages
```

W3 may be satisfied by a documented collection of pinned repositories; real
source files are not duplicated to inflate LOC. Synthetic repetition stays
labelled synthetic.

### 3.3 Corpora actually pinned

Recorded in `benchmarks/corpora.json` at execution time (step 4 of the execution
order), which is after this plan is frozen. The manifest is the authority for
exact SHAs, licenses, scopes and exclusions. This plan fixes only the selection
rules above and the requirement that every corpus be pinned and reproducible.

---

## 4. Mandatory measurements

### 4.1 Correctness (must precede the performance campaign)

```text
M1  baseline scan metrics per corpus, with categories kept distinct:
      files visited, supported candidates, parsed clean, parsed with recovery,
      unsupported input, size-limit skips, read failures, parser failures,
      extraction failures, ERROR nodes, MISSING nodes, declarations, imports,
      references, call-like, test candidates, bytes processed, elapsed time

M2  recovery denominators per the corrected rule (section 5)

M3  recovery investigation: representative real ERROR/MISSING classified as
      grammar limitation | unsupported/new syntax | adapter bug |
      mixed-language source | generated-source peculiarity | intentional
      malformed source | input/encoding issue | unknown

M4  extraction-quality audit against frozen, independently selected regions

M5  SourceRange validation against source bytes, including UTF-8 multibyte,
      CRLF, nested declarations, and same-name declarations in different
      contexts

M6  determinism: >= 3 full canonical comparisons on a substantial repository,
      plus an identical copy at a different checkout root; canonical facts or a
      canonical digest derived from complete facts must be identical

M7  real-file incremental correctness, all four languages, before the
      performance campaign
```

### 4.2 Performance and memory

```text
M8   stage timings where practical: discovery, reading, parser/grammar/query
       setup, raw parse, normalized extraction, canonicalization/sorting,
       serialization/output, end-to-end scan

M9   end-to-end real-repository scans: >= 5 fresh-process executions, reporting
       median, minimum, maximum, files/sec, MB/sec, LOC/sec

M10  short stage benchmarks: >= 10 steady-state samples

M11  process memory via a trustworthy OS mechanism (/usr/bin/time -v on Linux),
       for one substantial corpus per language plus the largest workload;
       the metric is named accurately and not called RSS unless it is RSS

M12  tree/source retention A/B/C experiment on at least one substantial corpus

M13  incremental performance: incremental parse only, fresh parse only,
       incremental parse + extraction, fresh parse + extraction
```

### 4.3 Throughput denominators

Throughput is computed from bytes/LOC **actually processed**, never from total
checkout size. Candidate bytes, processed bytes and skipped bytes are reported
separately.

---

## 5. Corrected denominators (fixed in advance)

```text
trees_returned          = parsed_clean + parsed_with_recovery
recovery_rate_among_parsed = parsed_with_recovery / trees_returned
```

If `trees_returned = 0`, report `N/A`, never `0%`. Recovery is never divided by
all supported candidates when some were never parsed. Recovered parse, parser
failure, extraction failure, unsupported input and expected skip stay separate
counters and are never collapsed.

---

## 6. The extraction support contract being scored

RepoDex is scored against **the syntactic promise TASK 1 actually made**, not
against semantic resolution.

In scope, and scored:

```text
declarations           (as documented per language in LANGUAGE_SPIKE.md)
imports                (as documented per language)
required reference kinds
CallLikeOccurrence     including deliberately unresolved forms
TestEvidence           as a SYNTACTIC candidate policy only
SourceRange correctness
recovery explicitness
determinism
incremental equivalence
```

Explicitly **not** scored as errors:

```text
obj.F() not resolved to a specific method          (correct: unresolved)
Thing() in Python not proven a constructor         (correct: unresolved)
pkg.F() in Go not resolved to a package function   (correct: unresolved)
Generic[int](value) not resolved call-vs-conversion (correct: unresolved)
first-class callable creation not called           (correct: a reference)
TestEvidence not matching a real runner's discovery (out of contract)
```

But an omission **is** an error:

```text
a required syntactic occurrence missing entirely        -> FN
a prediction with no source-grounded basis              -> FP
a duplicate prediction for one occurrence               -> FP
systematic range corruption                             -> HIGH or BLOCKER
```

Semantic ambiguity and extraction omission are different things and are never
conflated. A missing required occurrence is never filed as `AMBIGUOUS` or
`OUT_OF_SCOPE`.

---

## 7. Audit methodology (frozen before scoring)

Audit regions are selected and frozen **independently of RepoDex output** — never
by sampling RepoDex's own predictions, which would invalidate precision and
recall.

Within each frozen region:

```text
1. independently annotate every in-scope expected occurrence
2. collect every RepoDex prediction for the same categories in that region
3. match expected and actual one-to-one
4. unmatched expected occurrence = FN
5. unmatched prediction          = FP
6. duplicate prediction          = FP
7. matched prediction            = TP
```

Matching rules are fixed here, before scoring:

```text
a match requires the same category and the same source span
  (byte range) for the occurrence anchor
for call-like occurrences the anchor is the whole expression range
for declarations the anchor is the name range
a prediction whose anchor is inside the expected span but not equal is
  counted as a match only when the region contains no other occurrence of the
  same category within that span; otherwise it is scored by its own span
```

Per-language annotation targets: at least 30 declaration occurrences, 30
call-like occurrences, 10 import occurrences, and 10 test-candidate examples
including negative examples. These numbers describe the amount of annotated
material, not a licence to cherry-pick individual occurrences: complete frozen
regions are audited. If a region genuinely contains fewer examples in a
category, all available examples are audited and the smaller sample size is
reported.

Reported per language and category: TP, FP, FN, precision, recall, sample size.
No single universal "RepoDex accuracy" number is produced.

Holdout discipline: an audit-discovered bug may be fixed, but the pre-fix result
is preserved, a regression test is added, and the post-fix result is recorded.
The exact examples that triggered a fix are not used as evidence of
generalization.

---

## 8. Correctness conditions that block GO

Any of the following is a **BLOCKER** and forces
`validation_status = VALIDATION_BLOCKED` with
`architecture_recommendation = NOT_ISSUED`, unless a bounded foundation fix
resolves it and the affected validation is rerun:

```text
incremental and fresh extraction disagree on mandatory supported behaviour
canonical output is nondeterministic on real repositories
systematic source-range corruption
a priority-language adapter is fundamentally broken on real code
the scanner silently omits significant supported source
query truncation silently corrupts results
partial extraction is reported as complete
```

Any unresolved correctness-related **HIGH** finding affecting mandatory supported
coverage also blocks an unconditional `GO`:

```text
common declarations systematically missing
systematic range corruption
silent query truncation
nondeterministic canonical output
```

---

## 9. GO vs CONDITIONAL_GO

`GO` requires all mandatory evidence present (section 4) **and**:

```text
all four adapters remain substantive on real code
real parsing broadly reliable; recovery explicit
required extraction categories work credibly on real code
no systematic range corruption
canonical output deterministic
incremental and fresh extraction agree
no silent truncation
large workloads complete without panic
memory behaviour credible for continued development
performance credible for repository analysis
remaining gaps belong naturally in later resolver/map layers
no unresolved BLOCKER, and no unresolved correctness-related HIGH affecting
  mandatory supported coverage
```

`CONDITIONAL_GO` is used when the foundation is credible but continuation
depends on bounded conditions. Each condition records: problem, impact, required
follow-up, and an objective follow-up measurement.

`NO_GO` is used only for a fundamental architecture problem (a priority language
cannot be credibly supported; the normalized model loses essential distinctions;
incremental and fresh results remain inconsistent; common real source frequently
fails; common constructs systematically produce misleading facts; memory or
runtime characteristics are structurally impractical; usable syntax extraction
effectively requires an immediate language-server dependency).

`NOT_ISSUED` is used when mandatory evidence is unavailable or validation is
incomplete. Missing mandatory data is not automatically evidence for `NO_GO`.

---

## 10. Performance and memory budgets

```text
No external hard performance budget and no external hard memory budget were
provided for TASK 2.
```

This is stated explicitly rather than invented. No numeric pass/fail threshold
for throughput, latency or memory is fabricated to make the decision appear
objective. Performance and memory are therefore reported as **observations**
with their environment and workload, and are judged only against the qualitative
requirement in section 9 ("credible for repository analysis", "memory-conscious",
"incrementally viable"). Where a number is needed to justify a condition, the
number is reported with its measurement context, not asserted as a universal
threshold.

No universal accuracy threshold is invented either. Audit results are reported
as measured precision/recall with sample sizes.

---

## 11. What constitutes incomplete validation

Validation is `VALIDATION_BLOCKED` with `NOT_ISSUED` if any of these cannot be
obtained:

```text
a pinned corpus for a priority language (Rust, Go, Python, PHP)
a mandatory measurement from section 4
an environment capable of running the release binary
resolution of a foundation BLOCKER
```

The following are recorded as NOT MEASURED with a reason and do **not** by
themselves block validation, provided enough mandatory evidence exists for a
limited recommendation:

```text
optional old Go RepoDex comparison (section 45 of the task)
profiling, when no major performance problem was demonstrated
platform metrics that cannot be measured honestly on this machine
```

The campaign stops early — before the performance numbers are taken — if a
foundation blocker makes further numbers misleading. Evidence already gathered is
preserved.

---

## 12. Recorded changes to this plan after freezing

```text
none
```

No acceptance criterion was changed after observing results. Any future change
must be recorded here with its justification, clearly separated from the original
criterion.

---

## 13. Execution order (fixed)

```text
 1. Inspect TASK 1 results
 2. Verify exact TASK 1 commit and rerun gates
 3. Create and freeze this plan
 4. Select/pin corpora
 5. Create corpus manifest
 6. Capture environment
 7. Baseline scan all corpora
 8. Investigate recovery/parser/extraction failures
 9. Freeze independent audit source regions
10. Perform extraction-quality audit
11. Validate SourceRange correctness
12. Run real-repository determinism checks
13. Run real-file incremental correctness checks
14. Stop/fix if any foundation blocker exists
15. Run synthetic performance campaign
16. Run real-repository end-to-end performance campaign
17. Measure process memory
18. Run A/B/C tree/source retention experiment
19. Measure incremental performance
20. Run large/deep workload validation
21. Profile demonstrated major bottlenecks
22. Apply only bounded evidence-driven fixes
23. Preserve pre-fix evidence
24. Add regression tests
25. Re-run affected audits/benchmarks
26. Run untouched/holdout checks where available
27. Update findings and reports
28. Run complete Cargo verification
29. Determine VALIDATION_COMPLETE/BLOCKED
30. Issue GO / CONDITIONAL_GO / NO_GO / NOT_ISSUED
31. Answer TASK 3 readiness
32. Commit
```

---

## 14. Reproducibility record

Recorded in the run directory and in the reports:

```text
RepoDex SHA and dirty state
corpus SHAs and dirty state where applicable
commands
environment (Rust, Cargo, tree-sitter and grammar versions, OS, CPU, RAM,
  build profile, thread count = 1, file-size limit, ignore policy)
benchmark run directory
audit regions and matching rules
validation acceptance criteria (this document, with its freeze hash)
```

Evidence is written under `~/reposuite/repodex/benchmarks/<run-id>/` via the
central RepoDex path layer. Raw evidence is not replaced by narrative summaries.

## 15. Ignore policy to be verified

Scanner behaviour must not silently depend on global Git ignore, global excludes,
parent-directory ignore files outside the scan root, or developer-specific Git
configuration. The exact policy is recorded with the environment capture.
