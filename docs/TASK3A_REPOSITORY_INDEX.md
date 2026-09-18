# RepoDex TASK 3A — Deterministic Repository Fact Snapshot & Incremental File-Level Index

This document describes what TASK 3A actually implements. It does not describe
planned systems, and it is not a compatibility promise: the snapshot artifact is
explicitly **EXPERIMENTAL**.

## 1. Where this sits

```text
source file
    -> Tree-sitter
    -> language adapter
    -> FileAnalysis / normalized unresolved facts      (TASK 1 / TASK 2, validated)
    -> RepositoryFactSnapshot                          (TASK 3A)
    -> persistent deterministic snapshot artifact      (TASK 3A)
    -> incremental file-level rebuild                  (TASK 3A)
```

TASK 3A is **not** semantic resolution. There is no cross-file symbol
resolution, no resolved reference, no resolved call, no call graph, no knowledge
graph, no inferred FQN, no type inference, no module resolution beyond written
syntax, no behaviour route, no `locate()`, no fuzzy or semantic search, no
embedding, no LLM, no LSP integration, no framework intelligence, no watcher, no
daemon, no MCP/HTTP/GUI, no parallel scanner and no database.

A repository snapshot is a deterministic collection of source-grounded file
analyses. It is not yet a repository semantic graph.

## 2. Concepts

```text
RepositoryFactSnapshot   the in-memory snapshot: manifest + file analyses
RepositoryManifest       the canonical, persisted description of one snapshot
IndexedFile              one indexed supported file's record in the manifest
FileArtifact             the persisted FileAnalysis for one indexed file
ContentDigest            SHA-256 of the source bytes read for one file
AnalysisDigest           SHA-256 of one file's canonical normalized facts
AnalyzerFingerprint      identity of the extraction pipeline
SnapshotDigest           SHA-256 over canonical snapshot content
BuildStats               work performed by one build or update
CoverageSummary          repository-level coverage by analysis status
FactTotals               repository-level fact totals
```

Deliberately absent: `RepositorySymbol`, `SemanticSymbol`,
`ResolvedDeclaration`, `ResolvedCall`, `ResolvedReference`.

## 3. Canonical repository identity

Canonical snapshot content never depends on:

```text
absolute checkout path
output directory
machine username / home directory
timestamp
process id
filesystem enumeration order
random identifier
```

Repository-relative path is the canonical file path identity. Two identical
repository trees checked out under different absolute roots produce byte-equal
manifests and the same snapshot digest; this is covered by tests and by the
measured campaigns.

The **output directory does not affect canonical content** because a snapshot
build excludes its own artifact from discovery. When the output (or the previous
snapshot, or the staging directory) lives inside the repository root, the walk
prunes it. Without that, the manifest's discovery counters would depend on where
the artifact was written and a snapshot could ingest a previous snapshot's JSON.

## 4. Digests

Two distinct SHA-256 digests, for two distinct jobs:

```text
content_digest = sha256(bytes actually read for the file)
analysis_digest = sha256(canonical normalized facts of the analysis)
```

TASK 2's FNV-1a canonical digest is a change detector, not a collision-resistant
hash. It is **not** used as authoritative source-content identity. TASK 3A adds
the small `sha2` RustCrypto crate (already present in the local cargo cache, so
the build stays reproducible offline) for both digests.

For an over-limit file the read stops at `max_file_size + 1` bytes, so
`content_digest` covers exactly the prefix that was read and the record also
stores `source_bytes` and `source_truncated`. Reuse requires all three to match.

## 5. Analyzer fingerprint and configuration identity

Source equality is not enough to reuse an analysis: after a grammar bump or an
extraction change, the same bytes must produce new facts. Every snapshot records
an `AnalyzerFingerprint`, computed from:

```text
SCHEMA_VERSION
ANALYSIS_ABI_VERSION
tree_sitter::LANGUAGE_VERSION
tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION
grammar crate name + version for each of rust, go, python, php
```

`ANALYSIS_ABI_VERSION` is a plain constant with a documented maintenance
contract: increment it whenever normalized facts for unchanged bytes could
change (fact shape, adapter extraction logic, recovery/match-limit policy, input
validation rules). It does not need to change for docs, tests or CLI formatting.

`SnapshotConfig` records the analysis configuration that decides *which* files
are analyzed and how:

```text
supported language set
max_file_size
respect_gitignore
IGNORE_POLICY_VERSION
SCHEMA_VERSION
```

Reuse is allowed only when **both** the content digest and the analyzer
fingerprint match **and** the configuration matches.

## 6. Artifact format (EXPERIMENTAL)

```text
<snapshot-dir>/
  manifest.json          canonical manifest, pretty JSON, human-inspectable
  files/
    <object-key>.json    one normalized FileAnalysis, compact JSON
```

The object key is `sha256("repodex-file-object-v1\n" + relative_path + "\n" +
content_digest)`, hex, without an algorithm prefix. It is deterministic,
collision-resistant, contains no absolute path, and changes with the content, so
an edited file can never collide with its previous revision's artifact.

Nothing serializes a Tree-sitter `Tree` or a source buffer. A persisted artifact
is exactly the normalized facts. There is no database, no binary layout and no
compaction step.

## 7. Build

```bash
reposuite-repodex index build <repository> --output <snapshot-dir> [--json]
```

```text
deterministic discovery (scanner::walk_files, pruned directories, sorted)
per-file bounded read + SHA-256 content digest
existing bounded input validation, unchanged
parse + extract supported files
preserve recovery / incomplete / failure status
persist one artifact per indexed file
write the canonical manifest
verify-free atomic publication
```

Unsupported extensions are counted but not indexed. Failed and skipped files are
**never silently omitted**: they get a record whose `analysis_status` says so.

## 8. Incremental update

```bash
reposuite-repodex index update <repository> \
  --previous <old-snapshot-dir> --output <new-snapshot-dir> [--json] [--allow-incompatible]
```

```text
UNCHANGED  relative path present in both, content identity identical
           -> reuse the persisted normalized analysis (artifact copied verbatim)

CHANGED    present in both, content identity differs
           -> parse the whole current file, extract the whole file, replace

ADDED      present now, absent before
           -> parse and extract

DELETED    absent now, present before
           -> remove the record
```

A rename is a delete plus an add; there is no content-based rename optimization.
The unit of work is the whole file. There is no fine-grained fact patching, and
Tree-sitter changed ranges are never the invalidation mechanism.

Reuse copies artifact bytes rather than re-serializing; serialization is
deterministic, so a copy is byte-identical to a fresh write.

Reading and hashing **every** supported file on every update is expected and
intentional. There is no mtime-only fast path. A file is never reused because
its path, size or mtime matches.

## 9. Previous-snapshot validation

Before a previous snapshot is used, the manifest is validated:

```text
manifest format version
required fields and digest shape
snapshot digest recomputed from canonical content
canonical path ordering and duplicate paths
analyzer fingerprint
configuration compatibility
```

Behaviour:

```text
structurally invalid / corrupt previous snapshot
    -> update REFUSES with an explicit diagnostic and a hint to run `index build`
       (never a silent downgrade to a fresh build)

analyzer fingerprint or configuration mismatch
    -> update REFUSES by default, with the same hint
    -> `--allow-incompatible` opts in to rebuilding with reuse disabled
       (every file reparsed; result still equals a fresh build)
```

A missing file artifact referenced by the manifest is a hard failure, not a
silent re-parse.

## 10. Publication safety

```text
build into a sibling staging directory   <output>.building
verify the built snapshot
if <output> exists: rename it aside to   <output>.previous
rename staging into place                <output>
remove the backup
```

At every instant the previous valid snapshot exists either at `<output>` or at
`<output>.previous`, so an interrupted build never leaves a half-written
directory where a valid snapshot used to be. If the final rename fails, the
backup is restored before the error is reported. A failed build removes its
staging directory.

## 11. Status propagation

The manifest preserves, per file, the analyzer's own status:

```text
clean        parsed with no ERROR and no MISSING nodes
recovered    parsed, but the tree contains recovery artifacts
incomplete   parsed, but an analysis step was truncated (query match limit)
failed       read, grammar or query failure
unsupported  not analyzed: encoding or size limit
```

Coverage is reported as separate counters, never as one flag. Two derived
predicates exist:

```text
is_fully_analyzed()  scan complete AND no incomplete, failed or skipped file
                     (recovered still counts: the whole file was analyzed)
is_fully_clean()     additionally requires recovered == 0
```

A recovered analysis is never turned into a clean one merely because its facts
serialized successfully.

## 12. Exact repository lookup

`RepositoryFactIndex` is a small in-memory index over the loaded snapshot. It is
**not** semantic lookup and does not resolve anything:

```text
file by relative path
all files in deterministic path order
declarations by exact written name
import items by exact written target
call-like occurrences by exact written callee form
test-bearing declarations
```

Every query returns source-grounded occurrences located by a compound locator
(`relative_path` plus the file-local fact id, plus the exact source range).
`declarations_named("User")` may return several unrelated declarations in
different files; they are returned separately and never collapsed. Unknown names
return nothing; there is no fuzzy or natural-language matching.

File-local identities remain file-local. No global persistent semantic symbol id
is invented, and stability across source edits is not promised.

```bash
reposuite-repodex index find <snapshot-dir> \
  [--declaration <name>] [--import <target>] [--call <callee>] [--test] [--json]
```

## 13. Verification

```bash
reposuite-repodex index verify <snapshot-dir> [--json]
```

Verifies the manifest, the format version, the recomputed snapshot digest,
canonical ordering, duplicate paths, and every referenced artifact's presence,
digest, path and status.

**A successful verification means the artifact is structurally internally
consistent. It does not prove that the original source files still match the
snapshot.** The snapshot deliberately does not store the source, so freshness
against the working tree is a separate question that verification cannot answer.

## 14. Statistics

Machine-readable repository statistics, available without loading any
Tree-sitter tree:

```text
files total, files by language
clean, recovered, incomplete, failed, skipped
declarations, imports, references, call-like occurrences, test-bearing declarations
source bytes represented
snapshot artifact bytes
snapshot digest
analyzer fingerprint
```

Update statistics additionally expose reused files, reparsed files, re-extracted
files, added, changed, unchanged, deleted, bytes hashed, bytes reparsed and
update duration.

```bash
reposuite-repodex index stats <snapshot-dir> [--json]
```

Operational timing and update counters are **outside** canonical equality and are
never part of the snapshot digest.

## 15. Known limitations

### 15.1 A missing normalized occurrence does not always mean the construct does not exist

This is the single most important limitation for consumers. The snapshot records
what the adapter observed; it never proves a syntactic construct is absent from
the source.

### 15.2 Rust calls inside macro token trees

TASK 2's F002 boundary still holds. Tree-sitter's Rust grammar represents macro
arguments as a flat `token_tree` with no expression subtrees, so a call written
inside a macro argument can be absent from the normalized call-like facts. TASK 2
measured this as 13 missed source-written calls in the audited regions. TASK 3A
preserves the boundary: it does not expand macros, does not add heuristic regex
facts to improve recall, and does not invent facts to hide the gap.

### 15.3 Pinned grammar may recover on legal or edge syntax

The pinned grammar can produce `ERROR`/`MISSING` nodes for constructs that are
legal in the language. Those files are reported as `recovered`, which is distinct
from `clean`.

### 15.4 Recovered analysis is distinct from clean analysis

Recovered files produce facts, and those facts are persisted, but they are not
the same claim as a clean parse. Coverage counters keep them separate and
`is_fully_clean()` exists so callers can require the stronger condition.

### 15.5 Artifact verification is structural, not freshness

See §13.

### 15.6 The artifact is experimental

No long-term format compatibility is promised. The manifest carries a format
version and incompatible versions are rejected rather than guessed at.

## 16. Performance observations

Not a budget. Measured on this machine with a warm page cache, three repetitions
per scenario, one repository per language, using
`scripts/task3a_perf.py`. Cold-cache behaviour was **not** controlled and is
**not** claimed.

Evidence: `~/reposuite/repodex/benchmarks/task3a-perf-20260918T064309Z/`
(`perf-raw.jsonl`, `perf-summary.txt`, `equivalence.jsonl`).

Medians (ms):

```text
corpus                 files   source bytes   fresh   verify   no-change   one-change   fresh rebuild
rust   tokio-rs/tokio    799      5,793,899    1680      290         130          130           1650
go     gohugoio/hugo     912      6,236,812    1970      330         180          190           1990
python django/django    2932     19,402,248    6150     1240         610          610           6180
php    laravel/framework 3086    17,817,062    6390     1220         490          520           6470
```

Artifact size relative to processed source bytes:

```text
tokio    32,723,988 / 5,793,899  = 5.65x
hugo     36,973,803 / 6,236,812  = 5.93x
django  142,490,126 / 19,402,248 = 7.34x
laravel 138,083,200 / 17,817,062 = 7.75x
```

Peak RSS (MiB, median): fresh build / load-verify

```text
tokio    12.0 / 5.6
hugo     11.5 / 5.5
django   23.2 / 13.9
laravel  39.8 / 17.7
```

### 16.1 How much work remains in a no-change update?

All of it is file I/O plus hashing. Every supported file is opened, read up to
the size limit and SHA-256 digested; nothing is parsed and nothing is
re-extracted (`reparsed_files = 0`, `bytes_reparsed = 0`). The remaining cost is
proportional to total source bytes, not to the number of changed files: 130 ms
for 5.8 MB (tokio) up to 610 ms for 19.4 MB (django), roughly 32–45 MB/s
including process start-up, directory walking and manifest writing. It is
deliberately not zero and not claimed to be zero.

### 16.2 How much does one-file rebuild avoid compared with fresh full extraction?

For a single changed file the update reparses only that file:

```text
tokio    7,688 of 5,793,921 bytes reparsed   (0.13%)
hugo     1,122 of 6,236,834 bytes reparsed   (0.018%)
django     846 of 19,402,269 bytes reparsed  (0.004%)
laravel  6,665 of 17,817,084 bytes reparsed  (0.037%)
```

Wall time drops from 1.65–6.47 s (fresh) to 0.13–0.61 s (one-file update), a
10–13x reduction, and the one-file update costs essentially the same as the
no-change update because hashing dominates. In every measured repetition the
incremental snapshot digest equalled the digest of an independent fresh full
build of the same final bytes (`equivalence.jsonl`).

### 16.3 How large is the persisted normalized-fact snapshot relative to source?

Roughly 5.6–7.8x the processed source bytes. The manifest is a small fraction of
that; the per-file JSON artifacts dominate. The ratio is higher for Python and
PHP because those languages produce more facts per source byte (more
declarations and more call-like occurrences) and because compact JSON repeats
field names and row/column coordinates for every range. This is the expected
cost of an inspectable JSON artifact and is not optimized in TASK 3A.

### 16.4 Does loading/verifying the snapshot introduce a new major bottleneck?

Loading and verifying costs 290–1240 ms, about 17–20% of a fresh build. It reads
and re-digests every artifact, so it is proportional to artifact size rather than
source size — which is why django and laravel (≈140 MB of artifacts) are the
slowest. It is not free, but it is well below a full re-analysis and it does not
grow super-linearly. Loading the snapshot into `RepositoryFactIndex` (which
deserializes every analysis) is the more expensive operation when exact lookup is
needed; `index verify` avoids it by checking digests without building the index.

### 16.5 Does snapshot structure remain practical on the measured corpora?

Yes, on these corpora. The largest measured snapshot is 142 MB for 2,932 files,
built in about 6 s, verified in about 1.2 s, and updated with no changes in about
0.6 s. Peak RSS stays at 40 MiB or below for both building and loading, so the
snapshot path does not require holding the repository's trees or sources in
memory. No claim is made about much larger repositories; the ratio and the
per-file structure suggest cost grows linearly with source and artifact bytes.

## 17. Memory observations

Measured with `/usr/bin/time -v`, peak RSS of the whole process:

```text
                        build snapshot   load/verify snapshot
tokio (5.8 MB source)        12.0 MiB              5.6 MiB
hugo  (6.2 MB source)        11.5 MiB              5.5 MiB
django (19.4 MB source)      23.2 MiB             13.9 MiB
laravel (17.8 MB source)     39.8 MiB             17.7 MiB
```

Interpretation: the snapshot path is bounded by the current file plus the
in-memory manifest, not by the repository. **This is not directly comparable to
TASK 2's retained-tree experiments.** TASK 2 compared holding every Tree-sitter
tree in memory against releasing each tree after extraction, and measured
ratios of roughly 15.8x (tokio), 17.2x (hugo), 24.9x (django) and 18.0x
(laravel). TASK 3A does not retain trees at all — a tree is created, used for
extraction and dropped within one file — so the relevant statement is simply
that the measured snapshot build peaks in the tens of megabytes, which is
consistent with not retaining trees and not retaining sources. The measurements
are not a like-for-like comparison and are not presented as one.

## 18. Dependency discipline

TASK 3A adds exactly one dependency:

```text
sha2 0.10   collision-resistant content and analysis digests
```

Justification: content identity must be collision-resistant and must not reuse
the FNV-1a change detector. `sha2` was already present in the local cargo cache,
so the addition keeps the build reproducible offline, and SHA-256 is a
well-understood standard choice. Hashing is not a bottleneck at these sizes.

No database, no async runtime, no daemon, no watcher, no network access, no
storage service, no migrations, no snapshot history manager and no garbage
collector. The snapshot is a directory of JSON documents and a manifest.

## 19. What TASK 3A deliberately does not do

```text
no cross-file symbol resolution or resolved references
no call graph, knowledge graph or inferred FQN
no type inference or module resolution beyond written syntax
no behaviour routes, locate(), fuzzy search, semantic search or embeddings
no LLM, LSP, gopls, rust-analyzer, Pyright, PHPStan or Psalm integration
no framework intelligence or Laravel semantics
no watcher, daemon, MCP, HTTP/RPC or GUI
no parallel scanner
no database (SQLite, RocksDB, redb, LMDB) and no migrations
no fine-grained fact patching
no Tree-sitter changed ranges as the invalidation mechanism
no Rust macro expansion and no heuristic regex facts added for recall
no persistent-storage architecture, cache manager, snapshot history manager,
  garbage collector or remote synchronization
```
