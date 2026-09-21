# TASK 4B — Results

Go local-binding facts on `gohugoio/hugo` (912 Go files).

## Emitted binding counts by kind

```text
short_variable                  18,687
function_parameter               8,264
method_receiver                  4,130
function_literal_parameter       3,060
range_variable                   2,685
variable                         1,869
function_result                    251
constant                           225
type_switch_variable               143
function_literal_result             60
local_type                          54
select_receive_variable             15
type_parameter                      32
─────────────────────────────────────────
total                           39,475
```

All 13 Go kinds emitted; every kind present in Hugo.

## Independent audit (Hugo)

`task4b_audit` re-derives expected bindings via its own Tree-sitter traversal +
block-scope tracking (no production binding code):

```text
emitted bindings validated: 39,475
TP (binding == expected intro):       39,475
FP (emitted, no expected intro):           0
FN (expected intro, nothing emitted):      0
range/source errors:                       0
over-capture (`_`/invalid/package-level):  0
```

## Plain-name call projection (post-binding)

```text
plain_name calls:                       11,134   (was 10,608; +526 — the
                                                   extraction-gap fix reached
                                                   func literals in decl values)
blocked by a same-name local binding:    1,556
  short_variable                          1,197
  function_parameter                        179
  function_literal_parameter                 84
  variable                                   45
  range_variable                             28
  method_receiver                            16
  local_type                                  3
  type_switch_variable                        2
  type_parameter                              2
unblocked:                                 9,578
  unblocked same-package func candidates:
    exactly one                            4,143
    zero                                   5,435
    multiple                                   0
```

The 1,556 blocked calls are precisely the false-candidate risk TASK 4B exists to
eliminate — a `helper := func(){}`-style local now provably shadows the package
function at the call site.

## Snapshot size

```text
before TASK 4B:   37,008,541 B
after  TASK 4B:   59,285,649 B
added:           +22,277,108 B  (+60.2%)
```

## Performance

```text
fresh snapshot build:  ~2.4 s   (912 files, all reparsed)
peak RSS:              ~12 MB
```

## Verification

```text
cargo fmt --all -- --check            PASS
cargo check --locked                  PASS
cargo test --locked                   499 passed / 0 failed
cargo clippy --locked ... -D warnings PASS
cargo build --locked --release        PASS
incremental-vs-fresh                  PASS (rename/insert/delete/var→:=)
snapshot round-trip                   PASS
snapshot update-vs-fresh              PASS
Hugo independent audit                PASS (TP=39475 FP=0 FN=0)
Rust regressions                      PASS (local_bindings 29, target_path_v5 27,
                                            local_shadow_v2 17)
stale schema-2 snapshot               rejected (missing `bindings`)
stale T4A link artifact               rejected (FingerprintMismatch)
```
