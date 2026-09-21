# TASK 4D — Results

Go imported package-qualified call candidates on `gohugoio/hugo`.

## member_selector outcome distribution (40,234 calls)

```text
single_candidate          3,273
multiple_candidates          13
no_candidate                208
out_of_scope             36,740
────────────────────────────────
total                    40,234
```

## repo-local import-root calls (3,683)

```text
single_candidate                            3,273
multiple_candidates                            13
no:imported_package_namespace_ambiguous       174
no:import_root_shadowed_by_local_binding       17
out_of_scope:not_a_direct_package_selector    206   (multi-hop pkg.X.Y())
────────────────────────────────────────────────────
                                            3,683   ✓ reconciles projection
```

## out_of_scope reasons (all member_selector)

```text
selector_root_not_an_import      22,129   (receiver/value selectors: obj.M())
external_import                   9,478
not_a_direct_package_selector     5,133   (multi-hop a.b.C())
```

## §40 combined Go candidate-bearing coverage

```text
TASK 4C package-local plain_name:   3,918 single + 3 multiple   = 3,921
TASK 4D imported package selector:  3,273 single + 13 multiple  = 3,286
─────────────────────────────────────────────────────────────
combined bounded candidate-bearing Go calls                    7,207
```

## §45 alias audit

```text
aliased import items:                378
selector calls through an alias:   1,374
alias misbinding errors:               0   ✓
```

## §46 package-name-vs-path audit

```text
repo-local imports where package clause != path basename:   18
all resolve through the true clause name (0 misbindings)
```

## §47 external-test packages

```text
member_selector calls in *_test packages:   7,539
  single_candidate                          1,510   (via explicit repo-local imports)
  multiple_candidates                          11
  no_candidate                                 23
  out_of_scope                              5,995
cross-package-without-import leak             0   ✓
```

## §41–44 independent audit

`scripts/task4d_audit.py` independently re-derives every `member_selector`
call's outcome from persisted facts + import links + bindings (no production
candidate code). Audited **all 40,234** calls:

```text
CORRECT_METHOD_OR_VALUE_SELECTOR_OUT_OF_SCOPE   27,262
CORRECT_EXTERNAL_IMPORT_OUT_OF_SCOPE             9,478
CORRECT_SINGLE_IMPORTED_PACKAGE_FUNCTION         3,273
CORRECT_NO_IMPORTED_PACKAGE_CANDIDATE              208
CORRECT_MULTIPLE_IMPORTED_PACKAGE_FUNCTION          13
─────────────────────────────────────────────────────────
FALSE_IMPORTED_PACKAGE_FUNCTION_CANDIDATE            0
WRONG_IMPORTED_PACKAGE_FUNCTION_TARGET               0
IMPORT_ROOT_SHADOWING_ERROR                          0
METHOD_MISCLASSIFIED_AS_PACKAGE_CALL                 0
CROSS_MODULE_PACKAGE_LEAK                            0
MISSING_IMPORTED_PACKAGE_FUNCTION_CANDIDATE          0
```

## §52 post-TASK-4D call census (52,379 Go calls)

```text
member_selector          40,234   (3,286 now candidate-bearing via repo-local imports)
plain_name               11,134   (3,921 candidate-bearing via package-local rule)
type_conversion             622   (unresolved — T(v) / Generic[int](x) shape)
indirect                    389   (callee is an expression)
```

Remaining unsupported families: receiver/method selectors (type inference),
external-package selectors (dependency resolution), type conversions, indirect
calls, dot-imported names, package-level function-valued variables, generics.

## Artifact / performance

```text
candidate artifact:   23,117,103 B   (+1,106,825 over TASK 4C)
candidate records:    52,379
build time:           ~2.1 s  (derive 189 ms, verify 796 ms)
snapshot-ratio:       0.3898
```

## Verification

```text
cargo fmt / check / clippy -D warnings / release build   PASS
cargo test --locked                                       552 / 0 fail
TASK 4D fixtures (18 new)                                 PASS
independent member_selector audit (40,234 calls)          PASS
TASK 4C plain-name regression audit                       PASS (unchanged)
alias audit / external-test audit                         PASS (0 misbind/leak)
update-vs-fresh / determinism / cross-root                PASS
stale TASK 4C artifact                                    rejected (fingerprint)
Rust regressions                                          PASS
```
