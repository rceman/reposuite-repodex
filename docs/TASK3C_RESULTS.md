# TASK 3C results — Rust local plain-name call candidates

**A candidate is not a resolved call target.** Everything in this document is a
bounded syntactic observation, never a claim about runtime dispatch.

> **Post-completion correction note.** A follow-up correction
> (`REPODEX-T3C-LEXICAL-SHADOW-CORRECTION-V1`) required suppressing an outer
> `fn` candidate when a closer local `let`/parameter/pattern binding owns the
> written name. The persisted normalized facts do not represent those bindings,
> so the correction is `TASK3C_CORRECTION_BLOCKED_BY_FACT_MODEL` — see
> [`TASK3C_CORRECTION.md`](TASK3C_CORRECTION.md) and the escalated T3C-F005.

## 1. Base commit

```text
934f2d5b303919be1037d00e250ba5f1991750c5   (TASK 3B tip, unamended)
```

## 2. TASK 3C commit(s)

```text
6031fcb68316f521e3c0537752c30fc686384945  Add bounded Rust local call-candidate linking over the snapshot and links
```

The working tree at commit time contains `src/candidates/`, the `candidates`
CLI, `tests/call_candidates.rs`, `fixtures/callcandidates/`, the audit and perf
scripts, and this documentation.

## 3. Candidate model

```text
CallCandidateRecord {
    record_id, rule_id, language,
    source:        FactLocator          (path + call fact id)
    written:       String               (the written callee)
    outcome:       CandidateOutcome
    provenance:    CandidateProvenance
}

CandidateOutcome =
    NoCandidate        { reason }
    SingleCandidate    { candidate }
    MultipleCandidates { candidates[] }
    OutOfScope         { reason }

CandidateTarget = { relative_path, declaration_id, declaration_kind, name, scope_path }
CandidateProvenance = { scope_path, search_levels, enclosing_module, evidence[] }
```

`CandidateIndex` is the query/view: `from_source`, `by_rule`, `single`,
`multiple`, `none`, `out_of_scope`, `targeting_file`, `targeting_declaration`,
`cardinality_counts`.

## 4. Why it is separate from `LinkOutcome`

TASK 3B's `Exact`/`Ambiguous`/`Unresolved`/`OutOfScope` fits bounded structural
relationships, where "exactly one structural candidate" *is* the answer the
rule exists to give. For a call, "exactly one lexical candidate" is **not** an
answer — it is evidence. Reusing `Exact` would claim resolution; reusing
one-element `Ambiguous` would claim undetermined-ness the model does not have.
`CandidateOutcome` has no `Exact` variant and no ambiguity concept, so neither
meaning can be smuggled back in.

## 5. Artifact layout

```text
<candidates-dir>/
  manifest.json            versions, dependencies, counts, canonical digest, rule registry
  call_candidates.jsonl    one canonical candidate record per Rust call
```

Built by staging the artifact, verifying it in staging, then atomically
publishing — the same pattern as the link artifact.

## 6. Candidate-rule ABI/version

```text
candidate_manifest_version  1
candidate_schema_version    1
candidate_rule_abi_version  1
rule                        rust.call.local_function_candidate   (version 1)
candidate_fingerprint       sha256:43056aee99be33c992e0261aef87e93e1d6edd60c3bd937f1114a6e11a174254
```

Changing candidate semantics invalidates the candidate artifact without
invalidating the snapshot or link artifacts.

## 7. Dependency model

The manifest pins the exact upstream identity it was derived from:

```text
snapshot_digest / snapshot_schema_version / snapshot_analyzer_fingerprint
link_digest     / link_fingerprint        / link_rule_abi_version
```

Rebuilding either upstream artifact changes its digest, and `candidates verify`
rejects the stale candidate artifact with `SnapshotMismatch` or `LinkMismatch`
accordingly. Even though this first rule does not *use* the links, depending on
the link digest keeps the derived pipeline's ordering explicit.

## 8. Exact in-scope call definition

```text
language == Rust
form == plain_name
dynamic_callee == false
callee_written is exactly one written identifier (raw `r#name` allowed)
```

## 9. Exact candidate declaration definition

Source-written Rust `function` declarations — `DeclarationKind::Function`,
covering free and body-nested `fn` items — at a scope on the call's bounded
lexical chain. Methods, associated/trait functions, closures, locals, statics,
consts, tuple-struct/enum constructors, macros, imports and re-exports are not
candidates.

## 10. Lexical/module selection algorithm

1. From the call's `scope_id`, walk `parent_scope_id` until the first `file` or
   `module` scope, inclusive — the bounded lexical chain (`impl` is transparent;
   the chain cannot cross a module boundary).
2. Innermost first, collect `function` declarations at that `scope_id` whose
   `name` equals the written callee.
3. The first non-empty level supplies the whole candidate set.
4. Empty chain ⇒ `no_candidate`. One ⇒ `single_candidate`. Many ⇒
   `multiple_candidates`. Non-plain-name call ⇒ `out_of_scope`.

## 11. Curated fixture results

`fixtures/callcandidates/rust` — 26 records: 8 `single_candidate`, 1
`multiple_candidates`, 10 `no_candidate`, 7 `out_of_scope`.

| call (file :: scope) | written | outcome | candidates |
| --- | --- | --- | --- |
| lib.rs :: run | `helper` | single | `lib.rs#4` (module `helper`) |
| lib.rs :: missing | `absent` | none | — |
| lib.rs :: run_imported | `imported_fn` | none | — (imported) |
| lib.rs :: run_reexport | `reexported_fn` | none | — (re-exported) |
| lib.rs :: outer | `helper` | single | `lib.rs#10` (nested, shadows module) |
| a.rs :: run | `helper` | single | `a.rs#0` |
| inline.rs :: left::run | `helper` | single | `inline.rs#1` (`left::helper`) |
| inline.rs :: lonely_mod::run | `helper` | none | — (module boundary) |
| inline.rs :: file_run | `file_helper` | single | `inline.rs#7` |
| lonely.rs :: run | `helper` | none | — (crate-root helper is a different module) |
| scope.rs :: (impl S)::caller | `helper` | single | `scope.rs#0` (impl transparent) |
| scope.rs :: run | `helper` | single | `scope.rs#0` |
| scope.rs :: run | `closure` | none | — (local `let`) |
| scope.rs :: run | `f` | none | — (local fn-pointer `let`) |
| scope.rs :: run | `User` | none | — (tuple-struct, not a `fn`) |
| dup.rs :: run | `dup` | multiple | `dup.rs#0`, `dup.rs#1` |
| broken.rs :: run | `helper` | single | `broken.rs#0` (recovered source) |
| ext.rs :: run | `ext_helper` | none | — (extern-block fn, separate scope) |
| edge.rs :: run | `crate::helper` | out_of_scope | — |
| edge.rs :: run | `self::helper` | out_of_scope | — |
| edge.rs :: run | `obj.method` | out_of_scope | — |
| edge.rs :: run | `S::assoc` | out_of_scope | — |
| edge.rs :: run | `(helper)` | out_of_scope | — |
| edge.rs :: run | `produce()` | out_of_scope | — |
| edge.rs :: run | `produce` | none | — |
| edge.rs :: run | `my_macro` | out_of_scope | — |

Every record's **complete** candidate set is asserted, not just one expected
candidate.

## 12. Negative-test results

All pass (`tests/call_candidates.rs`, 35 tests). Highlights: imports and
re-exports never produce a candidate; qualified/member/associated/indirect and
macro calls are `out_of_scope`; closures, `let` fn-pointers and tuple-struct
constructions never fabricate a `fn` candidate; a `#[cfg]` duplicate is
`multiple_candidates`; a stale snapshot or stale link digest is rejected; a
tampered record, an unsorted record list, a non-`function` candidate, a
cardinality that lies, and a missing call locator all fail verification.

## 13. Real-source audit regions

Frozen regions were selected **independently of the candidate output**, from
normalized structure only: the in-scope-richest nested-function, inline-module
and impl-method files plus a deterministic stride over all in-scope files —
**44 files, 1,128 in-scope calls, 5,801 out-of-scope calls** audited. The
widened pass audited **all 799 files / 5,345 in-scope calls**.

## 14. Source-driven audit methodology

`scripts/task3c_audit.py` is **occurrence-driven**, not record-driven: it reads
every normalized `plain_name` call in the frozen regions and independently
re-derives the expected candidate set with a separate Python implementation of
the lexical/module rule, sharing no code with the Rust rule. It then
cross-checks every emitted candidate against the raw source bytes (the
declaration's `name_range` must read the callee name) and against scope
containment (a candidate's scope must be on the call's bounded chain).

## 15. TP / FP / FN candidate metrics

Full corpus (5,345 in-scope calls):

```text
candidate TP   1,396   (1,394 single + 2 multiple)
candidate FP   0
candidate FN   0
```

## 16. Every FALSE_CANDIDATE

```text
none
```

The audit's only flag was a checker bug (byte-offset slicing), fixed before the
final run — not a RepoDex defect. Zero `FALSE_CANDIDATE`, zero
`WRONG_SCOPE_CANDIDATE`, zero `MISSING_CANDIDATE`, zero `OUT_OF_SCOPE_WRONG`.

## 17. Macro-unavailable occurrence count

```text
~13,683 call-shaped tokens inside macro token trees  (approximate upper bound,
NOT_AVAILABLE_TO_TASK3C — not candidate misses)
```

## 18. Determinism results

Repeated builds produce byte-identical `manifest.json` and
`call_candidates.jsonl` and an identical `candidate_digest`. On tokio, three
independent builds (repeated, different output dir, relocated root) all produce
`sha256:6fe38a08c3f53ee9573cdd95aa3dba8354bb40eb315e41dcd32c39ebdfe23452`.

## 19. Cross-root results

The fixture copied to two absolute roots, and the whole tokio pipeline rebuilt
from a relocated checkout, produce identical candidate digests and bytes. No
checkout root, timestamp, PID or traversal order reaches canonical output.

## 20. Fresh-vs-update equivalence

Fixture: 8 scenarios (no change; local fn added/removed/renamed; call
added/removed/renamed; module moved/changed) — every one produces an identical
`candidate_digest` and identical record bytes whether the snapshot was rebuilt
incrementally or fresh. Real corpus: an `index update` + link + candidate
rebuild on mutated tokio reproduced the fresh-build digest
`sha256:5fd2f913…` and identical bytes.

## 21. Artifact sizes

| artifact | fixture bytes | tokio bytes |
| --- | --- | --- |
| TASK 3A snapshot | 55,614 | 32,723,989 |
| TASK 3B links | 19,956 | 4,549,242 |
| TASK 3C candidates | 14,538 | 17,716,367 |
| candidate / snapshot | 0.26 | 0.54 |
| candidate / links | 0.73 | 3.89 |

The candidate artifact is larger than the link artifact because it records a
complete disposition for every call (43,016 records, ~88% out-of-scope) — see
T3C-F002.

## 22. Performance

| | value |
| --- | --- |
| snapshot build | 1.95 s |
| link build | 0.81 s |
| **candidate build** | **1.23 s** |
| └ snapshot_load | ~317 ms |
| └ **rule derive** | **~118 ms** |
| └ serialize | ~42 ms |
| └ verify | ~586 ms |
| candidate verify (standalone) | 0.64 s |

The lexical rule itself is ~118 ms; loading and re-validating the snapshot
dominates. A full rebuild is cheap, so incremental candidate mutation is
unnecessary (T3C-F009).

## 23. Peak memory

Peak RSS for `candidates build` on tokio: **149,036 KB** (~145 MB), via
`/usr/bin/time -f %M`. Whole-process maximum, not a breakdown.

## 24. All findings

Thirteen findings in [`TASK3C_FINDINGS.md`](TASK3C_FINDINGS.md). No `BLOCKER`,
no `HIGH`. Notable: T3C-F003 / T3C-F013 (imports are the dominant bounded miss,
the recommended next step), T3C-F004 (macro-token-tree boundary), T3C-F005
(local-binding shadowing is unmodeled — a candidate is not a resolved target),
T3C-F001 (`#[cfg]` duplicates stay `multiple_candidates`), T3C-F007 (the only
`FALSE_CANDIDATE` was a checker bug).

## 25. Cargo verification

```text
cargo fmt --all -- --check                                        PASS
cargo check --locked                                              PASS
cargo test --locked                                               PASS   283 passed, 0 failed
cargo clippy --locked --all-targets --all-features -- -D warnings PASS
cargo build --locked --release                                    PASS
```

Also run: the 35-test `call_candidates` suite, the cross-file link suite, the
frozen-region and full-corpus audits, the performance harness, and the
update-vs-fresh equivalence check.

## 26. Anything NOT measured

Memory under a different allocator or page size; candidate build cost far above
~800 files / ~43,000 calls; the import-miss count is a name-match upper bound
(it may include coincidental same-name non-imports); the macro-hidden count is
an `ident(`-token upper bound, not an exact count; Windows behaviour of the
candidate layer (untested, still type-checks); semantic call resolution
(explicitly out of scope).

## 27. Recommended smallest next step

**Rust import-aware plain-name candidates** (T3C-F013). The measured dominant
gap is that ~1,040 tokio calls name a `use`-imported leaf but get
`no_candidate` only because the rule is import-blind. A bounded rule that lets
an imported name point at the import's already-linked declaration — still
without transitive re-export following or semantic claims — is the smallest
valuable expansion. **Not implemented.**

## 28. Confirmation: next task not started

Confirmed. No import-aware, re-export-aware, qualified-path, method,
associated-function or trait candidates, and no Go/Python/PHP candidates, were
implemented. `no_candidate` for imported calls is asserted in the tests.

## 29. Confirmation: nothing pushed

Confirmed. TASK 3C commits exist locally on top of `934f2d5`; `git push` was
never run and no remote ref was updated.
