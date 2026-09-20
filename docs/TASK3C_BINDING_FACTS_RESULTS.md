# TASK 3C-BF — Rust local-binding facts foundation

Status: `LOCAL_BINDING_FACTS_COMPLETE`

`PROMPT_ID: REPODEX-T3C-RUST-LOCAL-BINDING-FACTS-V1`
`PREVIOUS_PROMPT_ID: REPODEX-T3C-LEXICAL-SHADOW-CORRECTION-V1`

This task extended the normalized fact foundation so a persisted `FileAnalysis`
can represent Rust local name bindings and the bounded source regions in which
each binding can conservatively shadow an outer name. It is a *fact-extraction*
extension only: no candidate rule, link rule, or import-aware logic was added.
The open shadowing defect `T3C-F005` is intentionally still open — it now has
the facts a correction V2 needs, but V2 was not started.

## 1. Base commit

```text
d147a3626c0740f1987d01f96efaac1be10b03a9   (TASK 3C shadow-correction blocked report)
```

## 2. Implementation / report commit(s)

```text
8884bc0bea9ae53ae44799ac042afafbccad7c91   Add normalized Rust local-binding facts with bounded visibility ranges
```

The working tree at commit time contains the model, adapter, snapshot and test
changes described below, on top of the required base `d147a36`.

## 3. New fact model

A new normalized fact, `LocalBindingOccurrence` (`src/model/facts.rs`), persisted
as `FileAnalysis.bindings: Vec<LocalBindingOccurrence>`:

```text
binding_id          u32   — deterministic, reindexed in canonical order
snapshot_id         String
language            LanguageId
relative_path       String
scope_id            u32   — nearest persisted scope (fn/closure/module/file)
kind                BindingKind
name                String
name_range          SourceRange — exact written identifier
binding_site_range  SourceRange — the introducing syntax node
visibility_ranges   Vec<SourceRange> — half-open regions it may shadow
ambiguous           bool  — refutable bare-identifier uncertainty
```

`visibility_ranges` is deliberately a fact-local representation — ordinary
`{ ... }` blocks were *not* promoted to first-class `Scope`s. A later consumer
answers "does binding X cover byte P?" via `LocalBindingOccurrence::covers(P)`,
a pure source-range test, without reparsing source. It is explicitly **not**
semantic name resolution.

## 4. Binding kinds

```text
let
function_parameter
closure_parameter
for_pattern
match_pattern
if_let_pattern
while_let_pattern
```

No generic `Variable` collapse — each fact records *why* the name exists.

## 5. Pattern extraction policy

`bound_names` walks a pattern and emits one fact per written bound identifier:

* `identifier` → a bound name (ambiguity depends on position, §6);
* `shorthand_field_identifier` (`S { f }`) → a definite binding;
* `captured_pattern` (`name @ pat`) → `name` is a definite binding, then recurse
  into the sub-pattern;
* containers (`tuple`, `slice`, `or`, `parenthesized`, `tuple_struct`, `struct`,
  `field`, `ref`, `mut`, `reference`, `box`, `unary`, `deref`, `conjunction`,
  `identifier_pattern`) → recurse into children, **skipping the `type` field**
  so a struct/tuple-struct constructor name is never a binding;
* everything else — literals, `_` wildcards, `..`, paths, scoped paths — is not
  a binding (under-capture rather than over-capture on unknown shapes).

`_` never produces a fact. Local `const`/`static` remain `Declaration`s and are
deliberately **not** duplicated into `bindings` — the correction V2 jointly uses
`bindings` + `Constant` declarations + `ImportOccurrence`s.

## 6. Ambiguity policy

A bare `identifier` in a **refutable** position (`match` arm pattern, `if let`,
`while let`, `let`...`else`) cannot be proven by syntax alone to bind a fresh
local rather than name a unit variant / constant / path. Such a binding is
recorded with `ambiguous = true`. In **irrefutable** positions (`let`, `for`,
parameters) a bare identifier is definite (`ambiguous = false`).
`shorthand_field_identifier` and `name @ ...` captures are definite everywhere.

This keeps uncertainty visible instead of claiming certainty: a consumer may
treat `ambiguous` bindings as "possible" blockers, trading recall for the risk
of over-blocking on const/variant-like arms (`None`, `CONST`, `x` are all
recorded as ambiguous). This is the documented precision/recall trade-off.

## 7. Visibility representation and per-kind rules

`visibility_ranges: Vec<SourceRange>` — canonical half-open source ranges. All
are bounded by real Tree-sitter node boundaries; no `Scope` objects were added.

| kind | visibility |
|------|-----------|
| `let` | `[let_statement_end → enclosing_block_end]`. `let_declaration` includes the `;`, so visibility begins *after* the statement — never covering the initializer or prior code. A `let`...`else` binding covers the same region. |
| `function_parameter` | the whole function `body` block. A signature-only `fn`/`trait` item has no body → empty `visibility_ranges`. |
| `closure_parameter` | the closure `body` only — never leaks to the enclosing block; nested closures are bounded individually. |
| `for_pattern` | the loop `body` only — not the iterator `value`, not code after the loop. |
| `match_pattern` | the arm `condition` (guard) **and** the arm `value` (body) — two disjoint ranges when a guard is present, one otherwise. Does not leak into sibling arms. |
| `if_let_pattern` | `[let_condition_end → consequence_end]`. In a `let`-chain an earlier condition's binding covers later conditions' values plus the consequence. Never covers the `else` alternative. |
| `while_let_pattern` | the loop `body` only — not the condition `value`, not code after the loop. |

### `let` initializer

The new binding is not live inside its own initializer (`let helper = helper()`):
the initializer sits *before* the statement end, so it is outside
`[let_end → block_end]`. Verified by a dedicated test.

### Nested blocks

Because visibility ends at the enclosing block's `}`, a `let` inside `{ ... }`
covers only the remainder of that block; calls after the block closes are
outside the range (the mandatory acceptance case).

### Sequential same-name shadowing

`let helper` … `let helper` produces two facts whose visibilities may overlap
toward the block end. Both are preserved (no merging); a consumer picks the
nearest by `name_range`/visibility ordering.

## 8. `self` / receiver policy

`self`, `&self`, `&mut self`, `mut self`, `self: Ty` are `self_parameter` nodes
and are **not** emitted — `self` is a receiver, not a written name that can be a
plain-name callee. Documented, not accidental.

## 9. Findings (`T3C-BF-*`)

| ID | sev | disposition | finding |
|----|-----|-------------|---------|
| T3C-BF-F001 | medium | documented | Refutable bare-identifier bindings (`None`, `CONST`, `x` in `match`/`if let`/`while let`/`let else`) are emitted with `ambiguous = true`; a consumer must not treat them as definite blockers. |
| T3C-BF-F002 | low | documented | A `match`-arm binding covers the arm body even when the pattern may not match (e.g. a guard fails). The fact records *where the name would be bound*, conservatively. |
| T3C-BF-F003 | low | documented | Unrecognized pattern node kinds under-capture (emit nothing) rather than risk a wrong fact — see `PATTERN_CONTAINERS`. |
| T3C-BF-F004 | low | documented | `let` visibility runs to the block end without modeling divergence (`return`, `break`, `?`); it is conservative over-coverage for shadowing purposes. |
| T3C-BF-F005 | medium | documented | Bindings inside `macro_rules!`/`macro_invocation` bodies are not extracted — macros are never expanded (existing boundary). This is the dominant "missed" source in the real corpus. |
| T3C-BF-F006 | info | recorded | Snapshot size grew ~30.8% on tokio (see §15). |
| T3C-BF-F007 | low | documented | In an `if let`/`let`-chain, an earlier condition's binding covers later conditions' value regions plus the consequence — an approximation of Rust's chain scoping. |

## 10. Serialization & round-trip

`LocalBindingOccurrence` derives `Serialize`/`Deserialize` and lives inside
`FileAnalysis`, so it flows through TASK 3A snapshot build/verify/load and
JSON round-trip exactly — no in-memory side channel. `bindings_survive_a_
snapshot_round_trip` asserts exact persistence and dense deterministic IDs.

## 11. Schema / ABI changes and invalidation

```text
SCHEMA_VERSION            1 → 2   (normalized facts gained a `bindings` collection)
ANALYSIS_ABI_VERSION      1 → 2   (the Rust adapter emits a new fact kind)
MANIFEST_VERSION          1       (unchanged — manifest format did not change)
candidate_rule_abi_version 1      (unchanged — no candidate rule changed)
link rule ABI             —       (unchanged — no link rule changed)
```

Invalidation chain: `AnalyzerFingerprint::text` embeds `schema` + `abi`, so a
snapshot built by the pre-extension analyzer has a different fingerprint and is
not reused — an old snapshot lacking `bindings` is never treated as
analysis-compatible. Confirmed: the stored pre-extension tokio artifact is
`schema_version: 1` with no `bindings` key.

## 12. Real-corpus validation (tokio, 799 `.rs` files)

Binding facts emitted:

```text
let                    11,551
function_parameter      4,121
match_pattern           1,205
closure_parameter         897
for_pattern               407
if_let_pattern            307
while_let_pattern          94
-------------------------------
total                  18,582   (ambiguous 1,578)
```

695 of 799 files produced at least one binding.

### Audit methodology & results

`scripts/task3c_binding_audit.py` reads the snapshot and source tree. A
deterministic sample (seed `0xC0FFEE`) of **150** bindings across all kinds was
validated for: `name_range` slices exactly to the written identifier;
`binding_site_range` contains the name; every `visibility_ranges` is in-bounds
and half-open; non-parameter bindings have ≥1 visibility range.

```text
audited                       150
name_slice_or_bounds            0   failures
site_containment                0   failures
empty_visibility_non_parameter  0   failures
visibility_out_of_bounds        0   failures
```

### Negative audit (over-capture)

```text
wildcard `_` bindings                0
declaration-name-range collisions    0   (fn/type/method/module/macro names)
non-identifier binding names         0
call-callee-range collisions         0   (a use position is never a binding)
```

### TP / FP / FN

* TP (precision): 150/150 sampled bindings are correct → 0 FP detected.
* FN (recall): no systematic misses for supported forms. A comment/string-
  stripped heuristic lower-bound for `for <ident> in` and `let <ident>` matched
  or under-shot the emitted counts; the only per-file shortfalls are `for`/`let`
  inside `macro_rules!`/macro bodies, which are deliberately not expanded
  (T3C-BF-F005) — correct, not real FNs.
* Duplicate facts: none observed; each bound identifier emits exactly once.

## 13. Snapshot-size impact (tokio)

```text
before (pre-extension artifact)   32,723,988 bytes
after  (this extension)           42,798,538 bytes
added                             +10,074,550 bytes  (+30.8%)
binding facts                      18,582   (~542 JSON bytes/fact)
```

## 14. Rust snapshot build-time observation

Fresh `index build` of tokio: **~2.20 s** wall (release binary), 799 clean
files. No hard budget; recorded for regression awareness.

## 15. Does T3C-F005 now have the facts it needs?

Yes. A correction V2 can now answer "does a same-name local binding cover this
call position?" using only persisted facts: `bindings` (with `visibility_ranges`
+ `covers`) plus existing `Constant` declarations and `ImportOccurrence`s — no
source reparsing. The mandatory shadowing shapes (`let`, parameter, closure,
`for`/`match`/`if let`/`while let`, nested-block `let`, initializer exclusion,
sequential `let`s) are all represented.

## 16. Scope-keeping confirmations

* TASK 3C candidate semantics were **not** modified — `src/candidates/` and
  `src/links/` are git-clean; the rebuilt tokio candidate set is unchanged
  (`single`+`multiple` = 1,396, matching TASK 3C's TP=1,396).
* Lexical-shadow correction V2 was **not** started.
* Import-aware candidates were **not** implemented.
* Nothing was pushed.

## 17. Exact Cargo verification

```text
cargo fmt --all -- --check                                       PASS
cargo check --locked                                             PASS
cargo test --locked                                              PASS (all green)
cargo clippy --locked --all-targets --all-features -- -D warnings PASS
cargo build --locked --release                                   PASS
```

```text
SOURCE_PROMPT_ID: REPODEX-T3C-RUST-LOCAL-BINDING-FACTS-V1
PREVIOUS_PROMPT_ID: REPODEX-T3C-LEXICAL-SHADOW-CORRECTION-V1
```

PROMPT_ID: REPODEX-T3C-RUST-LOCAL-BINDING-FACTS-V1
