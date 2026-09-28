# Instrument Hotfix V3.1

- **Classifier bug**: `reposuite-repodex in cmd` misclassified `find`/`ls`/`cat`
  on the fixture path `/home/.../reposuite-repodex/tools/eval/fixtures/*` as a
  repodex invocation. 7 V3 sessions affected (old repodex_calls=7).
- **Fix**: `_is_repodex_invocation` identifies the executed program token —
  basename `reposuite-repodex` invoked with a subcommand, `env VAR=val`
  wrappers skipped. Path mentions no longer count. Corrected repodex_calls=0;
  no agent genuinely invoked repodex; S0 false repodex = 0.
- **`--from-traces` reclassification**: re-derives tool categories + model_calls
  from raw TOOL_REQUEST/MODEL_TOKENS events instead of trusting stored values.
- **allowed_changed_paths**: mutating tasks now fail if an unrelated file is
  modified; changed_paths normalized to repo-relative.
- **preflight**: fixture build must return exit 0.
- **repodex prepare**: failed precompute -> INFRA_INVALID, session blocked.
- No Agent sessions rerun (correction derived from committed raw traces).
- `PRODUCTION_REPODEX_BEHAVIOR_CHANGED=false`.
