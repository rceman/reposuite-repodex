# RepoSuite RepoDex — TASK 1

## Rust + Tree-sitter Multi-Language Foundation Spike

Repository:

```text
https://github.com/rceman/reposuite-repodex
```

This is the first implementation task for the new RepoSuite RepoDex repository.

The repository is intended to start from a minimal/empty state. Before writing anything, inspect the actual repository tree and existing instructions. Do not overwrite unexpected existing work.

Do not assume access to any previous conversations, plans, reviews, or implementations. This task is self-contained.

---

# 1. Objective

Bootstrap a new Rust implementation of **RepoSuite RepoDex** and establish the first deterministic repository-analysis foundation using Tree-sitter.

Current language priority:

```text
1. Rust
2. Go
3. Python
4. PHP
```

The purpose of TASK 1 is to prove that we can build a small, correct, deterministic syntax-analysis foundation suitable for later RepoDex work.

Required pipeline:

```text
repository files
    ↓
language detection
    ↓
Tree-sitter parsing
    ↓
language-specific extraction
    ↓
normalized source-grounded syntax facts
```

TASK 1 explicitly stops before:

```text
semantic resolution
repository map
cross-file graph
natural-language navigation
investigation memory
```

Those are future tasks.

---

# 2. Fundamental semantic boundary

Tree-sitter provides a **source-grounded syntactic interpretation** of source bytes according to a particular grammar version.

Tree-sitter does NOT prove semantic correctness.

Examples:

```text
obj.F()
```

does not prove which method `F` is invoked.

```text
Thing()
```

in Python does not prove whether `Thing` is:

```text
class
function
callable object
alias
dynamic binding
```

```text
pkg.F()
```

in Go does not by itself prove a resolved package function target.

A syntactically valid tree may also represent semantically invalid code.

Tree-sitter may recover from damaged source using:

```text
ERROR
MISSING
```

nodes.

Therefore the architectural boundary must remain:

```text
Tree-sitter syntax
      ↓
language adapter
      ↓
normalized unresolved syntax facts
      ↓
FUTURE semantic resolver
      ↓
FUTURE repository map
      ↓
FUTURE navigator
```

Do not introduce semantic claims into the normalized TASK 1 model.

---

# 3. Implementation language and infrastructure scope

RepoSuite RepoDex must be implemented in:

```text
Rust
```

Use stable Rust.

Use one normal Rust package containing:

```text
library
CLI binary
tests
fixtures
benchmark harness
documentation
```

TASK 1 scanning and TASK 2 baseline validation are intentionally:

```text
single-threaded
```

Do not add parallel repository scanning yet.

The following systems are outside the authorized scope:

```text
Tokio
async runtime
database
daemon
watcher
MCP
HTTP server
RPC
GUI
LLM APIs
embeddings
vector databases
distributed processing
language servers
```

If a demonstrated blocker appears to require one of these systems:

```text
document the blocker
explain why it would be needed
stop that line of implementation
request a future scope decision
```

Do not introduce the system autonomously.

---

# 4. Suggested repository shape

Use a compact Rust layout.

A reasonable structure is:

```text
Cargo.toml
Cargo.lock
README.md

src/
  lib.rs
  main.rs
  cli.rs

  model/
    mod.rs

  parser/
    mod.rs
    registry.rs
    rust.rs
    go.rs
    python.rs
    php.rs

  scanner/
    mod.rs

  paths/
    mod.rs

queries/
  rust/
  go/
  python/
  php/

fixtures/
  rust/
  go/
  python/
  php/

tests/

benches/

docs/
  ARCHITECTURE.md
  LANGUAGE_SPIKE.md
  SPIKE_RESULTS.md
```

This tree is guidance.

Do not create meaningless one-file abstractions or empty modules merely to imitate it.

Prefer a smaller implementation over speculative architecture.

---

# 5. Runtime namespace

All future RepoDex runtime state belongs under:

```text
~/reposuite/repodex/
```

Provide one central path resolver.

Conceptual paths:

```text
~/reposuite/repodex/
├── config/
├── cache/
├── indexes/
├── projects/
├── benchmarks/
└── tmp/
```

Support:

```text
REPOSUITE_REPODEX_HOME
```

as the root override.

Never use:

```text
~/.repodex
~/.reposuite/repodex
~/repodex
~/reposuite/repodex/nav
```

Use platform-appropriate home resolution.

Do not treat literal `~` as a filesystem path.

For TASK 1:

```text
languages
parse
scan
```

must not create persistent RepoDex directories during normal execution.

The path module only resolves paths.

Tests must never write into the real user home directory.

Use injected temporary roots.

Avoid process-global environment mutation in parallel tests.

---

# 6. Tree-sitter dependencies

Use maintained Rust Tree-sitter bindings and maintained grammars for:

```text
Rust
Go
Python
PHP
```

Requirements:

```text
pin compatible dependency versions
commit Cargo.lock
do not use floating Git branches
verify runtime/grammar compatibility through tests
```

Record:

```text
Rust toolchain
tree-sitter runtime version
Rust grammar crate/version/repository/license
Go grammar crate/version/repository/license
Python grammar crate/version/repository/license
PHP grammar crate/version/repository/license
```

Do not impose an arbitrary minimum dependency release age.

Use current compatible versions based on maintainability and compatibility.

Prefer packaged generated grammars.

Do not introduce grammar generation infrastructure unless a real blocker proves it necessary.

Document native compiler requirements where relevant because Tree-sitter grammars may include generated C code.

PHP may expose more than one parser entry point. Verify which grammar mode is appropriate for ordinary `.php` files and mixed PHP/HTML source.

---

# 7. Normalized model terminology

TASK 1 models **syntax occurrences**, not resolved repository semantics.

Prefer concepts such as:

```text
SourceFile
Scope
Declaration
ImportOccurrence
ReferenceOccurrence
CallLikeOccurrence
TestEvidence
SourceRange
Diagnostic
```

Avoid premature concepts such as:

```text
ResolvedSymbol
ResolvedReference
CallEdge
RepositorySymbol
```

The normalized model must not expose:

```text
tree_sitter::Node
tree_sitter::Tree
capture IDs
grammar-specific node identifiers
Tree-sitter internal node IDs
```

Language-specific syntax interpretation belongs inside the language adapter.

---

# 8. Experimental serialized schema

Machine-readable normalized output is experimental.

Include an explicit schema version.

For example:

```json
{
  "schema_version": 1
}
```

The exact representation may differ, but every serialized normalized result must expose a version.

Do not promise compatibility beyond this spike.

---

# 9. SourceRange contract

This contract is mandatory.

Every relevant normalized fact uses source positions referring to the original input bytes.

Byte ranges:

```text
[start_byte, end_byte)
```

must be half-open.

Positions:

```text
rows    = zero-based
columns = zero-based UTF-8 byte columns
```

Columns are NOT:

```text
display columns
Unicode scalar counts
UTF-16 code-unit offsets
1-based source columns
```

Preserve original source bytes.

Do not normalize newlines before calculating positions.

Test:

```text
LF
CRLF
UTF-8 multibyte text
missing final newline
```

Declaration-related facts must distinguish:

```text
full declaration range
name/selection range
optional body range
```

These ranges have different meanings and must not be conflated.

---

# 10. Fact IDs

Normalized facts may use snapshot-local IDs for internal relationships.

Requirements:

```text
deterministic for the same file snapshot
not promised to survive edits
not based on Tree-sitter node identity
not based only on line number
not random
```

The exact deterministic scheme is implementation-defined.

Do not design a persistent cross-edit symbol identity scheme in TASK 1.

That belongs to later repository indexing work.

---

# 11. Declaration model

Represent syntactically declared constructs.

Required declaration categories should cover where applicable:

```text
Module
Package
Namespace

Function
Method

Struct
Class
Interface
Trait
Enum

NamedType
TypeAlias

Constant
Variable

Field
Property
```

Not every language needs every kind.

Important Go distinction:

```go
type UserID string
```

is a named type declaration.

```go
type UserID = string
```

is a type alias.

Do not collapse them.

A declaration should retain as applicable:

```text
snapshot-local ID
written name
kind
full declaration range
name range
optional body range
containing scope
language
repository-relative path
language-specific syntactic metadata needed by this task
```

Do not fabricate bodies for declarations that do not have one.

Do not infer fully qualified semantic identities beyond source syntax.

---

# 12. Lexical/syntactic containment

Preserve enough containment to distinguish declarations and call-like occurrences in different contexts.

Scopes may include:

```text
file/module/namespace
class
interface
trait
impl block
function
method
anonymous callable
nested callable
```

This is syntactic containment, not name resolution.

Important language distinctions:

## Go

A method is syntactically declared at file scope with a receiver.

It is not lexically nested inside the receiver type.

Record receiver syntax separately.

## Rust

An:

```rust
impl Type
```

block is not the declaration of `Type`.

Record:

```text
impl target
optional implemented trait
```

as syntax relationships.

## Python

Nested functions/classes/lambdas must preserve containment.

## PHP

Namespace/class/interface/trait/function/method/closure contexts must remain distinct.

Anonymous functions and closures must have anonymous callable scopes.

Do not invent fake global declaration names for them.

---

# 13. ImportOccurrence

Imports remain unresolved syntax occurrences.

Preserve as applicable:

```text
statement range
individual entry range
target as written
imported item
alias
relative level/path
wildcard/glob form
special import category
```

Grouped imports must expose their individual components while retaining shared context.

Do not resolve imports to files/packages/modules.

Do not confuse different PHP `use` forms:

```text
namespace import
trait composition
closure capture
```

---

# 14. ReferenceOccurrence scope

TASK 1 does not require a complete identifier-reference engine.

Required references are limited to useful explicit syntax relationships, including:

```text
call-like target expressions

Go receiver type syntax

Rust impl target
Rust implemented trait path

Python base-class expressions where extracted

PHP extends
PHP implements
```

Additional explicit header relationships may be extracted where needed by required fixtures.

All references remain unresolved.

Retain:

```text
written syntax
role
range
containing scope
optional simple name selection
```

Do not claim target identity.

---

# 15. CallLikeOccurrence

Represent syntactic call-shaped expressions as:

```text
CallLikeOccurrence
```

not resolved calls.

Capture:

```text
full expression range
callee/target expression range
written target form
containing scope
syntactic call-like category
```

Useful categories may include:

```text
plain name
qualified path
member/selector
static/scoped form
indirect expression
explicit construction
```

Important ambiguities:

### Go

```go
T(x)
```

may be:

```text
conversion
call
```

without semantic information.

### Rust

Tuple-struct construction can resemble a normal call.

### Python

```python
Thing()
```

does not prove constructor semantics.

### PHP

Distinguish:

```php
foo($arg);
foo(...$args);
$callable = foo(...);
```

The first two are invocation syntax.

The third is first-class callable creation and must not be counted as invocation.

Do not resolve these cases beyond what syntax safely tells us.

---

# 16. TestEvidence

Tests are not a separate declaration kind.

A test remains:

```text
Function
Method
```

with attached:

```text
TestEvidence[]
```

Possible evidence:

```text
attribute
decorator
function/method naming convention
file naming convention
class/base-class syntax
signature shape
```

Document the exact syntactic policy.

Do not claim actual test-runner collection semantics.

---

# 17. General literal extraction

Do NOT implement generic literal indexing in TASK 1.

Do not create normalized facts for every:

```text
string
integer
boolean
float
```

RepoDex may later need searchable anchors such as:

```text
error strings
config keys
routes
event names
status values
```

but that should be added based on concrete future consumers.

---

# 18. Tree-sitter extraction strategy

For every language:

1. inspect the grammar structure;
2. inspect available upstream queries;
3. decide which extraction strategy is most appropriate.

Possible strategies:

```text
Tree-sitter Query
direct AST traversal
hybrid approach
```

Inspect upstream:

```text
tags.scm
locals.scm
highlights.scm
```

Record when a query is absent.

Do not automatically treat highlight captures as semantic facts.

Use:

```text
queries/<language>/*.scm
```

for substantial custom queries.

Do not bury large `.scm` query strings inside Rust source.

Requirements:

```text
compile shipped queries against pinned grammars in tests
reuse compiled queries during one command where practical
prefer named fields over positional assumptions
detect query match-limit exhaustion
never silently return truncated extraction as complete
```

Do not assume every Tree-sitter query predicate/directive is automatically implemented by the Rust execution path.

If upstream query behavior depends on unsupported predicates/directives, handle or avoid it explicitly.

---

# 19. Fixture coverage classification

Maintain a language construct coverage matrix.

Each relevant construct must be classified as:

```text
EXTRACT_REQUIRED
PARSE_PROBE
EXPLICITLY_UNSUPPORTED
```

Do not downgrade an `EXTRACT_REQUIRED` construct to `PARSE_PROBE` merely because implementation is difficult.

If an explicitly required construct cannot be supported:

```text
report the blocker
preserve failing evidence
do not hide the failure
```

---

# 20. Rust adapter

Implement substantive Rust support.

## EXTRACT_REQUIRED

Fixtures must cover:

```text
mod
nested inline mod
external mod declaration

use
grouped use
alias
self
super
crate
glob

struct
enum
trait
type alias

trait method declaration
trait method with default body

inherent impl
trait impl

free function
associated function
receiver method

const
static

nested function
closure

plain call syntax
qualified path call
generic call
method call

#[test]
multiple attributes
test negative cases

identically named methods in different impl contexts
```

## PARSE_PROBE / ambiguity coverage

Include:

```text
generics
lifetimes
where clauses
async syntax
raw identifiers
macro_rules!
macro invocation
cfg attributes
struct construction
tuple-struct construction
```

Do NOT:

```text
expand macros
evaluate cfg
pretend macro-generated declarations exist
```

---

# 21. Go adapter

Implement substantive Go support.

## EXTRACT_REQUIRED

Fixtures must cover:

```text
package

single import
grouped imports
renamed import
blank import
dot import

struct type
interface type
named type
type alias

generic type
generic function

free function
value receiver method
pointer receiver method

const
grouped const
iota
package variable

plain call-like syntax
selector call-like syntax
generic call-like syntax
indirect call-like syntax

anonymous function
nested anonymous function

_test.go
TestXxx
negative test case
table-driven test
t.Run closure
```

## PARSE_PROBE / ambiguity coverage

Include:

```text
embedded types
method expression
selector ambiguity
conversion vs call ambiguity
go statement
defer statement
short declaration
shadowing
build tags
```

Do NOT:

```text
evaluate build tags
resolve selectors
infer promoted methods
treat t.Run as proven test ownership
```

---

# 22. Python adapter

Implement substantive Python support.

## EXTRACT_REQUIRED

Fixtures must cover:

```text
import
multiple import
import alias

from import
relative import
wildcard import

module function
async function

class
method
async method

decorator

module assignment
annotated assignment

nested function
nested class
lambda

plain call
attribute call
chained call
indirect call

call in decorator
call in default argument

pytest-style test candidate
pytest negative candidate

unittest-style class/method candidate
unittest negative candidate
```

## PARSE_PROBE / ambiguity coverage

Include:

```text
comprehension
global
nonlocal
match
f-string expression
getattr(...)(...)
dynamic import
monkey-patching-shaped syntax
aliased TestCase import
```

Do NOT semantically resolve:

```text
decorators
inheritance
dynamic imports
monkey patches
actual pytest collection
```

Uppercase naming alone must not prove that a Python variable is semantically a constant.

---

# 23. PHP adapter

Implement substantive modern PHP support.

## EXTRACT_REQUIRED

Fixtures must cover:

```text
bracketed namespace
unbracketed namespace

use
grouped use
aliased use
use function
use const

class
interface
trait
enum

trait composition

function
method
static method
constructor

property
promoted constructor property
class constant

attribute

ordinary invocation
member invocation
nullsafe invocation
scoped/static invocation
indirect invocation
construction syntax

closure
arrow function
closure use capture

PHPUnit-style candidate
negative PHPUnit candidate

first-class callable syntax
```

## PARSE_PROBE / ambiguity coverage

Include:

```text
mixed HTML/PHP
multiple PHP regions
readonly/type syntax
variable functions
dynamic member names
trait adaptations
include
require
```

Do not confuse:

```text
namespace import
trait composition
closure capture
```

Do not claim Laravel/Lumen semantics.

---

# 24. Shared adversarial fixtures

Include cross-language cases for:

```text
empty file
comment-only file

UTF-8 multibyte text before extracted declarations

LF
CRLF
missing final newline
BOM/shebang where relevant

same declaration names in different contexts
deep nesting

incomplete declaration
broken expression
syntax damage between valid declarations

valid declaration before malformed region
valid declaration after malformed region

syntactically valid but semantically invalid source

generated-looking source

deterministically generated large source
```

Large synthetic fixture generation should be reproducible.

Avoid committing unnecessary multi-megabyte generated fixtures when they can be generated by the harness.

---

# 25. Parser recovery contract

Tree-sitter recovery must be explicit.

Detect:

```text
ERROR nodes
MISSING nodes
```

For required valid fixtures:

```text
unexpected ERROR = failure
unexpected MISSING = failure
```

For intentionally malformed fixtures:

```text
recovery is expected
recovery must be reported
neighboring recoverable facts should remain available
missing declaration names must not be fabricated
```

Document the policy for normalized facts intersecting recovery regions.

Do not classify a recovered tree as a clean parse.

---

# 26. Correctness testing requirements

Tests must accompany every adapter.

Do not implement four adapters and then write tests later.

For small required fixtures, assert **exact expected fact sets** where practical.

Assertions must cover:

```text
declaration kind
name
containment
receiver/impl/base relationships where required
imports
aliases
call-like forms
test evidence
source ranges
duplicate absence
```

Positive assertions such as:

```text
"contains declaration X"
```

alone are insufficient.

Tests must also verify that unexpected normalized facts are not emitted.

Tree snapshots may be used as supplementary diagnostics.

They are not the primary correctness oracle.

---

# 27. Canonical normalized facts

Validation must be able to access full canonical per-file normalized facts.

Aggregate counts alone are insufficient.

TASK 1 must provide at least one of:

```text
small library-level validation helper
experimental CLI flag exporting per-file facts
```

A convenient experimental CLI shape is acceptable, for example:

```text
reposuite-repodex scan <repo> --json --include-facts
```

The exact syntax is not a compatibility promise.

Canonical facts must be deterministically ordered.

TASK 2 must be able to compare:

```text
complete canonical facts
```

or:

```text
a reproducible digest computed from complete canonical facts
```

without relying only on aggregate counters.

---

# 28. Determinism

For identical source bytes and configuration:

```text
normalized facts must be identical
```

Verify:

```text
repeated parse determinism
repeated scan determinism
filesystem discovery order independence
hash-map ordering independence
no random IDs
no timestamps in canonical facts
no absolute checkout root in canonical facts
```

Additionally:

copy the same repository content under a different filesystem root and verify canonical normalized output remains equivalent.

This proves that absolute checkout paths do not leak into canonical identity/output.

---

# 29. Scanner

Implement a minimal deterministic repository scanner.

Supported extensions:

```text
.rs
.go
.py
.php
```

Requirements:

```text
recursive walk
deterministic relative path ordering
do not follow file or directory symlinks
dispatch supported files to correct adapter
continue after individual file failures
report failures by stage
```

---

# 30. Ignore policy

The scanner policy must be deterministic across machines.

Always prune:

```text
.git/
target/
vendor/
node_modules/
__pycache__/
.venv/
venv/
```

If `.gitignore` support is implemented:

```text
only repository-root-contained ignore policy may affect results
do not read global Git excludes
do not inherit ignore rules from parent directories outside scan root
do not depend on developer-specific global configuration
```

Document exact behavior.

Do not add complex ignore semantics in TASK 1.

---

# 31. File input rules

Use UTF-8 source input for this spike.

Invalid UTF-8:

```text
must not panic
must be reported as unsupported input
```

Default maximum supported file size:

```text
8 MiB
```

Allow an explicit override.

Reading must remain bounded.

Do not trust only a metadata size check.

Handle:

```text
file disappears during scan
permission/read error
file changes during access where detectable
```

without crashing the complete scan.

---

# 32. CLI

Implement at minimum:

```bash
reposuite-repodex languages
reposuite-repodex parse <file>
reposuite-repodex scan <repository>
```

Support JSON:

```bash
reposuite-repodex parse <file> --json
reposuite-repodex scan <repository> --json
```

Provide a way to export full canonical facts for validation.

CLI and JSON formats are:

```text
EXPERIMENTAL
```

Do not promise permanent compatibility.

`languages` reports:

```text
Rust
Go
Python
PHP
```

and supported file extensions.

`parse` should report:

```text
schema version
language
analysis status
diagnostics
normalized facts
```

`scan` should report at least:

```text
files visited
supported candidates

parsed clean
parsed with recovery

unsupported input
size-limit skips
read failures
parser failures
extraction failures

declarations
imports
references
call-like occurrences
test candidates

bytes actually processed
elapsed time
```

Define counter denominators clearly.

JSON stdout must contain JSON only.

Logs go to stderr.

---

# 33. Exit behavior

CLI exit behavior may remain experimental but must be explicit and documented.

A recommended convention is:

```text
0 = completed without analysis/recovery errors
1 = completed but one or more files had recovery/analysis failures
2 = invocation/setup failure
```

If another simple convention is chosen, document it.

Do not silently report partial failure as clean success.

---

# 34. Incremental parsing experiment

Tree-sitter incremental parsing is part of TASK 1.

Do not confuse:

```text
incremental syntax parsing
```

with:

```text
incremental repository indexing
```

Implement a reusable correctness harness:

```text
parse original source
    ↓
construct edited source
    ↓
compute InputEdit
    ↓
edit old tree
    ↓
incrementally parse new bytes
    ↓
independently full-parse same bytes
    ↓
extract complete normalized facts from both
    ↓
compare
```

For Rust and Go require:

```text
same-byte-length identifier rename
insertion
deletion
newline insertion
UTF-8-sensitive edit
CRLF-sensitive edit
introduce syntax damage
repair syntax damage
multiple consecutive edits
edit near EOF
```

For Python and PHP require at least one substantive incremental-equivalence scenario each.

Compare:

```text
node structure sufficiently to detect divergence
node ranges
ERROR/MISSING state
normalized facts
diagnostics
```

S-expression equality alone is insufficient.

`changed_ranges` may be recorded as evidence.

Do not use `changed_ranges` as the sole extraction invalidation mechanism.

For TASK 1:

```text
after every edit, re-extract the complete edited file
```

Do not implement fine-grained fact patching.

---

# 35. Incremental benchmark reset discipline

Correctness sequences may intentionally apply multiple consecutive edits.

Performance measurements are different.

For repeated timing measurements:

```text
reset to the same original source/tree state before every sample
apply the same edit
measure the same workload
```

Do not benchmark a sequence of progressively different documents and treat it as repeated measurements of one workload.

---

# 36. Synthetic benchmark harness

Build benchmark infrastructure in TASK 1.

Do not yet require the large real-repository validation campaign.

Synthetic workloads should include approximately:

```text
100 KiB
1 MiB
4 MiB
```

per language where practical.

Generated source must contain actual extractable constructs.

Do not create benchmark files consisting only of:

```text
comments
strings
whitespace
```

Also test:

```text
substantial nesting
file near 8 MiB limit
file above default 8 MiB limit
```

Measure separately where possible:

```text
file discovery
parser/grammar initialization
raw parsing
normalized extraction
parse + extraction
incremental parsing
incremental parse + complete extraction
```

Report:

```text
bytes processed
files processed
duration
MB/sec
facts emitted
```

Throughput denominators must use:

```text
bytes actually processed
```

not total repository bytes including skipped content.

---

# 37. Benchmark memory

TASK 1 only needs benchmark support and documentation.

If reliable peak process memory measurement requires an external OS command, document that command instead of implementing a misleading allocator-only metric.

Do not claim Rust allocator statistics represent all Tree-sitter/native memory.

---

# 38. What TASK 1 does NOT benchmark

TASK 1 does NOT require:

```text
large third-party repository campaign
150k+ LOC real corpus
manual precision/recall audit
old Go RepoDex comparison
final Rust-vs-Go verdict
production SLA
interactive locate latency
```

These belong to TASK 2 or later.

---

# 39. Documentation

Create:

```text
README.md
docs/ARCHITECTURE.md
docs/LANGUAGE_SPIKE.md
docs/SPIKE_RESULTS.md
```

---

# 40. README requirements

Explain:

```text
what RepoSuite RepoDex is
what TASK 1 implements
what TASK 1 does NOT implement
supported languages
native prerequisites
build instructions
test instructions
parse command
scan command
benchmark command
runtime namespace
known limitations
```

Do not advertise future features as implemented.

---

# 41. ARCHITECTURE.md

Document the actual implemented boundaries.

Explicitly state:

## Tree-sitter provides

```text
syntax tree
source-grounded syntactic structure
ranges
syntactically visible declarations
syntactically visible imports
call-shaped expressions
recovery information
incremental parsing support
```

## Tree-sitter does not provide

```text
complete semantic validity
type resolution
cross-file symbol resolution
runtime dispatch
resolved call graph
reflection
dependency injection
framework semantics
behavior ownership
why a behavior occurs
```

Document:

```text
Parser
  ↓
Language Adapter
  ↓
Normalized Syntax Facts
  ↓
FUTURE Semantic Resolution
  ↓
FUTURE Repository Map
  ↓
FUTURE Navigator
```

Also document:

```text
SourceRange semantics
snapshot-local ID semantics
recovery handling
determinism requirements
query vs traversal strategy
important dependencies
```

---

# 42. LANGUAGE_SPIKE.md

For each language document:

```text
grammar crate
grammar version
Tree-sitter compatibility
available upstream query files
chosen extraction strategy
custom queries if any
important grammar quirks
EXTRACT_REQUIRED coverage
PARSE_PROBE coverage
explicit unsupported cases
known ambiguities
```

Sections:

```text
Rust
Go
Python
PHP
```

---

# 43. SPIKE_RESULTS.md

Report actual evidence only.

Include:

```text
TASK 1 summary

exact toolchain/dependencies

Rust
Go
Python
PHP

fixture correctness

recovery behavior

determinism

incremental correctness

synthetic benchmark observations

known limitations

unmeasured items

final foundation status
```

Use:

```text
NOT MEASURED YET
```

where appropriate.

Do not invent benchmark numbers.

Do not claim Rust is faster than the old RepoDex implementation.

---

# 44. Required verification

Before completion run:

```bash
cargo fmt --check
cargo check --locked
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release
```

Run the benchmark harness using its documented command.

Do not:

```text
silence warnings only to pass clippy
ignore required failing tests
weaken assertions to hide unsupported behavior
```

---

# 45. Explicit non-goals

Do NOT implement:

```text
persistent production index

SQLite
RocksDB
redb
LMDB

cross-file semantic resolver

resolved symbol graph
resolved reference graph
resolved call graph

repository map

behavior routes

natural-language locate

investigation memory

negative candidate memory

LLM
embeddings
vector search

gopls
rust-analyzer
Pyright
PHPStan
Psalm

framework adapters
Laravel/Lumen semantics

daemon
watcher
MCP
HTTP
RPC
GUI

parallel scanning

TypeScript
JavaScript
Svelte
SvelteKit

old RepoDex feature parity
```

Do not create placeholder frameworks for these systems.

---

# 46. TASK 1 completion gates

TASK 1 is complete only if:

```text
Rust project bootstrapped

Rust adapter substantive
Go adapter substantive
Python adapter substantive
PHP adapter substantive

coverage matrix exists

required extraction fixtures exist
required correctness tests pass

SourceRange contract tested

NamedType vs TypeAlias handled where applicable

containment tested

recovery tested

ERROR/MISSING handling exists

determinism tested

same content under different checkout roots produces equivalent canonical facts

repository scanner works

parse CLI works
scan CLI works

full canonical per-file facts can be obtained for validation

incremental equivalence harness exists

mandatory incremental cases pass

synthetic benchmark harness exists and runs

documentation matches actual behavior

all required Cargo verification passes
```

A language adapter is not complete merely because the grammar can parse a file.

---

# 47. TASK 1 final status

Final foundation status must be exactly one of:

```text
FOUNDATION_SPIKE_COMPLETE
```

Meaning:

```text
all mandatory TASK 1 gates passed
```

or:

```text
FOUNDATION_SPIKE_BLOCKED
```

Meaning:

```text
one or more mandatory gates remain failing or unexecuted
```

If blocked:

```text
list exact blockers
preserve evidence
do not pretend completion
```

Record the status in:

```text
docs/SPIKE_RESULTS.md
```

and in the final report.

This status describes completion of TASK 1.

It is NOT the final Rust architecture decision.

Do NOT issue:

```text
GO
CONDITIONAL GO
NO GO
```

in TASK 1.

That belongs to TASK 2.

TASK 2 must validate the exact TASK 1 commit SHA, not merely trust the status marker.

---

# 48. Commit policy

After successful verification:

```text
commit TASK 1 implementation and documentation
```

Do not push unless explicitly instructed.

Do not rewrite unrelated repository history.

---

# 49. Final report

Provide:

```text
1. foundation_status:
   FOUNDATION_SPIKE_COMPLETE
   or
   FOUNDATION_SPIKE_BLOCKED

2. final commit SHA

3. resulting repository structure

4. normalized model summary

5. SourceRange and ID contract

6. exact dependency versions

7. Rust support matrix

8. Go support matrix

9. Python support matrix

10. PHP support matrix

11. exact verification commands and results

12. recovery test results

13. determinism test results

14. incremental-equivalence results

15. synthetic benchmark results actually measured

16. known parser limitations

17. known extraction ambiguities

18. explicitly unsupported constructs

19. anything NOT MEASURED

20. benchmark commands

21. recommended TASK 2 considerations
```

Do not hide failures.

Do not invent measurements.

---

# 50. Execution order

Execute in this order:

```text
1. Inspect current repository

2. Bootstrap Cargo project and dependency set

3. Implement SourceRange, schema version,
   diagnostics, scopes, normalized facts

4. Implement parser registry

5. Rust adapter + fixtures + exact tests

6. Go adapter + fixtures + exact tests

7. Python adapter + fixtures + exact tests

8. PHP adapter + fixtures + exact tests

9. Implement deterministic scanner

10. Implement experimental CLI

11. Add adversarial/recovery tests

12. Add canonical full-fact export/validation path

13. Add determinism tests including different checkout roots

14. Implement incremental-equivalence harness

15. Run mandatory incremental cases

16. Implement synthetic benchmark harness

17. Run synthetic benchmark suite

18. Complete documentation

19. Run full required verification

20. Record FOUNDATION_SPIKE_COMPLETE
    or FOUNDATION_SPIKE_BLOCKED

21. Commit
```

Do not postpone correctness testing until after all adapters.

---

# Guiding principle

The success criterion is not code volume.

TASK 1 succeeds if this pipeline becomes trustworthy:

```text
source bytes
    ↓
Tree-sitter
    ↓
language-specific adapter
    ↓
small normalized unresolved syntax model
```

The foundation must be:

```text
correct
source-grounded
deterministic
explicit about recovery
measurable
small enough to understand
```

Only after that is proven should RepoDex move toward:

```text
repository map
cross-file semantic linking
navigation
task intent → implementation route
persistent investigation knowledge
```
