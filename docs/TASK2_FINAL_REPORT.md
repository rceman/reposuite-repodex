# TASK 2 Final Report

Validation of the TASK 1 foundation commit
`4a6e8e3fe88be068932279b3bf896c731c812859`.

The narrative per-language results are in `docs/SPIKE_RESULTS.md` Part II; the
findings log is `docs/TASK2_FINDINGS.md`; the pre-registered plan is
`docs/TASK2_VALIDATION_PLAN.md`.

---

## 1. validation_status

```text
VALIDATION_COMPLETE
```

Reevaluated in the audit-completion pass (§31). All mandatory measurements
M1–M13 of the frozen plan are now present, including the previously missing M4
extraction-quality audit with numeric TP/FP/FN.

## 2. architecture_recommendation

```text
GO
```

Reevaluated against the frozen plan's `GO` criteria (§9 of the plan), not
preserved from the previous conclusion. All twelve `GO` conditions are met and no
BLOCKER or correctness-related HIGH remains; the residual findings are MEDIUM/INFO
grammar boundaries or TASK 3 design inputs. See §31 for the itemised argument.

## 3. Exact TASK 1 commit validated

```text
4a6e8e3fe88be068932279b3bf896c731c812859
Gate Unix-only tests and correct overstated TASK 1 claims
```

## 4. Final TASK 2 commit SHA

```text
606faf13c86db890ce9f1f22627e8279e17a3017
Validate TASK 1 foundation and fix the PHP anonymous-class range

26d2bb9e2a40dbcfe4c910c68795b71c955885a4
Complete the TASK 2 extraction-quality audit and reissue GO
```

## 5. Corpora evaluated

Manifest: `benchmarks/corpora.json` (`reposuite-repodex/task2-corpora/1`).
Pinned revisions are in the manifest; local paths are supplied externally.

```text
id                          language  files    bytes       LOC
public:BurntSushi/ripgrep   rust        110    1,902,104    56,386
public:clap-rs/clap         rust        337    2,595,314    84,668
public:serde-rs/serde       rust        208    1,246,220    42,623
public:tokio-rs/tokio       rust        799    5,793,899   183,416
local:reposuite-repodex     rust         56      514,620    14,677
public:gin-gonic/gin        go           99      694,082    24,226
public:spf13/cobra          go           36      512,600    16,765
public:gohugoio/hugo        go          912    6,236,812   230,409
local:repodex               go          175      821,575    32,066
local:gpt-tunnel-gateway    go          872    4,365,341   118,181
public:pallets/flask        python       83      589,437    18,345
public:psf/requests         python       37      407,127    12,032
public:django/django        python    2,932   19,402,248   526,612
local:vps_desk              python    1,058   10,760,626   309,855
public:laravel/framework    php       3,095   17,817,062   562,801
public:composer/composer    php         622    4,904,238   133,843
public:symfony/console      php         378    1,977,753    55,692
```

Per-language totals — every language exceeds the 150k LOC requirement:

```text
language  files  bytes       LOC
rust      1,510  12,052,157   381,770
go        2,094  12,630,410   421,647
python    4,110  31,159,438   866,844
php       4,095  24,699,053   752,336
total    11,809  80,541,058  2,422,597
```

## 6. Verification results

```text
cargo fmt --all -- --check                                       PASS
cargo check --locked                                             PASS
cargo test --locked                                              PASS  157 passed, 0 failed
cargo clippy --locked --all-targets --all-features -- -D warnings PASS  0 warnings
cargo build --locked --release                                   PASS
cargo bench                                                      PASS
Windows cross-target clippy (x86_64-pc-windows-gnu)              PASS (TASK 1 pass)
WINDOWS_NATIVE_NOT_EXECUTED — no native Windows host; none claimed
```

Evidence: `verification.txt` in the run directory.

## 7. Parsing / recovery per language

```text
language  visited  supported  clean  recovered  unsupported  parser failures
go          4,047      2,140   2,140          0        1,907               0
php         5,047      4,053   4,052          1          994               0
python      7,874      3,341   3,341          0        4,533               0
rust        2,197      1,520   1,503         17          677               0
total      19,165     11,054  11,036         18        8,111               0
```

All 18 recoveries are explained: 17 Rust (13 = F001 in clap `tests/**`, 4 =
intentional `malformed.rs` fixture runs) and 1 PHP (a malformed file).

## 8. Processing coverage and recovery denominators

```text
recovery rate (recovered / parsed):    18 / 11,054 = 0.163%
processing coverage (supported / visited): 11,054 / 19,165 = 57.7%
parser failures:      0
extraction failures:  0
read failures:        0
```

The recovery denominator is files that parsed, not files visited. The 8,111
unsupported files are files whose language RepoDex does not support (or
non-source content in the checkouts), not parse failures.

Facts across the corpus:

```text
declarations 180,109   imports 41,508   references 38,794
calls 696,545          test candidates 44,754
processed bytes ~68 MiB
```

## 9. Extraction audit methodology

This section records the **completed** extraction-quality audit. An earlier pass
of this report claimed `VALIDATION_COMPLETE` while stating that numeric TP/FP/FN
had not been computed; that was inconsistent with the frozen plan, which lists
the extraction-quality audit as mandatory measurement M4 and requires
`VALIDATION_BLOCKED` / `NOT_ISSUED` when a mandatory measurement is missing. The
audit below closes that gap. The frozen plan was not modified.

**Frozen regions.** Regions were selected from source properties only — path
patterns and source-content markers such as test-declaration conventions
(`#[test]`, `func Test`, `def test_`, `function test`) — and frozen before any
RepoDex prediction was collected. They are declared in
`scripts/task2_extraction_audit.py` (`REGIONS`) and recorded in
`audit/frozen-regions-v2.json`. No region was chosen from a RepoDex prediction.

```text
id                    language  file / lines                                              role
rust-primary          rust      serde test_borrow.rs 1-196                              primary
rust-supplementary    rust      tokio sync_broadcast.rs 55-132                          F002 macro boundary
go-primary            go        cobra args_test.go 1-250                                primary
go-supplementary      go        gin gin_integration_test.go 7-26                        import items
python-primary        python    django template_tests/test_loaders.py 1-277             primary
php-primary           php       laravel PromptsAssertionTest.php 1-424                  primary
```

**Matching rules** (fixed before scoring; the frozen plan §7):

```text
same category and same occurrence identity
declarations match on recorded name, including struct fields and enum variants
imports counted per statement and per item
call-like occurrences match on (row, callee name)
test candidates match on the declaration name carrying the evidence
one-to-one matching: unmatched expected = FN, unmatched prediction = FP,
duplicate prediction = FP, matched = TP
semantic ambiguity is not extraction failure: an unresolved member/selector
target is still a correct call-like occurrence, never an FP
a Rust call written inside a macro argument is a documented policy boundary
(F002); it is reported separately as a strict FN and excluded from contract recall
```

Two independent measurements were made per region: RepoDex predictions whose
anchor lies fully inside the region, and a source-only scanner that never reads
RepoDex output. Every disagreement was inspected by hand.

## 10. Extraction results per language

Hand-verified TP/FP/FN, precision and recall per category. `n` is the annotated
sample size in the region.

```text
language  category      region              n     TP   FP   FN   precision  recall
rust      declarations  primary            32     32    0    0    100%       100%
rust      imports       primary             5      5    0    0    100%       100%   (5 stmts / 10 items)
rust      calls         primary            42     42    0    0    100%       100%
rust      tests         primary            12     12    0    0    100%       100%
rust      declarations  supplementary       5      5    0    0    100%       100%
rust      calls         supplementary      55     42    0   13     100%       76.4%  (strict; 100% contract)
rust      tests         supplementary       4      4    0    0    100%       100%

go        declarations  primary            30     30    0    0    100%       100%
go        calls         primary            92     92    0    0    100%       100%
go        tests         primary            20     20    0    0    100%       100%
go        imports       primary+supp        2      2    0    0    100%       100%   (2 stmts / 20 items)

python    declarations  primary            32     32    0    0    100%       100%
python    imports       primary            10     10    0    0    100%       100%   (10 stmts / 11 items)
python    calls         primary           108    108    0    0    100%       100%
python    tests         primary            21     21    0    0    100%       100%

php       declarations  primary            49     49    0    0    100%       100%
php       imports       primary            14     14    0    0    100%       100%   (14 stmts / 14 items)
php       calls         primary           142    142    0    0    100%       100%
php       tests         primary            15     15    0    0    100%       100%
```

```text
aggregate   declarations  148 TP / 0 FP / 0 FN   precision 100%   recall 100%
            imports        55 TP / 0 FP / 0 FN   precision 100%   recall 100%   (items)
            calls         426 TP / 0 FP / 13 FN  precision 100%   recall 96.9%  (strict)
            tests          72 TP / 0 FP / 0 FN   precision 100%   recall 100%
```

**The 13 strict false negatives are all one thing.** They are calls written
inside Rust macro arguments (`assert_ok!(tx.send("hello"))`,
`assert_pending!(recv.poll())`, `assert!(recv.is_woken())`,
`assert_ready_ok!(recv.poll())`). tree-sitter-rust parses macro arguments as a
flat `token_tree` with no expression subtrees, so the calls cannot be seen
without a token-tree expression parser. This is the documented F002 boundary and
is stated in `docs/LANGUAGE_SPIKE.md`; it is a deliberate policy limit, not an
unnoticed defect. Reported two ways so nothing is hidden:

```text
strict (macro-argument calls are required):   426 / 439 = 97.0% recall overall
contract (macro-argument calls out of scope): 426 / 426 = 100% recall overall
```

**Negative examples in the test category** were audited and correctly left
unmarked: 2 non-`#[test]` Rust functions, 9 non-`Test` Go helpers, 7 non-`test_`
Python methods and the PHP `handle`/`__construct` methods carry no test evidence.

**Qualitative mismatches.** No false positives and no false negatives were found
in the primary regions. Two scanner disagreements were inspected and resolved as
audit artifacts, not defects:

* PHP: the source scanner undercounted 14 `new class extends ...` constructions
  (anonymous classes without an argument list) and one import-count difference
  traced to the trait `use` case F004; RepoDex was correct.
* Rust primary: the scanner over-counted 5 pattern/tuple-struct occurrences
  (`Cow::Owned(ref s)`, `Str(&'a str)`) and mangled 4 turbofish callee names
  (`assert_de_tokens_error::<&str>`); after correction the match is exact.

One documentation gap was found and fixed without code change: the PHP language
constructs `empty($x)` / `isset($x)` are shaped as calls by the grammar and
recorded as `plain_name` occurrences (finding F009).

**Holdout.** The regions above were frozen after the F003 range fix and none of
them was used to derive it, so they act as independent holdout evidence: the PHP
region contains 15 anonymous-class constructions and **all 15 have a callee range
of exactly the 5 bytes `class`**, confirming the F003 fix generalises beyond its
regression fixture.

## 11. Source-range validation

Committed script: `scripts/task2_range_validation.py`. Every emitted range must
be within the source, rows ordered, and the recorded `(row_start, column_start)`
equal to the true UTF-8 byte position of `byte_start`.

```text
language  files  ranges  bad ranges  multibyte files
rust         80  10,733           0                0
go           80  15,993           0                4
python       80   2,326           0                7
php          80   7,006           0                1
total       320  36,058           0               12
```

CRLF handling was validated separately by converting every fixture LF→CRLF:

```text
language  files  ranges  bad ranges  crlf files  recovered
rust          8     407           0           8          1
go            6     328           0           6          1
python        6     313           0           6          1
php           6     340           0           6          1
total        26   1,388           0          26          4
```

Evidence: `diagnostics/range-validation.txt`.

## 12. Determinism validation

Three full canonical scans each, comparing top-level digest and complete fact
signature:

```text
corpus                files  digest                    distinct digests  distinct signatures
django/django         2,932  fnv1a64:0b54ce97af0cf2fb            1                  1
laravel/framework     3,086  fnv1a64:205500ee5ee155a7            1                  1
gohugoio/hugo           912  fnv1a64:9d4beccaf8bbfafc            1                  1
```

Different checkout roots, identical output:

```text
pallets/flask  rootA = rootB = fnv1a64:3ee14e1e50449c3c
spf13/cobra    rootA = rootB = fnv1a64:8c7d031ead80281c
```

Evidence: `diagnostics/determinism.txt`.

## 13. Real-file incremental correctness

`tests/task2_real_incremental.rs`: **12 real corpus files, 8 comparisons each**
(single edits of several kinds plus a multi-step sequence). Incremental result
equivalent to an independent fresh parse+extract in every comparison. The skip
path when `REPODEX_TASK2_CORPUS_DIR` is unset was exercised and passes.

## 14. Synthetic benchmark results

Artifacts:
`~/reposuite/repodex/benchmarks/task2-validation-20260917T101413Z-synthetic/`
(`results.txt`, `results.json`). Stages: discovery/read, parser init + first
parse, raw parse, normalized extraction, full parse + extraction, incremental
parse only, incremental parse + extraction, default 8 MiB acceptance, over-limit
rejection, explicit raised max-file-size.

```text
rust 100 KiB full parse + extraction   ~22.4 ms
rust   1 MiB full parse + extraction  ~255.6 ms
php    4 MiB full parse + extraction ~1,331.9 ms
php over-8-MiB file: rejected under the default limit; processed when raised
```

## 15. Real repository full-scan results

Five fresh processes per corpus, single-threaded, release build. Combined:
**8,838 files, 55.2 MiB, 1,764,320 LOC, 824,781 facts.**

```text
corpus                  lang    files     LOC     median ms  files/s  MiB/s  kLOC/s
public:laravel/framework php    3,086   562,801    5,021.3   614.6    3.38   112.1
public:django/django     python 2,932   526,612    6,098.7   480.8    3.03    86.3
public:gohugoio/hugo     go       912   230,409    2,266.8   402.3    2.62   101.6
public:tokio-rs/tokio    rust     799   183,416    2,206.4   362.1    2.50    83.1
public:composer/composer php      589   133,843    2,086.2   282.3    2.19    64.2
public:clap-rs/clap      rust     338    84,668      925.5   365.2    2.68    91.5
public:gin-gonic/gin     go        99    24,226      296.6   333.8    2.23    81.7
public:pallets/flask     python    83    18,345      176.6   470.1    3.18   103.9
```

Five single repositories exceed 150k LOC (laravel, django, hugo, tokio, and the
local `vps_desk` Python tool), so the single-repository size requirement is met
without a synthetic collection.

## 16. Stage timing breakdown

The synthetic matrix separates stages so cost can be attributed rather than
averaged. Extraction is a substantial share of the combined cost — not a
rounding error — which is why the incremental end-to-end gain in §20 is far
smaller than the parse-only gain.

## 17. Throughput denominators

Throughput is against **supported files actually processed**, not visited files.
MiB/s uses processed source bytes. kLOC/s uses the manifest LOC counts.
Synthetic sources are regular and repetitive, so the synthetic figures are an
optimistic bound, not a target.

## 18. Process memory measurements

Peak RSS of the end-to-end scan processes (`Maximum resident set size`):

```text
tokio/rust     30,132 KiB   29.4 MiB
hugo/go        32,480 KiB   31.7 MiB
django/python 104,876 KiB  102.4 MiB
laravel/php   123,920 KiB  121.0 MiB
```

Evidence: `memory/process-memory.txt`.

## 19. Retention A/B/C memory comparison

`examples/retention.rs`, one process per variant, peak RSS = kernel `VmHWM`:

```text
corpus   variant  files  bytes       facts    trees  sources  peak RSS
tokio    A          874  5,793,899   62,373      0       0      10.4 MiB
tokio    B          874  5,793,899   62,373    799       0     164.0 MiB
tokio    C          874  5,793,899   62,373    799     799     169.6 MiB
hugo     A        2,568  6,236,812   73,233      0       0       9.2 MiB
hugo     B        2,568  6,236,812   73,233    912       0     158.6 MiB
hugo     C        2,568  6,236,812   73,233    912     912     164.5 MiB
django   A        7,091 19,402,248  278,589      0       0      20.8 MiB
django   B        7,091 19,402,248  278,589  2,932       0     518.2 MiB
django   C        7,091 19,402,248  278,589  2,932   2,932     536.8 MiB
laravel  A        3,411 17,817,062  266,600      0       0      36.2 MiB
laravel  B        3,411 17,817,062  266,600  3,095       0     650.5 MiB
laravel  C        3,411 17,817,062  266,600  3,095   3,095     667.5 MiB
```

Fact counts identical across variants in every language. Retaining trees
dominates memory (~15–18×); retaining sources adds roughly their own size. F006.

## 20. Incremental performance

```text
stage                            rust 100 KiB    php 1 MiB
fresh parse                          12.845 ms    215.740 ms
incremental parse only                0.675 ms      6.783 ms   (~19×, ~32×)
fresh parse + extraction             22.431 ms    414.228 ms
incremental parse + extraction       10.745 ms    156.137 ms   (~2.1×, ~2.7×)
```

Extraction is whole-file, so the end-to-end incremental gain is ~2×, not ~20×.
F007.

## 21. Large / deep workload results

```text
8,388,608 bytes (exact limit), default guard:  accepted
8,388,609 bytes (one over), default guard:    refused, status unsupported,
  diagnostic file_too_large ("larger than the configured maximum of 8388608
  bytes (the read was truncated after 8388609 bytes)"), exit 1, no panic,
  0 bytes retained
8,388,609 bytes, --max-file-size 16777216:    processed
500 nested `if true {}` levels, 1,006,026 bytes:  clean, no stack overflow
```

Evidence: `diagnostics/large-deep.txt`.

## 22. All unresolved BLOCKER/HIGH findings

```text
none
```

The only HIGH finding (F003, PHP anonymous-class range) was fixed. Remaining
open findings are F001 (MEDIUM, grammar), F002 (MEDIUM, grammar/recall), F008
(MEDIUM, model, carried from TASK 1), F009 (LOW, documentation of a grammar
shape), and INFO items F004–F007.

## 23. Fixes made during TASK 2

```text
F003  src/parser/php.rs — anonymous-class callee range narrowed to the `class`
      keyword, robust to attributed anonymous classes
      fixtures/php/calls.php — added a plain and an attributed anonymous class
      fixtures/expected/php/calls.php.txt — re-blessed
      tests/php_adapter.rs — new regression test
```

Plus non-code deliverables: `docs/TASK2_VALIDATION_PLAN.md`,
`docs/TASK2_FINDINGS.md`, `docs/TASK2_FINAL_REPORT.md`,
`benchmarks/corpora.json`, `scripts/task2_range_validation.py`,
`scripts/task2_extraction_audit.py`, `examples/retention.rs`,
`tests/task2_real_incremental.rs`.

The audit-completion pass added one documentation sentence (F009) to
`docs/LANGUAGE_SPIKE.md` and changed no production code.

No other production code changed. The architecture boundary is unchanged.

## 24. Pre-fix / post-fix evidence

```text
                      pre-fix   post-fix
PHP files scanned      4,095     4,095
anonymous-class facts    569       569
incorrect callee ranges  569         0
affected files           182         0
```

All performance, memory and retention measurements in this report were taken
after the F003 fix, so no pre-change performance is reported as post-change
evidence. Evidence: `diagnostics/F003-php-anonymous-class.txt`.

## 25. Anything NOT MEASURED

```text
* native Windows execution (cross-target type-check only)
* multi-threaded / parallel scan throughput (scans were single-threaded by design)
* generated-source effects measured separately
* cross-file or semantic resolution (out of scope for TASK 2)
* long-duration soak / memory-growth-over-time behaviour
* very large single files beyond the raised limit
* the old Go RepoDex comparison (see §26)
```

The previously-listed gap "numeric TP/FP/FN precision/recall against
hand-annotated ground truth" is **now measured** — see §9 and §10.

## 26. Optional old RepoDex comparison

```text
NOT PERFORMED
```

The old Go RepoDex implementation was included as a Go corpus for parsing, but no
behavioural or performance comparison against it was run, and none is claimed.
`task2.md` marks this comparison optional.

## 27. Exact evidence behind the architecture recommendation

`GO` rests on the frozen plan's `GO` conditions (plan §9), each evidenced:

```text
* all mandatory measurements M1-M13 present, including M4 extraction audit
* 0 parser failures, 0 extraction failures, 0 read failures across 19,165 files
* recovery 18 / 11,054 = 0.163%, every recovery explained
* extraction audit: 100% precision in every region; 100% contract recall;
  strict call recall 96.9%, the 13 misses all the documented F002 macro boundary
* 0 bad ranges over 320 real files (36,058 ranges) and 26 CRLF conversions
* canonical digest + complete-fact signature identical across runs and roots
* incremental result equivalent to fresh parse on 12 real files
* one genuine defect (F003) found, fixed, regression-tested, re-verified to 0,
  and independently confirmed by 15/15 correct ranges in the audit holdout region
```

Residual findings are non-blocking and each belongs to a later layer: F001/F002
(Rust grammar macro boundaries, out of scope by design), F009 (documentation of a
grammar shape), F008 (unproduced model variant carried from TASK 1), and F006/F007
(retention and whole-file extraction measurements that inform TASK 3 design). No
unresolved BLOCKER and no unresolved correctness-related HIGH remains, so the
unconditional `GO` conditions are satisfied.

## 28. TASK 3 readiness

TASK 3 can proceed. It should still respect the recorded boundaries: do not
retain a tree per file for the whole repository (F006); do not assume incremental
extraction is cheap (F007); keep Rust macro recall boundaries visible (F001,
F002); keep recovery and grammar limitations explicit in any new layer.

## 29. Smallest recommended TASK 3

A **bounded, non-retaining repository pass** that consumes the existing
normalized syntax facts and produces a per-file, deterministic index — no
semantic resolution, no resolved call graph, no retained trees — so the next
layer is built on validated, deterministic facts rather than unresolved
assumptions.

## 30. Raw benchmark artifact path

```text
~/reposuite/repodex/benchmarks/task2-validation-20260917T101413Z/
  environment.json  corpora.json  raw-results.jsonl  end-to-end.json
  verification.txt  repodex-commit.txt  repodex-dirty.txt
  validation-plan.sha256
  memory/     process-memory.txt  retention-abc.txt
  diagnostics/ F003-php-anonymous-class.txt  determinism.txt
               large-deep.txt  range-validation.txt
  audit/      frozen-regions.json      extraction-audit.json
              frozen-regions-v2.json   extraction-audit-v2.json
~/reposuite/repodex/benchmarks/task2-validation-20260917T101413Z-synthetic/
  results.txt  results.json
```

Large machine-generated benchmark output is not committed; the small, stable
manifest (`benchmarks/corpora.json`) and the validation scripts
(`scripts/task2_range_validation.py`, `scripts/task2_extraction_audit.py`) are.

---

## 31. Audit-completion pass

The first pass of this report issued `VALIDATION_COMPLETE` / `CONDITIONAL_GO`
while stating that numeric TP/FP/FN had not been computed. The frozen plan lists
that audit as mandatory measurement M4 and requires `VALIDATION_BLOCKED` /
`NOT_ISSUED` when a mandatory measurement is missing, so the first-pass status was
inconsistent with the plan. This pass closes the gap.

**What the frozen plan required** (unchanged): mandatory measurement M4 —
extraction-quality audit against frozen, independently selected regions, with
TP/FP/FN, precision and recall per language and category, one-to-one matching,
sample targets of >=30 declarations, >=30 calls, >=10 imports and >=10
test-candidate examples (including negatives) per language, and semantic
ambiguity kept separate from extraction failure. Section 4.1 marks it mandatory;
section 11 makes a missing mandatory measurement a blocking condition.

**What was done**: regions frozen from source properties only
(`audit/frozen-regions-v2.json`, `scripts/task2_extraction_audit.py`); independent
source-only annotation of every in-scope occurrence; RepoDex predictions
collected for the same regions; one-to-one matching; full TP/FP/FN table (§10).

**Result**: 100% precision in every category and language; 100% contract recall;
strict call recall 96.9% with all 13 misses being the documented F002 macro
boundary. No new defect was found. One documentation gap (F009) was corrected
without a code change.

**Holdout**: the audit regions were frozen after the F003 fix and none was used
to derive it; the PHP region independently confirms the fix (15/15 anonymous-class
callee ranges exactly 5 bytes).

**Fix discipline**: no code defect was exposed by this pass, so no pre-fix /
post-fix code evidence is reported. The F003 pre/post evidence in §24 is
unchanged and preserved.

**Status reevaluation**: with M4 present, all M1–M13 are satisfied. Every
`GO` condition in plan §9 is met, no BLOCKER and no correctness-related HIGH
remains, so `validation_status = VALIDATION_COMPLETE` and
`architecture_recommendation = GO`. `CONDITIONAL_GO` was not retained merely
because it was the previous conclusion; it was tested against the frozen criteria
and the criteria are now met. Residual MEDIUM/INFO findings remain documented and
are constraints on TASK 3, not on the validity of the TASK 2 foundation.

**TASK 3**: may proceed. TASK 3 was not started in this pass.

**Commit**: `26d2bb9e2a40dbcfe4c910c68795b71c955885a4` — "Complete the TASK 2
extraction-quality audit and reissue GO". Not pushed.
