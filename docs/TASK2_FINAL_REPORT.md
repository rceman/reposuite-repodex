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

## 2. architecture_recommendation

```text
CONDITIONAL_GO
```

## 3. Exact TASK 1 commit validated

```text
4a6e8e3fe88be068932279b3bf896c731c812859
Gate Unix-only tests and correct overstated TASK 1 claims
```

## 4. Final TASK 2 commit SHA

```text
see the commit that introduced this file; recorded in the follow-up commit that
updates this line, and in the TASK 2 completion message
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

Regions were **frozen before inspecting RepoDex output** (`audit/frozen-regions.json`),
selected by a rule using only line counts and path names. Per region:

```text
precision  every emitted fact's range must exist in the source and have the
           recorded (row, column) equal the true byte position
recall     occurrences enumerated independently by regex/text search must not
           include anything RepoDex did not predict (lower bound on recall)
```

## 10. Extraction results per language

```text
language  file                         decl pred/mismatch/indep   import pred/indep   call pred/mismatch   test  not_predicted
rust      tokio benches/copy.rs         31 / 0 / 25                 7 / 7              81 / 0             0/0   0
rust      tokio rt_multi_threaded.rs    19 / 0 / 19                 7 / 7             126 / 0             0/0   0
go        gpt-tunnel main.go            26 / 0 / 20                 1 / 1             113 / 0             0/0   0
go        gpt-tunnel ..._test.go       18 / 0 /  2                 1 / 1             102 / 0             1/0   0
python    django apps/config.py         13 / 0 / 11                 7 / 7              55 / 0             0/0   0
python    django __init__.py            36 / 0 / 27                11 /11              89 / 0             0/0   0
php       laravel Gate.php              54 / 0 / 43                14 /15             167 / 0             0/0   0
php       laravel AuthManager.php       25 / 0 / 19                 7 / 8              45 / 0             0/0   0
```

```text
range mismatches (false-positive ranges):  0 of 1,024 audited facts
independently-found misses (false negatives): 0
sample size: 8 files (2 per language), 1,024 facts
```

Two apparent import-count differences (PHP) and one test-candidate difference
(Go) were investigated and are artifacts of the independent checks, not adapter
defects: F004 (trait `use` is not an import) and F005 (`_test.go` filename
evidence is legitimate).

**Honest limitation.** A full TP/FP/FN confusion matrix against hand-annotated
ground truth was **not** computed. What is measured is: every emitted range is
source-exact (no malformed ranges) and the independent enumeration found nothing
RepoDex missed. Numeric precision/recall per category against a labelled corpus
is NOT MEASURED (see §25).

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
(MEDIUM, model, carried from TASK 1), and INFO items F004–F007.

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
`examples/retention.rs`, `tests/task2_real_incremental.rs`.

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
* numeric TP/FP/FN precision/recall per category against hand-annotated ground
  truth — only source-exact ranges and a zero-miss independent enumeration were
  measured
* native Windows execution (cross-target type-check only)
* multi-threaded / parallel scan throughput (scans were single-threaded by design)
* generated-source effects measured separately
* cross-file or semantic resolution (out of scope for TASK 2)
* long-duration soak / memory-growth-over-time behaviour
* very large single files beyond the raised limit
* the old Go RepoDex comparison (see §26)
```

## 26. Optional old RepoDex comparison

```text
NOT PERFORMED
```

The old Go RepoDex implementation was included as a Go corpus for parsing, but no
behavioural or performance comparison against it was run, and none is claimed.
`task2.md` marks this comparison optional.

## 27. Exact evidence behind the architecture recommendation

`CONDITIONAL_GO` rests on:

```text
* 0 parser failures, 0 extraction failures, 0 read failures across 19,165 files
* recovery 18 / 11,054 = 0.163%, every recovery explained
* extraction audit: 0 range mismatches and 0 independent misses in every region
* 0 bad ranges over 320 real files (36,058 ranges) and 26 CRLF conversions
* canonical digest + complete-fact signature identical across runs and roots
* incremental result equivalent to fresh parse on 12 real files
* one genuine defect (F003) found, fixed, regression-tested, re-verified to 0
```

The conditionality is the quantified Rust grammar boundaries (F001, F002) and
the two TASK 3 design inputs (F006 tree retention, F007 whole-file extraction).
None is a foundation blocker; each constrains TASK 3. Hence `CONDITIONAL_GO`,
not `GO`.

## 28. TASK 3 readiness

TASK 3 can proceed under the constraints above: do not retain a tree per file
for the whole repository (F006); do not assume incremental extraction is cheap
(F007); keep Rust macro recall boundaries visible (F001, F002); keep recovery and
grammar limitations explicit in any new layer.

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
  audit/      frozen-regions.json  extraction-audit.json
~/reposuite/repodex/benchmarks/task2-validation-20260917T101413Z-synthetic/
  results.txt  results.json
```

Large machine-generated benchmark output is not committed; the small, stable
manifest (`benchmarks/corpora.json`) and the validation script
(`scripts/task2_range_validation.py`) are.
