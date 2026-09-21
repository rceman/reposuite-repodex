# TASK 4C — Results

Go package-local plain-name call candidates on `gohugoio/hugo`.

## Outcome distribution (11,134 plain_name calls)

```text
single_candidate         3,918
multiple_candidates          3
no_candidate             7,213
out_of_scope            41,245   (non-plain_name Go calls: member_selector,
                                 type_conversion, indirect, qualified)
───────────────────────────────
total Go records        52,379
```

## NoCandidate reasons

```text
no_package_function                5,222
shadowed_by_local_binding          1,556
package_namespace_ambiguous          413
dot_import_namespace_uncertain        22
```

## §36 reconciliation — T4B projection vs actual

T4B projected 4,143 unblocked calls with a unique package function:

```text
still single_candidate                    3,918
became multiple_candidates                    3   (same-name func variants)
suppressed: package_namespace_ambiguous     211   (func + same-name non-func decl)
suppressed: dot_import_namespace_uncertain   11
                                          ─────
                                            4,143   ✓ exact
```

## §37 local-binding blockers by kind

```text
short_variable            1,197
function_parameter          179
function_literal_parameter   84
variable                     45
range_variable               28
method_receiver              16
local_type                    3
type_switch_variable          2
type_parameter                2
─────────────────────────────
total                     1,556   (reconciles with TASK 4B exactly)
```

## §42 external-test packages

```text
plain_name calls in *_test packages: 746
  single_candidate                    197   (within foo_test itself)
  shadowed_by_local_binding           341
  no_package_function                 185
  dot_import_namespace_uncertain       22
  package_namespace_ambiguous           1
ordinary-package leak count             0   ✓
```

## §43 package main

```text
plain_name calls in package main:    120
  single_candidate                    73
  no_candidate                        47
```

## §38–41 independent audit

`scripts/task4c_audit.py` independently re-derives each plain-name call's
outcome from persisted facts + `go_package` topology + bindings + package
declarations — no production candidate code. Audited **all 11,134** calls:

```text
CORRECT_SINGLE_PACKAGE_FUNCTION       3,918
CORRECT_NO_CANDIDATE                  6,800
PACKAGE_NAMESPACE_AMBIGUITY_CORRECT     413
CORRECT_MULTIPLE_PACKAGE_FUNCTION         3
─────────────────────────────────────────
FALSE_PACKAGE_FUNCTION_CANDIDATE          0
WRONG_PACKAGE_FUNCTION_TARGET             0
CROSS_PACKAGE_LEAK                        0
LOCAL_SHADOWING_ERROR                     0
IMPORT_SHADOWING_ERROR                    0
MISSING_PACKAGE_FUNCTION_CANDIDATE        0
```

## §48 package-qualified projection (measurement only)

```text
member_selector calls                       40,234
  selector root is an import name           13,352
    resolves to a repo-local package         3,683
    resolves to an external package          9,669
  (remaining ~26,882 are receiver.method() — not package-qualified)
```

The 3,683 repo-local package-qualified calls are the population the next
bounded task (imported package-qualified function candidates) would address.

## Artifact / performance

```text
candidate artifact:   22,064,278 B   (Rust+Go combined; Hugo is all-Go)
candidate records:    52,379
build time:           ~1.9 s  (load 760 ms, derive 156 ms, serialize 53 ms, verify 800 ms)
snapshot-ratio:       0.3721
```

## Verification

```text
cargo fmt --all -- --check            PASS
cargo check --locked                  PASS
cargo test --locked                   534 passed / 0 failed
cargo clippy --locked ... -D warnings PASS
cargo build --locked --release        PASS
Go candidate fixtures                 35 pass (full §33 matrix)
independent Hugo audit                PASS (11,134 calls, 0 errors)
external-test exhaustive audit        PASS (746 calls, 0 leaks)
update-vs-fresh                       PASS
determinism / cross-root              PASS (identical digest)
stale candidate artifact              rejected (RuleFingerprintMismatch)
Rust regressions                      PASS (call_candidates 55, unchanged)
```
