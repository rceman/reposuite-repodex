# RepoSuite RepoDex — TASK 2

## Real Repository Validation, Extraction Audit, Performance & Rust Foundation Decision

Repository:

```text
https://github.com/rceman/reposuite-repodex
```

TASK 2 executes only after TASK 1:

```text
Rust + Tree-sitter Multi-Language Foundation Spike
```

Primary languages:

```text
1. Rust
2. Go
3. Python
4. PHP
```

TASK 1 asks:

```text
Can we build a small deterministic syntax-analysis foundation?
```

TASK 2 asks:

```text
Does that foundation remain trustworthy and practical
when exposed to real repositories?
```

This task is self-contained and intended for autonomous execution after TASK 1.

---

# 1. Hard TASK 1 precondition

Before any TASK 2 benchmark or audit:

inspect the current RepoDex repository.

Read:

```text
docs/SPIKE_RESULTS.md
docs/ARCHITECTURE.md
docs/LANGUAGE_SPIKE.md
```

TASK 2 may proceed only if TASK 1 records:

```text
FOUNDATION_SPIKE_COMPLETE
```

Record the exact TASK 1 commit SHA being validated.

The status marker alone is insufficient.

Rerun:

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release
```

Verify that all mandatory TASK 1 capabilities actually exist:

```text
Rust adapter
Go adapter
Python adapter
PHP adapter

normalized unresolved syntax facts
SourceRange contract
containment/scopes
imports
required references
CallLikeOccurrence
TestEvidence
recovery diagnostics

deterministic scanner
canonical full-fact validation/export path

incremental-equivalence harness
synthetic benchmark harness
```

If TASK 1 is not green:

```text
DO NOT start TASK 2 validation.
```

Instead:

```text
1. preserve TASK 1 evidence;
2. identify the exact blocker;
3. make only bounded TASK 1-scope corrections;
4. add a regression test;
5. rerun TASK 1 verification;
6. proceed only if TASK 1 becomes green.
```

Do not redesign architecture merely to reach TASK 2.

---

# 2. TASK 2 status model

Keep two independent conclusions.

## Validation execution status

Exactly one:

```text
VALIDATION_COMPLETE
VALIDATION_BLOCKED
```

## Architecture recommendation

Exactly one:

```text
GO
CONDITIONAL_GO
NO_GO
NOT_ISSUED
```

Use:

```text
NOT_ISSUED
```

when mandatory evidence is unavailable or validation is incomplete.

Missing mandatory data is not automatically evidence for `NO_GO`.

A completed experiment can still produce:

```text
NO_GO
```

if the evidence demonstrates a fundamental foundation problem.

---

# 3. Validation plan must be frozen first

Before observing TASK 2 results, create:

```text
docs/TASK2_VALIDATION_PLAN.md
```

Record:

```text
mandatory corpora/workloads

mandatory measurements

the extraction support contract being scored

the correctness conditions that block GO

conditions distinguishing GO vs CONDITIONAL_GO

known external performance/memory budgets if provided

what constitutes incomplete validation
```

Do not invent universal accuracy thresholds.

If no external hard performance budget was provided, say so explicitly.

Do not create a fake threshold merely to make the decision appear objective.

Do not change acceptance criteria after seeing results unless the change is:

```text
explicitly recorded
technically justified
clearly separated from the original criterion
```

An unconditional `GO` is not allowed while any unresolved:

```text
BLOCKER
or correctness-related HIGH
```

finding affects mandatory supported coverage.

---

# 4. Objective

Validate the TASK 1 foundation against real production source.

Evaluate:

```text
real-code parsing
recovery behavior
normalized extraction quality
false positives
false negatives
source-range correctness
determinism
repository throughput
stage costs
memory use
tree retention costs
incremental equivalence
incremental performance
large-file behavior
nested/pathological behavior
language-specific weaknesses
```

Determine whether the foundation is suitable for a future TASK 3 involving repository-level relationships.

Do not implement TASK 3.

---

# 5. Explicitly forbidden future systems

Do NOT implement in TASK 2:

```text
persistent production index
database

cross-file semantic resolver

resolved call graph
resolved symbol graph

repository knowledge graph

repository navigator
natural-language locate

investigation memory

LLM
embedding
vector DB

gopls
rust-analyzer
Pyright
PHPStan
Psalm

Laravel/Symfony framework intelligence

daemon
watcher
MCP
HTTP
RPC
GUI

TypeScript
JavaScript
Svelte
SvelteKit
```

TASK 2 evaluates the current foundation.

Do not hide foundation weaknesses by adding semantic systems.

---

# 6. Baseline execution model

All mandatory TASK 2 baseline scans are:

```text
single-threaded
```

Do not add parallel scanning.

If future parallelism appears desirable, record it as a future optimization.

Do not introduce it into this validation campaign.

---

# 7. Corpus categories

Use two categories where available:

```text
A. relevant user-owned / RepoSuite repositories available locally

B. pinned public open-source repositories
```

Do not assume private repositories are accessible.

Do not request credentials.

Do not scan unrelated personal directories.

---

# 8. Public corpus requirements

Select at least one substantial public production repository for each language:

```text
Rust
Go
Python
PHP
```

Prefer repositories containing:

```text
multiple modules/packages
tests
nested code layout
modern syntax
non-trivial imports
non-trivial call expressions
substantial source volume
```

Reasonable candidate classes include:

```text
Rust:
  substantial Rust CLI/server/library

Go:
  substantial Go service/tool/library

Python:
  substantial Python framework/tool/library

PHP:
  substantial modern PHP framework/library
```

Possible candidates may include well-established projects such as:

```text
Rust:
  ripgrep / bat / another substantial production Rust repository

Go:
  Cobra / Prometheus component / another substantial Go repository

Python:
  Black / Flask / another substantial Python repository

PHP:
  Laravel Framework / Symfony component or framework
```

These are candidates, not mandatory repository names.

Choose corpora based on relevance and reproducibility.

---

# 9. Pin every corpus

Every public corpus must record:

```text
repository URL
exact commit SHA
license
language
supported source roots where constrained
excluded roots
```

Never benchmark a moving branch without recording the resolved SHA.

Do not commit third-party repositories into RepoDex.

---

# 10. Protect benchmark corpora

Do not modify the user's existing working repositories.

Use:

```text
clean isolated checkout
disposable copy
in-memory edits
```

for experiments.

For every corpus record:

```text
commit SHA
dirty state
untracked state where relevant
```

Pinned commit SHA alone does not describe local modifications.

Incremental edit experiments must not mutate the canonical benchmark corpus.

---

# 11. Corpus manifest

Create a committed machine-readable manifest, for example:

```text
benchmarks/corpora.json
```

or an equally simple equivalent.

Each entry should include:

```text
id
language
repository URL
pinned revision
license
source scope
exclusions
notes
```

Do not commit machine-specific absolute local paths into canonical configuration.

Local checkout paths may be supplied externally.

---

# 12. RepoSuite/user-owned repositories

If readily available locally, useful validation targets may include:

```text
the old Go RepoDex implementation
other RepoSuite Go services/tools
reposuite-repodex itself
available Python projects
available PHP/Laravel/Lumen projects
```

Do not assume any particular local path.

Do not let discovery of these repositories become a large task.

If unavailable:

```text
record unavailable
continue with public corpora
```

---

# 13. Benchmark artifact namespace

All generated TASK 2 evidence belongs under:

```text
~/reposuite/repodex/benchmarks/
```

Use a unique run directory:

```text
~/reposuite/repodex/benchmarks/<run-id>/
```

Resolve this through the central RepoDex path layer.

Store useful raw evidence such as:

```text
environment.json
corpora.json
validation-plan.json if useful
commands.txt
raw-results.jsonl
summary.json
memory/
diagnostics/
audit/
```

Do not commit large machine-generated benchmark outputs.

Small stable manifests/scripts belong in Git.

---

# 14. Environment capture

Before performance measurements record:

```text
RepoDex commit SHA
RepoDex dirty status

Rust version
Cargo version

tree-sitter runtime version
grammar versions

OS
kernel/version where available

CPU model
logical CPU count

RAM

relevant storage/filesystem information where easily available

build profile

scanner thread count

file-size limit

ignore policy
```

Do not collect secrets or unrelated machine information.

---

# 15. Revalidate ignore policy

Benchmark results must not depend on developer-specific ignore state.

Verify scanner behavior does NOT silently depend on:

```text
global Git ignore
global Git excludes
parent-directory ignore files outside scan root
developer-specific Git configuration
```

Record exact ignore policy.

---

# 16. Baseline scan metrics

For every corpus collect:

```text
files visited
supported candidate files

parsed clean
parsed with recovery

unsupported input
size-limit skips
read failures
parser failures
extraction failures

ERROR node counts
MISSING node counts

declarations
imports
reference occurrences
call-like occurrences
test candidates

source bytes actually processed
supported LOC where measured

elapsed time
```

Keep categories distinct.

Do not collapse:

```text
recovered parse
parser failure
extraction failure
unsupported input
expected skip
```

into one counter.

---

# 17. Recovery denominator

Do NOT calculate recovery as:

```text
files_with_recovery / all_supported_candidates
```

when some candidates were never parsed.

Define:

```text
trees_returned =
    parsed_clean
  + parsed_with_recovery
```

Then:

```text
recovery_rate_among_parsed =
    parsed_with_recovery / trees_returned
```

If:

```text
trees_returned = 0
```

report:

```text
N/A
```

not:

```text
0%
```

Separately report:

```text
processing coverage
parser failures
extraction failures
skips by reason
```

---

# 18. Recovery investigation

For real files with:

```text
ERROR
MISSING
```

inspect representative examples.

Classify findings:

```text
grammar limitation
unsupported/new syntax
adapter bug
mixed-language source
generated-source peculiarity
intentional malformed source
input/encoding issue
unknown
```

Create minimal regression fixtures for important problems.

Do not silently patch around grammar deficiencies.

---

# 19. Extraction-quality audit methodology

This is mandatory.

Do NOT select isolated examples only from RepoDex predictions.

That would produce invalid precision/recall measurements.

Instead:

```text
select and freeze source files
or bounded source regions
independently of RepoDex output
```

Within every selected region:

```text
1. manually/independently annotate every in-scope expected occurrence;

2. collect every RepoDex prediction for those same categories
   inside the region;

3. match expected and actual occurrences one-to-one;

4. unmatched expected occurrence = FN;

5. unmatched prediction = FP;

6. duplicate prediction = FP;

7. correctly matched prediction = TP.
```

Define matching rules before scoring.

Do not cherry-pick only successful predictions.

---

# 20. Audit sample targets

Per language target at least:

```text
30 declaration occurrences
30 call-like occurrences
10 import occurrences
10 test-candidate examples including negative examples
```

These numbers describe the amount of annotated material.

They do NOT authorize choosing only those individual occurrences.

Audit complete frozen source regions.

If the selected corpus genuinely contains fewer examples in a category:

```text
audit all available examples
report the smaller sample size
```

---

# 21. Audit semantic boundaries

Do not punish RepoDex for intentionally unresolved semantics.

Example:

```text
x.F()
```

may correctly be represented as:

```text
member/selector CallLikeOccurrence
```

without knowing the target method.

That is not a false result.

But if the required syntactic occurrence is missing entirely:

```text
that is an FN
```

Do not hide extraction failures under:

```text
AMBIGUOUS
OUT_OF_SCOPE
```

Semantic ambiguity and extraction omission are different things.

---

# 22. TestEvidence ground truth

Audit `TestEvidence` against the documented syntactic TASK 1 policy.

Do NOT compare against full actual runtime discovery semantics of:

```text
pytest
unittest
PHPUnit
cargo test
go test
```

unless the TASK 1 policy explicitly models the relevant rule.

TASK 2 validates the promised syntax-level candidate policy.

It does not require hidden semantic/framework intelligence.

---

# 23. Extraction audit result categories

Use:

```text
TP
FP
FN
AMBIGUOUS
OUT_OF_SCOPE
```

For each audited item record:

```text
repository
commit
relative path
source region/range
category
expected result
actual result
classification
notes
```

For each category and language calculate where meaningful:

```text
TP
FP
FN
precision
recall
sample size
```

Do not produce one universal “RepoDex accuracy” number.

---

# 24. Holdout discipline after fixes

An audit-discovered bug may be fixed.

If so:

```text
preserve the original pre-fix result
add a regression test
record the post-fix result
```

Do not use only the exact examples that triggered the fix as evidence of generalization.

Where practical, preserve a small untouched holdout subset for final validation.

Document when a validation example influenced implementation.

---

# 25. Range validation

Explicitly validate normalized ranges against source bytes.

Include real files containing where available:

```text
UTF-8 multibyte text
CRLF
nested declarations
same-name declarations in different contexts
```

Verify:

```text
declaration range
name range
body range
call-like target range
import ranges
```

Any systematic range corruption is:

```text
HIGH
or
BLOCKER
```

depending on breadth.

---

# 26. Determinism on real repositories

For at least one substantial repository:

run at least:

```text
3 full canonical-result comparisons
```

Canonical normalized facts must remain identical.

Also copy identical repository contents to a different checkout root and verify canonical fact equivalence.

Do not compare only aggregate counts.

Compare:

```text
complete canonical facts
```

or:

```text
canonical digest derived from complete facts
```

Timing metadata must remain outside canonical equality.

---

# 27. Incremental correctness happens before heavy benchmarking

After:

```text
baseline correctness
range validation
determinism
```

run real-source incremental equivalence before the main performance campaign.

An incremental correctness failure can invalidate later performance conclusions.

Do not postpone it until the end.

---

# 28. Real-source incremental test selection

Test all:

```text
Rust
Go
Python
PHP
```

Use multiple non-trivial real files where practical.

Rust and Go receive more cases because they are current highest priorities.

Representative edits:

```text
identifier rename
small expression edit
line insertion
line deletion
new declaration
remove declaration
edit near EOF
introduce temporary syntax damage
repair syntax damage
```

For each edit compare:

```text
incremental parse
vs
fresh full parse
```

and:

```text
normalized full-file extraction from incremental tree
vs
normalized full-file extraction from fresh tree
```

Any unexplained mismatch is a foundation blocker until understood.

---

# 29. Incremental timing reset rule

For each performance sample:

```text
start from the same original bytes
start from the same original tree state
apply the same edit
measure the same workload
```

Do not let repeated timing runs accumulate different edits.

Sequential multi-edit correctness tests are allowed but are not a replacement for controlled timing repetitions.

---

# 30. Performance stages

Measure where practical:

```text
repository discovery

source reading

parser/grammar/query setup

raw parsing

normalized extraction

canonicalization/sorting

serialization/output

end-to-end scan
```

Do not over-engineer benchmark instrumentation.

Keep nondeterministic timing fields separate from canonical normalized results.

---

# 31. Performance workloads

Use:

```text
TASK 1 synthetic workloads
+
TASK 2 real corpora
```

Ensure at least one real-code workload reaches approximately:

```text
150,000 supported source LOC
```

or larger.

If no single selected repository reaches this size:

construct an explicitly documented collection of pinned real repositories.

Do not duplicate real source files merely to inflate LOC.

Synthetic scaling tests may use generated repetition but must remain labelled:

```text
synthetic
```

---

# 32. Throughput denominator

Throughput such as:

```text
MB/sec
LOC/sec
```

must be calculated using:

```text
bytes/LOC actually processed
```

not total checkout size containing skipped files/directories.

Report separately:

```text
candidate bytes
processed bytes
skipped bytes where measurable
```

Do not inflate throughput or recovery metrics through incorrect denominators.

---

# 33. End-to-end scan measurements

For substantial workloads record:

```text
source files
source bytes processed
source LOC processed
facts emitted

wall-clock duration

files/sec
MB/sec
LOC/sec where available
```

Use release builds.

Run at least:

```text
5 fresh-process executions
```

for substantial end-to-end scans.

Report at minimum:

```text
median
minimum
maximum
```

Fresh process does NOT prove cold filesystem cache.

Describe cache conditions honestly.

---

# 34. Stage benchmark repetitions

For short stage benchmarks use at least:

```text
10 steady-state samples
```

Batch very short operations where necessary.

Do not benchmark debug builds.

Record build configuration.

---

# 35. Memory measurement

Measure process-level memory using a trustworthy platform mechanism.

On Linux, an example is:

```text
/usr/bin/time -v
```

On another platform:

use and name the equivalent OS metric accurately.

Do not label every platform-specific metric “RSS” if it is not actually RSS.

Do not treat Rust allocator statistics as total Tree-sitter process memory.

Measure representative scans for at least:

```text
one substantial Rust corpus
one substantial Go corpus
one substantial Python corpus
one substantial PHP corpus
largest overall workload
```

Record:

```text
peak process memory metric
source bytes processed
facts emitted
repository/corpus
```

If reliable process-level measurement is unavailable:

```text
NOT MEASURED
```

with reason.

---

# 36. Tree/source retention experiment

Run the same parsing and extraction workload in all variants.

Compare:

## Variant A

```text
extract facts
release Tree-sitter trees
release source buffers
```

## Variant B

```text
extract same facts
retain Tree-sitter trees
release source buffers
```

## Variant C

```text
extract same facts
retain Tree-sitter trees
retain source buffers
```

Keep fact retention/output policy identical across variants.

Record exactly what remains alive.

Use at least one representative substantial corpus.

Measure process memory separately for each variant.

Interpret carefully:

```text
A → baseline syntax-analysis process cost

B → approximate additional cost of retained trees

C → approximate cost of retained trees + retained source buffers
```

Do not claim B alone represents the full future incremental repository engine.

Do not present process peak-memory differences as exact internal `Tree` object sizes.

---

# 37. Incremental performance

Measure separately:

```text
incremental parse only

fresh full parse only

incremental parse + complete file extraction

fresh full parse + complete file extraction
```

The important question is:

```text
how much incremental parse benefit survives
after normalized extraction cost is included?
```

Do not advertise parser-only speedup as expected repository-index speedup.

---

# 38. Large-file behavior

Validate:

```text
8 MiB default guard
explicit override
near-limit allowed file
above-limit file
bounded reads
clear diagnostics
no panic
```

Do not raise the limit merely to improve benchmark numbers.

Record behavior and cost.

---

# 39. Deep/nested workload behavior

Use real or synthetic deeply nested workloads.

Look for:

```text
pathological parse time
pathological extraction time
query explosion
match-limit exhaustion
duplicate facts
stack/recursion problems
memory spikes
```

Query truncation must never be silent.

Partial extraction must never be reported as complete.

---

# 40. Rust-specific real-code review

Inspect real Rust examples involving:

```text
impl contexts
trait impls
associated functions vs receiver methods
generics
nested modules
macro-heavy code
cfg-heavy code
tuple-struct construction
qualified paths
test attributes
```

Document where syntax-only extraction is sufficient and where a later semantic resolver is required.

Do not expand macros.

---

# 41. Go-specific real-code review

Inspect:

```text
receiver methods
interfaces
generics
selector expressions
package-qualified syntax
conversion/call ambiguity
anonymous functions
table-driven tests
t.Run
build-tagged files
```

Verify receiver syntax remains unresolved and is not falsely converted into semantic ownership.

---

# 42. Python-specific real-code review

Inspect:

```text
decorators
async functions
inheritance syntax
nested functions
lambdas
attribute/chained calls
dynamic call patterns
pytest candidates
unittest candidates
relative imports
```

Pay particular attention to test-candidate false positives.

Do not claim actual test-runner discovery.

---

# 43. PHP-specific real-code review

Inspect:

```text
namespaces
grouped use
use function
use const
traits
trait composition
enums
attributes
constructors
promoted properties
member calls
static calls
nullsafe calls
closures
arrow functions
PHPUnit candidates
mixed HTML/PHP
```

Explicitly test:

```php
foo($arg);
foo(...$args);
$callable = foo(...);
```

Confirm the first two are invocation syntax and the third is first-class callable creation.

Do not confuse closure `use` with namespace import.

---

# 44. Generated code

Do not automatically exclude all generated-looking source.

Agents may eventually need to inspect generated files.

Where a simple deterministic marker exists, classify:

```text
generated-looking
normal source
```

but do not build a complex generated-source classifier.

Measure/report significant generated-source effects separately where useful.

---

# 45. Optional old Go RepoDex comparison

If the previous Go RepoDex is readily available locally and runnable, perform a limited comparison.

This is optional.

Compare only materially similar work.

Possible overlap:

```text
repository scanning
file discovery
simple structural extraction/indexing
startup/runtime overhead
memory under comparable work
```

For every comparison explain:

```text
what old RepoDex does
what new RepoDex does
where workloads differ
```

Do not claim:

```text
Rust is X times faster
```

when the analysis performed is different.

---

# 46. Profiling

If a major performance problem appears:

profile before optimizing.

Identify likely dominating stage:

```text
filesystem
reading
Tree-sitter parse
queries
tree traversal
allocation
fact construction
sorting
serialization
```

Small targeted fixes are allowed if:

```text
bottleneck is demonstrated
change is local
correctness remains unchanged
regression test/benchmark is added
```

Do not redesign major architecture during TASK 2.

---

# 47. Allowed TASK 2 fixes

TASK 2 may fix:

```text
parser integration bug
adapter extraction bug
range bug
recovery bug
determinism bug
scanner robustness bug
obvious evidence-backed performance pathology
```

Every correctness fix requires a regression fixture/test.

Preserve:

```text
pre-fix evidence
post-fix evidence
```

Rerun affected validation/benchmarks.

Do not add future semantic layers to solve syntax problems.

---

# 48. Findings log

Create:

```text
docs/TASK2_FINDINGS.md
```

Use stable IDs:

```text
F001
F002
F003
...
```

Each finding should include:

```text
language
repository
path/example
category
severity
description
root cause if known
status
regression test if fixed
```

Categories may include:

```text
GRAMMAR
EXTRACTION
MODEL
RANGE
RECOVERY
PERFORMANCE
MEMORY
SCANNER
DETERMINISM
AMBIGUITY
```

---

# 49. Severity levels

Use:

```text
BLOCKER
HIGH
MEDIUM
LOW
INFO
```

Examples:

## BLOCKER

```text
incremental and fresh extraction disagree
in mandatory supported behavior
```

## HIGH

```text
common declarations systematically missing
systematic range corruption
silent query truncation
canonical output nondeterministic
```

## MEDIUM

```text
bounded uncommon construct unsupported
```

Do not inflate every limitation into a defect.

---

# 50. Architecture recommendation rules

After validation issue one of:

```text
GO
CONDITIONAL_GO
NO_GO
NOT_ISSUED
```

---

# 51. GO

Use `GO` only if mandatory evidence is complete and:

```text
all four adapters remain substantive

real parsing is broadly reliable

recovery is explicit

required extraction categories work credibly on real code

no systematic range corruption exists

canonical output is deterministic

incremental and fresh extraction agree

no silent truncation exists

large workloads complete without panic

memory behavior is credible for continued development

performance is credible for repository analysis

remaining semantic gaps belong naturally in later resolver/map layers
```

Additionally:

```text
no unresolved BLOCKER
no unresolved correctness-related HIGH
affecting mandatory supported coverage
```

may remain.

GO means:

```text
continue architecture development
```

It does NOT mean:

```text
production ready
feature complete
semantic resolution complete
interactive locate SLA proven
faster than Go
```

TASK 2 does not implement `locate()`, therefore it cannot prove future interactive navigation latency.

---

# 52. CONDITIONAL_GO

Use when the foundation is credible but continuation depends on bounded conditions.

Examples:

```text
specific grammar gap
specific adapter omission
tree-retention memory concern
single expensive extraction stage
specific query redesign
specific false-positive family
```

For every condition record:

```text
problem
impact
required follow-up
objective follow-up measurement
```

Optional missing measurements may become conditions if enough mandatory evidence exists for a limited architectural recommendation.

---

# 53. NO_GO

Use `NO_GO` only when evidence shows a fundamental architecture problem.

Examples:

```text
priority language cannot be credibly supported

normalized model loses essential distinctions

incremental/fresh results remain inconsistent

common real source frequently fails

common constructs systematically produce misleading facts

memory/runtime characteristics are structurally impractical

basic usable syntax extraction effectively requires immediate
language-server/semantic-engine dependency

Tree-sitter cannot provide a trustworthy foundation
for required syntax analysis
```

A NO_GO result is valid.

Do not manipulate the criteria to avoid it.

---

# 54. VALIDATION_BLOCKED / NOT_ISSUED

If mandatory validation cannot be completed because of:

```text
unavailable required corpus
unavailable essential measurement
unfixed foundation blocker
environment failure
```

use:

```text
validation_status = VALIDATION_BLOCKED
architecture_recommendation = NOT_ISSUED
```

Do not convert absence of evidence into `NO_GO`.

---

# 55. TASK 3 readiness

TASK 2 must answer:

```text
Is the foundation ready for TASK 3:
Repository Map + Cross-file Semantic Linking Foundation?
```

If yes:

recommend only the smallest justified TASK 3.

Do NOT implement TASK 3.

Future concepts may eventually include:

```text
File
Module
Declaration
SemanticSymbol
Definition relation
ImportEdge
ReferenceEdge
CallEdge
TestLink
ConfigAnchor
FrameworkAnchor
```

but do not prematurely finalize that schema in TASK 2.

---

# 56. Documentation updates

Update:

```text
README.md
docs/ARCHITECTURE.md
docs/LANGUAGE_SPIKE.md
docs/SPIKE_RESULTS.md
```

Add:

```text
docs/TASK2_VALIDATION_PLAN.md
docs/TASK2_FINDINGS.md
```

Raw benchmark evidence remains under:

```text
~/reposuite/repodex/benchmarks/<run-id>/
```

Do not replace raw evidence with narrative summaries.

---

# 57. SPIKE_RESULTS.md final organization

Expand to include:

```text
Executive summary

TASK 1 commit validated

Validation status

Architecture recommendation

Environment

Corpus manifest

Rust
  parsing/recovery
  extraction audit
  ranges
  determinism
  performance
  incremental
  memory
  limitations

Go
  parsing/recovery
  extraction audit
  ranges
  determinism
  performance
  incremental
  memory
  limitations

Python
  parsing/recovery
  extraction audit
  ranges
  determinism
  performance
  incremental
  memory
  limitations

PHP
  parsing/recovery
  extraction audit
  ranges
  determinism
  performance
  incremental
  memory
  limitations

Cross-language comparison

Stage performance analysis

Memory-retention analysis

Incremental analysis

Open findings

Decision

TASK 3 readiness
```

---

# 58. Final verification

After all bounded TASK 2 corrections rerun:

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release
```

Rerun all validation/benchmark measurements affected by code changes.

Do not report pre-change performance as final evidence for post-change code.

---

# 59. Reproducibility requirements

Record enough information to reproduce the campaign:

```text
RepoDex SHA
RepoDex dirty state

corpus SHAs
corpus dirty state where applicable

commands
environment

file-size limit
ignore policy

thread count

benchmark run directory

audit regions
audit matching rules

validation acceptance criteria
```

Do not rely only on shell history.

---

# 60. Commit policy

Commit TASK 2:

```text
code fixes
regression fixtures
tests
manifests
small scripts
documentation
summaries
```

Do NOT commit:

```text
third-party repositories
large raw benchmark output
build artifacts
profiler dumps
machine-specific absolute paths
secrets
```

Do not push unless explicitly instructed.

---

# 61. Final report

Provide:

```text
1. validation_status:
   VALIDATION_COMPLETE
   or
   VALIDATION_BLOCKED

2. architecture_recommendation:
   GO
   CONDITIONAL_GO
   NO_GO
   or
   NOT_ISSUED

3. exact TASK 1 commit validated

4. final TASK 2 commit SHA

5. corpora evaluated:
   URL
   SHA
   language
   supported files
   processed bytes
   LOC

6. verification results

7. parsing/recovery results per language

8. processing coverage and correct recovery denominators

9. extraction audit methodology

10. extraction results per language/category:
    TP
    FP
    FN
    precision
    recall
    sample size

11. source-range validation

12. determinism validation

13. real-file incremental correctness

14. synthetic benchmark results

15. real repository full-scan results

16. stage timing breakdown

17. throughput denominators

18. process memory measurements

19. retention A/B/C memory comparison

20. incremental performance

21. large/deep workload results

22. all unresolved BLOCKER/HIGH findings

23. fixes made during TASK 2

24. pre-fix/post-fix evidence where relevant

25. anything NOT MEASURED

26. optional old RepoDex comparison

27. exact evidence behind architecture recommendation

28. TASK 3 readiness

29. smallest recommended TASK 3

30. raw benchmark artifact path
```

Do not hide negative results.

Do not invent missing data.

---

# 62. Overnight execution behavior

This task may run unattended.

Do not stop for every ordinary problem.

When safe and within authorized scope:

```text
diagnose
minimize
fix
add regression test
reverify
continue
```

Examples that should not automatically terminate the entire task:

```text
one file recovers
one adapter bug is found
one optional corpus is unavailable
one benchmark exposes a bounded defect
```

However stop the main benchmark campaign when a foundation blocker makes further numbers misleading.

Examples:

```text
canonical output nondeterministic

systematic source ranges corrupt

incremental/fresh normalized results disagree

priority language adapter fundamentally broken

scanner silently omits significant supported source

query truncation silently corrupts results
```

Attempt only bounded foundation corrections.

If not corrected:

```text
VALIDATION_BLOCKED
NOT_ISSUED
```

and preserve evidence.

---

# 63. Execution order

Execute in this order:

```text
1. Inspect TASK 1 results

2. Verify exact TASK 1 commit and rerun gates

3. Create and freeze TASK2_VALIDATION_PLAN.md

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

# Guiding principle

TASK 2 is not a competition to produce impressive benchmark numbers.

RepoDex ultimately exists to make AI agents much faster at understanding unfamiliar repositories.

A parser that is extremely fast but emits misleading facts is not useful.

A parser that is correct but cannot operate economically at repository scale is also not sufficient.

The desired foundation is:

```text
correct
deterministic
source-grounded
explicit about uncertainty
fast enough for repository analysis
memory-conscious
incrementally viable
extensible toward later semantic linking
```

without pretending that syntax analysis already provides repository semantics.

The final outcome must be evidence, not optimism.
