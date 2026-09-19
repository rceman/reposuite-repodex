# TASK 3C findings log

Each finding is labelled by category and severity. Categories: `CANDIDATE_MODEL`,
`LEXICAL_SCOPE`, `MODULE_SCOPE`, `FALSE_CANDIDATE`, `MISSING_CANDIDATE`,
`MACRO_BOUNDARY`, `DETERMINISM`, `ARTIFACT`, `PERFORMANCE`. Severity:
`BLOCKER`, `HIGH`, `MEDIUM`, `LOW`, `INFO`.

The central reminder for every finding: **a candidate is not a resolved call
target.** Nothing below is a semantic-resolution claim.

## T3C-F001 — `MODULE_SCOPE`, INFO — `#[cfg]`-gated duplicates stay `multiple_candidates`

Tokio's `tokio/src/util/memchr.rs` (and its `tokio-util` copy) defines
`fn memchr_inner` twice under complementary `#[cfg]` conditions. Under the
bounded syntactic rule both are eligible `function` declarations at the same
scope, so the outcome is `multiple_candidates` — the rule is `#[cfg]`-unaware
and honestly preserves both rather than guessing. Evaluating `#[cfg]` is out of
scope; the recorded candidates are correct under the bound. **Disposition:
documented; correct under the rule.**

## T3C-F002 — `ARTIFACT`, LOW — the artifact records every call's disposition

The candidate artifact emits one record per Rust call-like occurrence,
including `out_of_scope` records for non-plain-name calls. On tokio that is
43,016 records, ~88% of which are `out_of_scope`. This matches the TASK 3B
convention (which emits `OutOfScope` link records) and keeps the artifact
self-describing, but it makes the artifact ~0.54× the snapshot it derives from.
**Disposition: accepted; size is a documented trade-off, not a defect.**

## T3C-F003 — `MISSING_CANDIDATE`, MEDIUM — imports/re-exports are the dominant bounded miss

The rule searches only source-written `function` declarations in the lexical
chain, so a call whose target arrives through `use` or `pub use` gets
`no_candidate` even when a real declaration exists elsewhere. On tokio, 1,040
of 3,949 `no_candidate` calls (≈26%) name a `use`-imported leaf or alias. This
is the largest single coverage gap and is a *bounded miss*, not a
`FALSE_CANDIDATE`. **Disposition: recommends a separate bounded import-aware
candidate task (see T3C-F013).**

## T3C-F004 — `MACRO_BOUNDARY`, MEDIUM — macro-token-tree calls are unavailable

Calls written inside macro token trees (`my_macro! { helper() }`) are never
extracted by TASK 1 (TASK 2 F002), so they are `NOT_AVAILABLE_TO_TASK3C`, not
candidate false-negatives. The audit counts ~13,683 call-shaped tokens inside
macro bodies on tokio (an upper-bound approximation). **Disposition: a known
extraction boundary; out of scope for candidate generation.**

## T3C-F005 — `LEXICAL_SCOPE`, HIGH — local bindings can shadow a `fn` candidate (OPEN — blocked by fact model)

The rule searches `function` declarations only; `let`, `const`, `static` and
closure bindings are not declarations in the normalized model and therefore do
not shadow. `fn helper(){}; fn run(){ let helper = ||{}; helper(); }` returns
the module `fn helper` as a `single_candidate` even though the real binding is
the local closure. The module function is an eligible declaration in the
lexical chain, but returning it when a nearer `let`/`param`/pattern owns the
name is an unsafe over-approximation — the case the shadow-correction task
(`REPODEX-T3C-LEXICAL-SHADOW-CORRECTION-V1`) targeted.

*Original defect:* closer local value bindings are not modeled, so an outer
free `fn` is returned as a candidate when a nearer `let`/`param`/pattern owns
the written name.
*Reproduction:* `docs/TASK3C_CORRECTION.md` §2 — `run`/`runp`/inner-block cases
all emit `single_candidate(helper@0)`.
*Fix:* **BLOCKED** — `TASK3C_CORRECTION_BLOCKED_BY_FACT_MODEL`. Detecting the
mandatory `let`/parameter/pattern blockers requires a local-binding fact (plus
an enclosing-block live range, because `{ }` blocks are not scopes) that the
normalized model does not persist. That is a TASK 1/2 extraction-layer
extension and a blocking architectural dependency — see
`docs/TASK3C_CORRECTION.md`.
*Regression evidence:* cannot be added until the facts exist; the mandatory
fixtures are unsatisfiable under the current model.
*Residual unsupported binding forms:* `let`, function parameter, closure
parameter, `for`, `match`, `if-let`, `while-let`. Representable but
corpus-no-op: local `const`/`static`, same-name `use` imports.

**Disposition: open correctness defect; correction is blocked pending the
local-binding fact extension. A candidate is not a resolved call target — this
gap is documented rather than patched with an unsafe source scan.**

## T3C-F006 — `LEXICAL_SCOPE`, INFO — `impl` blocks are transparent

A method inside `impl S { ... }` that calls a free `helper()` reaches the
module-level `fn helper`, because `impl` contributes no module-item boundary.
This matches Rust semantics (an `impl` body's free-name lookups resolve against
the enclosing module). **Disposition: verified; correct.**

## T3C-F007 — `FALSE_CANDIDATE`, INFO — the only flag was a checker bug, not RepoDex

Mid-task the audit raised a `FALSE_CANDIDATE`: a `single_candidate` whose
declaration `name_range` text did not equal the callee. Investigation showed the
candidate was correct; the audit script had sliced a UTF-8-decoded string by
*byte* offsets, misaligning on multi-byte characters. Fixed by slicing source
bytes. The final audit reports **zero** `FALSE_CANDIDATE` across all 5,345
in-scope calls. **Disposition: checker fixed; no RepoDex defect.**

## T3C-F008 — `ARTIFACT`, INFO — provenance carries only occurrence-specific evidence

`CandidateProvenance` holds the scope path, search levels, enclosing module and
rule evidence — it no longer re-states the record's `source`, `written`,
`rule_id` or `language`, which already live once on the record. Combined with
concise per-call reasons, this cut the artifact ~41% (30.3MB → 17.7MB on
tokio). **Disposition: implemented.**

## T3C-F009 — `PERFORMANCE`, INFO — the rule is cheap; I/O dominates

On tokio the candidate build is ~1.23s, of which `derive` is ~118ms — the
lexical rule itself. The rest is `snapshot_load` (~317ms), `serialize` (~42ms)
and `verify` (~586ms, which re-loads every `FileAnalysis` to re-validate
locators). A full rebuild is cheap, so incremental candidate mutation is
unnecessary. **Disposition: documented; full rebuild retained.**

## T3C-F010 — `CANDIDATE_MODEL`, INFO — `single_candidate` is never `Exact` or `Ambiguous`

`CandidateOutcome` is a dedicated four-way cardinality with no `Exact` variant
and no ambiguity concept, so a single lexical match can never be re-labelled as
a resolved or structural target. Enforced by the type and by tests that
serialize the outcome and assert it is `single_candidate`, never `exact` or
`ambiguous`. **Disposition: enforced.**

## T3C-F011 — `MODULE_SCOPE`, INFO — nested functions shadow module-level ones

A `fn` nested inside a `fn` body is at lexical level 0, so it shadows a
same-named module-level `function`. `fn outer(){ fn helper(){}; helper(); }`
yields the nested `helper`, never both. Verified on the fixture and consistent
with Rust's inner-item shadowing. **Disposition: verified; correct.**

## T3C-F012 — `DETERMINISM`, INFO — the artifact is a pure function of its inputs

Candidate bytes and digest are identical across repeated builds, different
output directories and relocated checkout roots, and identical between an
incremental-update pipeline and a fresh-build pipeline on the same final bytes.
No checkout root, timestamp, PID or traversal order reaches canonical output.
**Disposition: verified on the fixture and the tokio corpus.**

## T3C-F013 — `MISSING_CANDIDATE`, MEDIUM — recommended smallest next step

The measured dominant gap is import-aware resolution (T3C-F003): ~1,040 tokio
calls name an imported leaf but get `no_candidate` only because the rule is
import-blind. The smallest valuable expansion is a bounded **import-aware
plain-name candidate** rule that lets a `use`-imported name point at the
import's resolved declaration — still without following re-exports transitively
or claiming semantic resolution. **Disposition: recommended; not implemented
here (out of scope for TASK 3C).**
