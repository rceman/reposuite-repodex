# Cross-file structural linking (TASK 3B)

TASK 3B adds the first cross-file layer on top of the TASK 3A repository fact
snapshot:

```text
RepositoryFactSnapshot
    -> repository structural model
    -> language-specific module/package/namespace relationships
    -> import/module link outcomes
```

RepoDex implements **these bounded structural import-link rules**. It does not
resolve imports in the semantic sense, and this document never claims it does.

## The central rule

Every cross-file relationship answers one question:

```text
WHY does RepoDex believe these two things are related?
```

A relationship without reproducible provenance is invalid, so every link record
carries its rule id, its source fact locator, the written source form, the
evidence that produced the candidates, and the outcome.

## Outcomes

Every relationship has exactly one of four discrete outcomes. There is
deliberately **no numeric confidence score**: a discrete outcome plus explicit
evidence is easier to audit than a float.

| Outcome | Meaning |
| --- | --- |
| `Exact` | Under this rule's documented structural assumptions there is exactly one candidate. It never means "the runtime target". |
| `Ambiguous` | The rule produced more than one candidate, or a candidate that cannot be proven unique. Every candidate is kept. |
| `Unresolved` | The rule applied and produced no candidate. |
| `OutOfScope` | The relationship is deliberately outside this task. It is recorded, never silently dropped. |

The four are never collapsed into each other:

```text
multiple candidates        -> never reduced to the first one
no candidate               -> never replaced by a guess
external / out-of-scope    -> never reported as unresolved
absence of evidence        -> never reported as a semantic negative
```

One consequence is worth stating explicitly, because it looks odd at first:
`python.absolute_import.local_candidate` can be `Ambiguous` with **one**
candidate. "Ambiguous" here means "not uniquely determined", not "more than
one". An absolute Python import with exactly one repository candidate is still
not determined, because `sys.path` can shadow it.

## Provenance

Each link record carries:

```text
rule_id                  stable machine-readable rule id (a compatibility ABI)
source                   snapshot-local fact locator
written                  the written source form of the target
evidence                 the structural facts that produced the candidates
metadata                 content-digested repository metadata this rule read
outcome                  the discrete outcome
```

## Relationship locators

A relationship endpoint reuses TASK 3A's file-local identity:

```text
relative_path + fact kind + local fact id   (source occurrence)
relative_path + declaration id              (declaration target)
entity id                                   (structural entity target)
```

No permanent semantic symbol identity is introduced. A file-local fact id is
explicitly **not** promised to be stable across source edits.

## Artifact format

```text
<links-dir>/
  manifest.json     schema/rule ABI versions, dependencies, counts, digest
  links.jsonl       one canonical relationship per line
  entities.jsonl    one canonical structural entity per line
```

`manifest.json` records:

```text
link_manifest_version        1
link_schema_version          1
link_rule_abi_version        1
snapshot_digest              the exact TASK 3A snapshot this artifact depends on
snapshot_schema_version      the snapshot's normalized schema version
snapshot_analyzer_fingerprint the snapshot's analyzer fingerprint
link_fingerprint             derived-link compatibility fingerprint
link_fingerprint_text        its human-readable form
link_digest                  canonical digest of the links and entities
links / structural_entities  counts
outcomes / kinds             count summaries
rules                        the complete rule documentation registry
metadata                     content-digested metadata dependencies
```

The artifact does **not** contain a second copy of the normalized file facts.
Every relationship references the snapshot through a locator. Measured size is
9.7%–16.8% of the snapshot it derives from (see `TASK3B_RESULTS.md`).

Canonical output excludes absolute paths, timestamps, PIDs, temporary
directories, filesystem traversal order and random identifiers. `link_id` is a
pure function of the rule, the source locator and the written form.

## Derived-link fingerprint

`LinkFingerprint` covers only what changes link *semantics*:

```text
link manifest version
link schema version
link rule ABI version
per-language resolution-policy versions (Rust, Go, Python, PHP)
```

Changing any of these invalidates the derived artifact. It does **not**
invalidate the underlying TASK 3A snapshot, which has its own independent
fingerprint.

## Metadata dependencies

A rule that reads repository metadata records:

```text
relative_path      the metadata file
content_digest     SHA-256 of its bytes
present            whether the file exists
rule_id / field    the consuming rule and the field it read
value              the parsed value (or `malformed:<reason>`)
```

Only repository-root `go.mod` is parsed today, because only
`go.import.local_module` requires it. There is no generic config-file framework.

Absence is a dependency too. An absent `go.mod` is recorded with the digest of
empty bytes and `present: false`, so **creating** the file invalidates a
previous artifact. `links verify` fails when a `present: false` dependency now
exists, or when a `present: true` dependency is missing or has a different
digest.

## Verification

```bash
reposuite-repodex links verify <links-dir> --snapshot <snapshot-dir> [--repository <repo>]
```

Verification checks:

```text
manifest structure and schema/rule ABI versions
the recorded snapshot digest against the supplied snapshot
the recomputed snapshot digest of that snapshot
canonical ordering of links and entities
no duplicate link ids
every rule id appears in the manifest's rule registry
every source locator exists in the snapshot
every declaration/file candidate locator exists in the snapshot
every structural candidate entity id exists in the artifact
the canonical link digest
metadata dependencies (when --repository is supplied)
```

A valid link artifact means:

```text
internally consistent with the recorded snapshot and metadata
```

It does **not** mean:

```text
all relationships are semantically correct at runtime
```

## Rust rules

### `rust.mod.standard_file`

```text
input syntax        mod <name>;   (a Module declaration with no inline body)
repository assumption  the declaring file's directory is the module directory
metadata dependency none
Exact               exactly one of <dir>/<name>.rs or <dir>/<name>/mod.rs exists
Ambiguous           both exist
Unresolved          neither exists
OutOfScope          never
known exclusions    #[path = "..."] attributes, non-standard module mechanisms,
                    macro-generated modules, cfg evaluation, the crate root itself
candidate derivation  the two standard source-file forms for the declaring
                    file's module directory
```

Inline modules (`mod foo { ... }`) stay structural content of the same file and
never produce a fake external file link.

### `rust.use.crate_path`

```text
input syntax        use crate::a::b::C;  /  use crate::a::{b, c};  /  use crate::a::*;
repository assumption  crate roots are files named lib.rs or main.rs
metadata dependency none
Exact               the whole module prefix resolves to one module and the final
                    segment names exactly one declaration or child module
Ambiguous           the module prefix resolves but the final segment names more
                    than one declaration
Unresolved          no crate root was identified, a prefix step matched no
                    module, or the final segment matched nothing
OutOfScope          never (paths not beginning with `crate` use the rule below)
known exclusions    glob semantics, re-exports, the prelude, macro-generated
                    names, trait lookup, self::/super:: paths, external crates
candidate derivation  walk the path from the crate root; each non-final segment
                    must match a reachable child module; the final segment may
                    match declarations or child modules
```

### `rust.use.non_crate_path`

```text
input syntax        use self::... / use super::... / use std::... / use <extern>::...
Exact               never
OutOfScope          always, with the written path preserved
```

Non-`crate` paths are recorded explicitly rather than silently omitted, so the
"no relationship here" case is visible in the artifact.

## Go rules

### `go.package.same_directory`

```text
input syntax        package <name>
repository assumption  package membership is directory-based over indexed files
metadata dependency none
Exact               always: the declaring file belongs to the structural package
                    group (<directory>, <package name>)
Ambiguous           never
Unresolved          never
known exclusions    build tags are not evaluated, so membership is structural
                    over indexed files, not a particular Go build configuration
candidate derivation  files in one directory declaring the same package name form
                    one structural package group
```

`package foo` and `package foo_test` share a directory and stay **distinct**
groups. They are never merged.

### `go.import.local_module`

```text
input syntax        import "path"   (including blank `_` and dotted forms)
repository assumption  the repository root holds go.mod
metadata dependency go.mod (content digest, module declaration)
Exact               the import is inside the module and exactly one package group
                    exists at the mapped directory
Ambiguous           more than one package group exists at that directory
Unresolved          the import is inside the module but no package group exists
                    at the mapped directory
OutOfScope          never (imports outside the module use `go.import.external`)
known exclusions    dependency inspection or download, build tags, vendoring
                    semantics, replace/exclude directives
candidate derivation  the import is local only if it equals the module path or
                    begins with the module path followed by `/`; the remainder
                    maps to a repository directory, then to package groups there
```

The prefix test is on **path segments**, not text: with module path
`example.com/project`, the import `example.com/projectile` is **not** local.

### `go.import.external`

```text
Exact / Ambiguous / Unresolved   never
OutOfScope          the import is outside the local module, or go.mod is missing
                    or malformed so localness cannot be proven
```

A missing or malformed `go.mod` never yields a local link. The import is
recorded as external/out of scope, and the dependency on the metadata's
*absence* or malformed state is recorded so a later change invalidates the
artifact.

## Python rules

Python resolution is environment-dependent, so these rules are deliberately
conservative. The documented policy is **P-PY-1: the repository root is the
package root.**

### `python.relative_import.package_path`

```text
input syntax        from .name import x   /  from . import name   /  from ..pkg import x
repository assumption  policy P-PY-1; a file's package path is its directory
                    path relative to the repository root
metadata dependency none
Exact               exactly one candidate
Ambiguous           more than one candidate
Unresolved          no candidate, the import escapes above the repository root,
                    or the importing file is not inside a package
known exclusions    dynamic imports, namespace packages, sys.path manipulation,
                    installed distributions, .pth files
candidate derivation  levels are resolved against the importing file's package
                    path. `from .name import x` matches module files
                    (`pkg/name.py`, `pkg/name/__init__.py`). `from . import name`
                    additionally matches package attributes: declarations named
                    `name` in the files that define the base package, because the
                    item may denote a submodule *or* a package attribute
```

The `written` field renders both `from . import name` and `from .name import x`
as `.name`, because that is the written target in both cases. The raw import
form is carried separately in the provenance evidence as
`written_module=<none>` or `written_module=name`, so an auditor can tell them
apart without guessing.

### `python.absolute_import.local_candidate`

```text
input syntax        import pkg.mod   /  from pkg.mod import x
repository assumption  policy P-PY-1
metadata dependency none
Exact               NEVER. This rule can never be exact.
Ambiguous           at least one repository module file matches the written dotted
                    path — including exactly one match, because sys.path can
                    shadow it
Unresolved          never
known exclusions    sys.path, editable installs, namespace packages, runtime
                    environment, installed distributions
candidate derivation  the written dotted path maps to `pkg/mod.py` and
                    `pkg/mod/__init__.py` under the repository root
```

A matching local module is exposed as a candidate, never as semantic truth.

### `python.absolute_import.external`

```text
Exact / Ambiguous / Unresolved   never
OutOfScope          the absolute import matches no repository module
```

### Dynamic imports

```python
importlib.import_module(name)
__import__(name)
```

remain ordinary call-like syntax in the snapshot and produce no relationship.
No runtime resolution is attempted.

## PHP rules

PHP gives a useful syntax-grounded namespace layer.

### `php.namespace.declaration`

```text
input syntax        namespace App\Service;
repository assumption  namespace declarations are file-scoped or block-scoped;
                    nested namespace blocks derive their full name from the
                    enclosing namespace chain
metadata dependency none
Exact               always: the namespace declaration belongs to its structural
                    namespace entity, and each class-like declaration inside it
                    belongs to the same entity under its syntactic qualified name
Ambiguous           never
Unresolved          never
known exclusions    global-namespace declarations receive no invented namespace
                    entity; Composer/PSR-4 autoload availability is not claimed
candidate derivation  the qualified name is `namespace + "\" + written declaration
                    name`, composed from explicit namespace syntax
```

This is a **syntactic qualified name**, not proof of autoload or runtime
availability.

### `php.use.qualified_name`

```text
input syntax        use App\Service\Foo;   /  use App\Service\Foo as Bar;
                    use App\Service\{Foo, Bar};
repository assumption  written qualified names match written qualified
                    declarations
metadata dependency none
Exact               exactly one syntactic qualified declaration matches
Ambiguous           more than one matches (e.g. the same class declared twice)
Unresolved          no declaration matches, but a declared namespace prefix does
known exclusions    Composer autoload resolution, PSR-4 assumptions, aliases do
                    not change the written imported target, function/constant
                    imports are handled by a separate rule
candidate derivation  textual match of the written qualified name against the
                    syntactic qualified declarations; a grouped `use A\B\{C, D};`
                    composes the shared prefix with each entry
```

An alias is recorded but does not change the written imported target.

### `php.use.external`

```text
Exact / Ambiguous / Unresolved   never
OutOfScope          no declaration matches and no declared namespace prefix
                    contains the written name
```

### `php.use.non_class_import`

```text
input syntax        use function App\Service\helper;   /  use const App\Service\VERSION;
Exact / Ambiguous / Unresolved   never
OutOfScope          always
```

Function and constant imports are kept separate and are **never** linked to
class declarations.

## Not implemented

RepoDex does not implement, and TASK 3B does not add:

```text
call-target resolution
method dispatch
receiver or type resolution
general reference resolution
inheritance resolution
trait implementation resolution beyond existing syntax facts
type inference
overload resolution
dynamic dispatch
framework routing
Laravel container resolution, Django framework semantics,
Go interface dispatch, Rust trait dispatch
behavior graphs or behavior routes
natural-language locate(), semantic search, fuzzy search,
full-text search, ranking
LLM, embeddings, vector search
LSP, gopls, rust-analyzer, Pyright, PHPStan, Psalm
watcher, daemon, MCP, HTTP/RPC
databases, graph databases
persistent Tree-sitter trees
```

There is **no fuzzy, full-text or semantic search**. TASK 3A's exact lookup
remains available and is also exact: it matches written names, targets and
callees exactly and never collapses equal names into one entity.

## No call linking

Even when an import makes a call target look obvious, TASK 3B produces no call
edge:

```python
from foo import bar
bar()          # TASK 3B: an import relationship. NOT a call edge.
```

That belongs to TASK 3C.

## CLI

```bash
reposuite-repodex links build <snapshot-dir> --repository <repo> --output <links-dir> [--json]
reposuite-repodex links verify <links-dir> --snapshot <snapshot-dir> [--repository <repo>] [--json]
reposuite-repodex links stats <links-dir> [--json]
reposuite-repodex links show <links-dir> <path> [--json]
reposuite-repodex links exact|ambiguous|unresolved|out-of-scope <links-dir> [--json]
```

`links build` never mutates the TASK 3A snapshot.

## Library API

```rust
use repodex::links::{build_links, verify, LinkIndex};

let outcome = build_links(snapshot_dir, Some(repository), links_dir)?;
let index = LinkIndex::load(links_dir)?;

index.from_source("src/lib.rs");              // relationships written in a file
index.targeting_file("src/util.rs");          // relationships naming a file
index.targeting_declaration("src/util.rs", 0);// relationships naming a declaration
index.exact();                                // by outcome
index.ambiguous();
index.unresolved();
index.out_of_scope();
index.by_rule(rule::RUST_USE_CRATE_PATH);     // by rule
index.by_kind("use_path");                    // by kind
index.counts_by_rule();
```

This is deliberately not a query language and not a search engine. Every
operation is an exact filter over derived relationships.
