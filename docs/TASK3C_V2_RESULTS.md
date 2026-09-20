# TASK 3C — Lexical Shadowing Correction V2 — Results

`REPODEX-T3C-LEXICAL-SHADOW-CORRECTION-V2` closes `T3C-F005` (HIGH) by making
the Rust local-function candidate rule aware of persisted lexical blockers.
V1 was documentation-only and `BLOCKED_BY_FACT_MODEL`; the prerequisite
`REPODEX-T3C-RUST-LOCAL-BINDING-FACTS-V1` added `FileAnalysis.bindings`, so the
blockers are now normalized facts.

V2 implementation commit: `9933a80ed76192cd5ba3e21c4f969faf7f11fb84`.

## 1. Base

```text
bd94df396677107070ae3036914dad3907bf144a
```

which descends from `8884bc0bea9ae53ae44799ac042afafbccad7c91` →
`d147a3626c0740f1987d01f96efaac1be10b03a9`. Working tree was clean at start.

## 2. What V2 changed

The candidate rule `rust.call.local_function_candidate` consumes persisted
facts only — `bindings`, `declarations` (`function`/`constant`), `imports`,
`scopes`, `calls`. It never reparses source, runs Tree-sitter, regex-scans, or
invokes rust-analyzer/rustc/LSP.

### Blocker algorithm (`src/candidates/rule_rust.rs`)

```text
call byte = call.callee_range.byte_start
same-name covering local binding
    -> definite  : no_candidate(shadowed_by_local_binding)
    -> ambiguous : no_candidate(blocked_by_ambiguous_local_binding)
at each lexical level, innermost first (file/module boundary stops the walk):
    same-name `function`   -> candidate set for that level (stop)
    same-name `const`/`static` -> no_candidate(blocked_by_local_constant) (stop)
    same-name `use` leaf/alias -> no_candidate(blocked_by_import_binding) (stop)
nothing relevant -> continue outward
```

A binding applies to a call when `binding.covers(call_byte)` **and** the
call's scope is the binding's own scope or nested inside it only through
`closure` scopes (closures capture; a nested `fn`/method/item does not). When
several same-name bindings cover, the latest by `name_range.byte_start` wins.

`use` items resolve to their local name — `use p::x as alias` binds `alias`,
`use a::b` binds `b`, `use a::b::{self}` binds `b`, wildcard `use`s bind no
single name and never block. Blockers are blockers only — never candidates.

### Versions

`CANDIDATE_RULE_ABI_VERSION` 1 → 2, `POLICY_VERSION_RUST_CALL` 1 → 2. The
`CandidateFingerprint` changes (`rule_abi=1 rust=1` → `rule_abi=2 rust=2`), and
`verify` now rejects a manifest whose `candidate_fingerprint` differs from the
current build (`RuleFingerprintMismatch`) — stale V1 artifacts are
not analysis-compatible.

## 3. V1 failing reproductions, preserved

Under the blocker-blind rule each mandatory case emitted the unsafe outer
`fn`; under V2 each is correctly suppressed. Reproduced via the audit's
independent re-derivation:

```text
case                  V1                       V2
let-blocks            single_candidate(fn)     no_candidate(shadowed_by_local_binding)
param                 single_candidate(fn)     no_candidate(shadowed_by_local_binding)
nested-block          single_candidate(fn)     no_candidate(shadowed_by_local_binding)
use-in-fn             single_candidate(fn)     no_candidate(blocked_by_import_binding)
use-in-nested-mod     no_candidate             no_candidate(blocked_by_import_binding)
const                 single_candidate(fn)     no_candidate(blocked_by_local_constant)
```

(The `use-in-nested-mod` case was already `no_candidate` in V1 — the file-level
`fn` is outside `mod m`'s boundary — and V2 attributes the block to the import.)

## 4. Audit methodology

`scripts/task3c_audit.py` now derives the expected outcome independently with
the V2 blocker policy: it re-implements `covers`, the transparent-scope gate,
and `import_local_name`, and computes the nearest applicable blocker or the
`function` candidate set. It classifies every in-scope call against the emitted
record and never calls the production rule.

## 5. Real-corpus audit (tokio, 799 files, all in-scope calls)

```text
in-scope calls:            5,345
out-of-scope calls:       37,671   (37,671 OUT_OF_SCOPE_CORRECT)

correct none:              2,746
correct single:            1,394
correct multiple:              2
correctly blocked:
  local binding:             165
  ambiguous binding:           8
  import:                  1,030
  constant:                    0

FALSE_CANDIDATE:               0
WRONG_SCOPE_CANDIDATE:         0
MISSING_CANDIDATE:             0
BLOCKED_WRONG_REASON:          0
candidate TP/FP/FN:       1,396 / 0 / 0

previously-unsafe outer candidates prevented: 0 (defect latent on this corpus)
macro-hidden calls (NOT_AVAILABLE_TO_TASK3C): ~13,683 (upper bound)
```

**V1→V2 real-corpus diff:** 0 outcome-kind transitions. 1,203 `no_candidate`
records gain precise blocker provenance (165 `shadowed_by_local_binding`, 8
`blocked_by_ambiguous_local_binding`, 1,030 `blocked_by_import_binding`). No
`single`/`multiple` was suppressed, and none was fabricated — on tokio every
blocked call also lacked a reachable same-name `fn` in V1, so V1 was
coincidentally safe there (the defect is latent; the fixture matrix is where
V1 emitted the unsafe candidate).

## 6. Pipeline & determinism

- Upstream update-vs-fresh candidate equivalence: `let` added, `let` removed,
  parameter renamed, nested block changed, import blocker added, plus the
  pre-existing function/call/module cases — all produce identical
  `candidate_digest` and `call_candidates.jsonl` bytes.
- Repeated builds byte-identical; identical across output directories and
  across absolute checkout roots. Blocker provenance uses relative scope names
  and byte ranges — no absolute paths.
- Stale-V1 invalidation: a tampered `candidate_fingerprint` is rejected with
  `RuleFingerprintMismatch` (`a_stale_rule_fingerprint_is_rejected`).

## 7. Size & performance (tokio)

```text
candidate artifact:  V1 17,716,367 B  ->  V2 17,805,856 B   (+89,489 B, +0.5%)
records file:        17,713,813 B     ->  17,803,302 B
record count:        43,016 (unchanged — one record per call)
V2 candidates build: ~3.09 s   peak RSS ~166 MB
```

V1 build was ~3 s at ~149 MB in the prior task; the per-file binding-name index
adds a small constant. No performance budget applies — correctness is primary.

## 8. Verification

```text
cargo fmt --all -- --check                                        PASS
cargo check --locked                                              PASS
cargo test --locked                                               PASS (all suites)
cargo clippy --locked --all-targets --all-features -- -D warnings  PASS
cargo build --locked --release                                    PASS
scripts/task3c_audit.py --all                                     PASS (0 FALSE/WRONG_SCOPE)
```

## 9. Boundaries kept (not implemented)

Import-aware target candidates, re-export following, qualified-path, method,
associated-function, trait candidates, local callable target resolution, and
Go/Python/PHP candidates remain out of scope. A `use`/`const`/binding blocks an
unsafe outer candidate but is never itself a `CandidateTarget`.
