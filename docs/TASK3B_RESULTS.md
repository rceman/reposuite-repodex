# TASK 3B results

Cross-file structure and import linking foundation.

```text
base commit   fd62f5d6313d5d043b10fee5a7ae4c5ad7bcddf4   (TASK 3A)
prompt id     REPODEX-T3B-CROSSFILE-STRUCTURE-LINKING-V1
```

This document reports only what was measured or verified. Section 11 lists what
was **not** measured.

## 1. What TASK 3B added

```text
RepositoryFactSnapshot                       (TASK 3A, unchanged)
    -> derived repository structural model
    -> language-specific module/package/namespace relationships
    -> import/module link outcomes
    -> <links-dir>/manifest.json + links.jsonl + entities.jsonl
```

The linking layer consumes the persisted normalized facts plus repository-root
`go.mod` only. It never reparses source through Tree-sitter and never mutates the
TASK 3A snapshot.

## 2. Artifact format

```text
<links-dir>/
  manifest.json      schema/rule ABI, dependencies, counts, canonical digest
  links.jsonl        one canonical relationship per line
  entities.jsonl     one canonical structural entity per line
```

| Field | Value |
| --- | --- |
| `link_manifest_version` | 1 |
| `link_schema_version` | 1 |
| `link_rule_abi_version` | 1 |
| language resolution-policy versions | Rust 1, Go 1, Python 1, PHP 1 |
| link fingerprint | `sha256:7101754c650264c3cc3c863b83be75834315c94901095addc68b588a32b61ae1` |
| digest algorithm | SHA-256, rendered `sha256:<lowercase-hex>` |

The fingerprint covers the manifest version, the schema version, the rule ABI
version and the four language policy versions. Changing link semantics therefore
invalidates the derived artifact without invalidating the TASK 3A snapshot.

## 3. Relationship locator and provenance model

```text
source locator        relative_path + fact kind + file-local fact id (+ item index)
declaration target    relative_path + declaration id + syntactic qualified name
structural target     derived entity id + structural kind + structural key
link id               lnk-<16 hex>, a pure function of (rule, source locator, written)
```

Every record carries `rule_id`, the source locator, the written source form, an
`evidence` list, and any `metadata` dependencies. No permanent semantic symbol
identity is introduced; file-local fact ids are explicitly not promised stable
across source edits.

## 4. Implemented rule IDs

```text
rust.mod.standard_file
rust.use.crate_path
rust.use.non_crate_path

go.package.same_directory
go.import.local_module
go.import.external

python.relative_import.package_path
python.absolute_import.local_candidate
python.absolute_import.external

php.namespace.declaration
php.use.qualified_name
php.use.external
php.use.non_class_import
```

All fourteen are documented in the artifact itself: `manifest.json` carries the
complete rule registry (input syntax, repository assumptions, metadata
dependency, the four outcome conditions, known exclusions, candidate derivation).
A test asserts that every emitted rule id appears in that registry with all
conditions non-empty.

## 5. Outcome semantics

```text
Exact        exactly one candidate under this rule's documented assumptions
Ambiguous    more than one candidate, or a candidate that cannot be proven unique
Unresolved   the rule applied and produced no candidate
OutOfScope   deliberately outside this task
```

The four are never collapsed. In particular:
`python.absolute_import.local_candidate` is never `Exact`, including with a single
candidate; a missing or malformed `go.mod` never produces a local Go import; and
non-`crate` Rust `use` paths are recorded as out of scope rather than dropped.

## 6. Curated fixture results

`fixtures/crossfile/<language>/` are multi-file repositories built into snapshots
and linked by `tests/crossfile_links.rs`. The suite asserts the **complete**
relationship set per rule, not merely the presence of expected links.

### Rust — `fixtures/crossfile/rust`

| Rule | Count | Exact | Ambiguous | Unresolved | OutOfScope |
| --- | --- | --- | --- | --- | --- |
| `rust.mod.standard_file` | 7 | 5 | 1 | 1 | 0 |
| `rust.use.crate_path` | 9 | 6 | 1 | 2 | 0 |
| `rust.use.non_crate_path` | 2 | 0 | 0 | 0 | 2 |
| **total** | **18** | **11** | **2** | **3** | **2** |

Notable cases: `mod api;` is `Ambiguous` because both `src/api.rs` and
`src/api/mod.rs` exist; `mod absent;` is `Unresolved`; `mod outer { mod inner; }`
resolves through an inline module to `src/outer/inner.rs`;
`crate::dupmod::dup` is `Ambiguous` with two declarations; `crate::util::helper`
is `Exact` even though an unrelated `crate::deep` also declares `helper`.

### Go — `fixtures/crossfile/go`

| Rule | Count | Exact | Ambiguous | Unresolved | OutOfScope |
| --- | --- | --- | --- | --- | --- |
| `go.package.same_directory` | 7 | 7 | 0 | 0 | 0 |
| `go.import.local_module` | 6 | 2 | 2 | 2 | 0 |
| `go.import.external` | 2 | 0 | 0 | 0 | 2 |
| **total** | **15** | **9** | **2** | **2** | **2** |

Notable cases: `package bar` and `package bar_test` in one directory stay
distinct groups; `example.com/fixturex` is **not** local under module path
`example.com/fixture` (segment-wise prefix test); `example.com/fixture` maps to
the repository root; `example.com/fixture/internal` maps to a directory with no
Go file and is `Unresolved`. One metadata dependency (`go.mod`) is recorded.

### Python — `fixtures/crossfile/python`

| Rule | Count | Exact | Ambiguous | Unresolved | OutOfScope |
| --- | --- | --- | --- | --- | --- |
| `python.relative_import.package_path` | 7 | 4 | 0 | 3 | 0 |
| `python.absolute_import.local_candidate` | 2 | 0 | 2 | 0 | 0 |
| `python.absolute_import.external` | 2 | 0 | 0 | 0 | 2 |
| **total** | **11** | **4** | **2** | **3** | **2** |

Notable cases: `from . import sibling` resolves to a declaration in
`pkg/sub/__init__.py`; `from ...top import thing` escapes the repository root and
is `Unresolved`; a root-level file's `from . import x` is `Unresolved`; absolute
imports with a single repository candidate are `Ambiguous`, never `Exact`.

### PHP — `fixtures/crossfile/php`

| Rule | Count | Exact | Ambiguous | Unresolved | OutOfScope |
| --- | --- | --- | --- | --- | --- |
| `php.namespace.declaration` | 11 | 11 | 0 | 0 | 0 |
| `php.use.qualified_name` | 5 | 3 | 1 | 1 | 0 |
| `php.use.external` | 1 | 0 | 0 | 0 | 1 |
| `php.use.non_class_import` | 2 | 0 | 0 | 0 | 2 |
| **total** | **19** | **14** | **1** | **1** | **3** |

Notable cases: `App\Dup\Thing` is `Ambiguous` with two declarations;
`App\Missing\Thing` is `Unresolved` because the `App` namespace prefix is
declared; `Symfony\...` is `OutOfScope`; the alias in
`use App\Other\Foo as OtherFoo;` does not change the written target; the grouped
`use App\Service\{Foo as GroupedFoo};` composes its shared prefix; a declaration
in the global namespace receives no invented namespace entity.

Fixture artifact sizes (tiny repositories, so the manifest dominates):

| Fixture | Snapshot bytes | Link bytes | Ratio |
| --- | --- | --- | --- |
| rust | 28,395 | 28,140 | 0.991 |
| go | 20,745 | 28,582 | 1.378 |
| python | 17,577 | 22,375 | 1.273 |
| php | 24,238 | 29,231 | 1.206 |

Fixture link digests (all four share the fingerprint above):

```text
rust    sha256:db3d308e1f8dda0aeb2524bcdc866d8ed9e14626aedff606343f0a0f6cb4d46a
go      sha256:578ddf4cb8fe9ba4366b274a9ea6398836e9fc878eb2666a54f352c1dab6b730
python  sha256:076d588402bbac77e4c6bd06313a8e6ef01732938925a8cdea77805bcb900f1b
php     sha256:73995abe8ca530deb17a803b53dee5c6408f7cfc33b42919b5faa974359c2b77
```

These tiny ratios are expected and are **not** the artifact-size result: on these
fixtures the per-artifact fixed cost (manifest plus one entity line per file)
dominates. See §9 for the real-repository ratio.

## 7. Negative ambiguity results

The suite proves RepoDex does **not** manufacture exactness when the evidence is
insufficient. `tests/crossfile_links.rs` contains:

```text
no_rule_emits_exact_without_a_single_structural_candidate
    every Exact relationship in all four fixtures names exactly one candidate and
    carries non-empty provenance
ambiguous_relationships_keep_every_candidate
    every Ambiguous relationship in all four fixtures keeps its candidates
same_written_name_in_an_unrelated_module_is_not_collapsed
rust_module_search_does_not_silently_pick_the_first_candidate
go_import_prefix_is_matched_on_path_segments_not_text
python_absolute_import_with_two_local_roots_stays_ambiguous
python_dynamic_imports_produce_no_relationship
import_relationship_never_becomes_a_call_edge
missing_metadata_never_produces_a_local_go_import
malformed_go_mod_never_produces_a_local_go_import
adding_a_go_mod_invalidates_a_previous_link_artifact
rust_use_of_a_module_outside_the_crate_stays_out_of_scope
```

All pass. No `FALSE_EXACT` was observed in any fixture.

## 8. Determinism results

```text
repeated_link_builds_produce_identical_bytes
    two builds of the same snapshot into different directories produce identical
    manifest.json, links.jsonl and entities.jsonl bytes
equivalent_checkouts_under_different_roots_produce_equivalent_artifacts
    the go fixture copied to two different absolute roots produces identical
    snapshot digest, identical link digest and identical artifact bytes
the_output_directory_does_not_affect_canonical_identity
    near and deeply nested output directories produce the same link digest
```

On the four real repositories, repeated builds (3×) plus a build into a relocated
output directory produced **one** distinct link digest each, and a no-change
TASK 3A update followed by a link rebuild reproduced the fresh-build digest
exactly. See §9.

## 9. Real-repository measurements

One representative repository per language, from the TASK 2/TASK 3A corpus set.
`scripts/task3b_perf.py` (release build).

| | rust | go | python | php |
| --- | --- | --- | --- | --- |
| corpus | tokio-rs_tokio | gohugoio_hugo | django_django | laravel_framework |
| snapshot build (s) | 1.86 | 2.15 | 6.59 | 5.30 |
| link build (s) | 0.76 | 0.87 | 2.90 | 2.96 |
| link / snapshot build | 0.409 | 0.405 | 0.440 | 0.558 |
| snapshot bytes | 32,723,988 | 36,973,803 | 142,490,126 | 138,083,200 |
| link bytes | 4,549,242 | 6,206,857 | 13,850,332 | 18,227,818 |
| **size ratio** | **0.1390** | **0.1679** | **0.0972** | **0.1320** |
| links | 6,841 | 6,985 | 16,918 | 20,937 |
| exact | 116 | 1,981 | 1,138 | 16,236 |
| ambiguous | 0 | 1,138 | 8,034 | 3 |
| unresolved | 1,324 | 0 | 22 | 2 |
| out-of-scope | 5,401 | 3,866 | 7,724 | 4,696 |
| structural entities | 142 | 275 | 659 | 432 |
| metadata dependencies | 0 | 1 | 0 | 0 |
| peak RSS (link build, KB) | 73,988 | 88,928 | 270,944 | 283,972 |
| deterministic (distinct digests) | 1 | 1 | 1 | 1 |
| no-change update == fresh | yes | yes | yes | yes |

Link-build phase breakdown (ms), from the same runs:

| Phase | rust | go | python | php |
| --- | --- | --- | --- | --- |
| snapshot load | 297.1 | 336.8 | 1,231.9 | 1,217.9 |
| metadata | 0.0 | 0.1 | 0.0 | 0.0 |
| structure derivation | 1.3 | 1.6 | 4.4 | 14.1 |
| rule derivation | 25.5 | 30.6 | 76.4 | 93.4 |
| serialize | 11.1 | 15.4 | 34.7 | 42.6 |
| verify in staging | 398.6 | 438.1 | 1,440.1 | 1,456.9 |
| **total** | **754.4** | **858.2** | **2,851.5** | **2,912.5** |

Loading the snapshot and verifying the staged artifact dominate; the actual rule
derivation is 25 ms–93 ms.

**Answer to the question that decides incremental design.** Yes: a full
relationship rebuild is cheap enough that incremental link mutation is
unnecessary for now. It costs 0.405×–0.558× a snapshot build of the same
repository, and only 3.4%–4.0% of that time is rule derivation. TASK 3B therefore
rebuilds the whole derived artifact from the whole snapshot.

## 10. Real-repository audit

`scripts/task3b_audit.py` re-derives each sampled relationship **independently**,
from raw source text and the filesystem, using a separate implementation that
shares no code with the Rust rules. A checker that shares code with the thing it
checks proves very little.

Required sample: 20 `Exact` plus 10 `Ambiguous`/`Unresolved` per rule.

| corpus | CORRECT_EXACT | CORRECT_AMBIGUOUS | CORRECT_UNRESOLVED | OUT_OF_SCOPE_BY_POLICY | FALSE_EXACT |
| --- | --- | --- | --- | --- | --- |
| tokio-rs_tokio | 31 | 0 | 10 | 10 | **0** |
| gohugoio_hugo | 40 | 10 | 0 | 10 | **0** |
| django_django | 20 | 10 | 10 | 10 | **0** |
| laravel_framework | 40 | 3 | 2 | 20 | **0** |

Widened sample (up to 1,000 per outcome per rule, 12,296 relationships total):

| corpus | sampled | CORRECT_EXACT | CORRECT_AMBIGUOUS | CORRECT_UNRESOLVED | OUT_OF_SCOPE | FALSE_EXACT |
| --- | --- | --- | --- | --- | --- | --- |
| tokio-rs_tokio | 2,116 | 116 | 0 | 1,000 | 1,000 | **0** |
| gohugoio_hugo | 3,912 | 1,912 | 1,000 | 0 | 1,000 | **0** |
| django_django | 3,022 | 1,000 | 1,000 | 22 | 1,000 | **0** |
| laravel_framework | 3,246 | 2,000 | 3 | 2 | 1,241 | **0** |
| **total** | **12,296** | 5,028 | 2,003 | 1,024 | 4,241 | **0** |

No `MISSING_CANDIDATE` and no `WRONG_CANDIDATE` verdicts were recorded in the
final run either. The audit script exits non-zero if any `FALSE_EXACT` is found;
all four runs exited zero.

Two checker defects were found and fixed during the audit, and both were the
*checker's* fault, not RepoDex's — they are worth recording because they show the
checker was genuinely independent:

1. the checker assumed a crate root was the first path segment, so it mis-adjudicated
   `tokio/src/loom/std/parking_lot.rs` (the crate root is `tokio/src/`);
2. the checker assumed one file per module, so it mis-adjudicated
   `tokio/src/util/linked_list.rs` declaring an inline `mod tests { .. }`.

A third finding, T3B-F001 in `docs/TASK3B_FINDINGS.md`, *was* a real artifact
defect: `from . import X` and `from .X import ...` render identically in
`written`, so the artifact did not let an auditor tell them apart. The raw import
form is now carried in the provenance evidence.

## 11. Not measured

```text
memory measurement on a machine with a different allocator or page size
link-build cost for repositories larger than ~7,000 indexed files
link-build cost with more than one metadata format in play
incremental relationship patching (deliberately not implemented)
GPU, network or distributed behaviour (none exists)
Windows behaviour of the link layer (untested; the crate type-checks for
    x86_64-pc-windows-gnu as before)
a second independent implementation for cross-checking the Rust rules'
    module-tree traversal beyond what scripts/task3b_audit.py re-derives
semantic correctness at runtime (explicitly out of scope; verification proves
    internal consistency only)
```

Peak RSS above is `/usr/bin/time -f %M`, which reports the maximum resident set
of the process, not a breakdown.

## 12. Findings

Fourteen findings are recorded in `docs/TASK3B_FINDINGS.md`. None is a `BLOCKER`
or `HIGH`. One `MEDIUM` (T3B-F001) was fixed; one `LOW` (T3B-F002) was fixed; one
`LOW` (T3B-F010) and several `INFO` findings are documented and accepted.

```text
FALSE_EXACT defects in required rules: 0
```

## 13. Verification

```text
cargo fmt --all -- --check                                        PASS
cargo check --locked                                              PASS
cargo test --locked                                               PASS   236 passed, 0 failed
cargo clippy --locked --all-targets --all-features -- -D warnings PASS
cargo build --locked --release                                    PASS
```

Suites added by TASK 3B:

```text
tests/crossfile_links.rs     42 tests: fixture relationship sets, negative
                             ambiguity, determinism, cross-root equivalence,
                             TASK 3A update-vs-fresh link equivalence, artifact
                             integrity, query API
```

`tests/expected_facts.rs` now also covers the cross-file fixture sources, because
they live under `fixtures/`.

Harnesses added by TASK 3B:

```text
scripts/task3b_perf.py    performance and artifact-size measurement (§9)
scripts/task3b_audit.py   independent relationship audit (§10)
```

## 14. TASK 3C readiness

**Is the structural-link foundation ready for TASK 3C (reference and call
candidate linking)?** Yes.

The foundation provides what TASK 3C needs:

* a deterministic, provenance-bearing, content-digested cross-file relationship
  index that is independent of checkout root;
* a four-way outcome model that already keeps "candidate" distinct from
  "resolved", which is exactly the discipline TASK 3C must preserve;
* stable rule ids and an artifact schema whose dependency is the exact snapshot
  digest, so a call-candidate layer can be added as a separate derived artifact
  over the same snapshot;
* a working pattern for content-digested metadata dependencies;
* a measured cost model showing a full rebuild is cheap.

**Smallest recommended TASK 3C scope.** One derived artifact, one rule family,
no new metadata, no incremental machinery:

```text
one language first (Rust), not four
one rule: rust.call.local_candidate
scope: a call-shaped occurrence whose written callee is a single identifier
    (no member selector, no path) inside a module scope, matched against
    declarations reachable through the crate module tree already derived by
    TASK 3B
outcomes: zero candidates -> Unresolved, one -> candidate, many -> Ambiguous;
    NEVER Exact, because a lexical match is not dispatch
artifact: <calls-dir>/manifest.json + calls.jsonl, depending on the exact
    snapshot digest and, if needed, the link digest
reuse: the existing LinkOutcome/LinkTarget/provenance types and the
    sort_candidates ordering, so there is one implementation of "never collapse
    ambiguity"
```

Deliberately deferred: member-selector calls (`obj.F()`), trait/interface
dispatch, Go interface dispatch, Python attribute calls, PHP dynamic calls,
and anything that would require name resolution beyond the module tree.

TASK 3C must still avoid presenting candidate identity as runtime dispatch.

## 15. Commit state

```text
base commit            fd62f5d6313d5d043b10fee5a7ae4c5ad7bcddf4   (TASK 3A, unamended)
TASK 3B implementation 16d855c8ff5b0abab8b287336a86dfb5fa44211b
```

`16d855c` has `fd62f5d` as its sole parent. The TASK 3A commit was not amended.
Nothing was pushed. TASK 3C was not started.
