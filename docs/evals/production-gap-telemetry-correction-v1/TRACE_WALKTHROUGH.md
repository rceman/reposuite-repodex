# Walkthrough

- `obs-phpnav-1` (fixtures/observed-stream.jsonl): `tool_call_started tc-q` ->
  `context_artifact_presented` with REAL observation metadata
  (`out_of_scope:late_static_binding`, `no_candidate:*`) -> `fu-1 grep
  discovery_search` -> report bucket `native_discovery_after_artifact`,
  reason bucket `out_of_scope:late_static_binding` presentations=1.
- `obs-phpinh-3`: shell op `test` -> `task_action_started` (definite stopper,
  NOT discovery).
- `obs-phpnav-5`/`obs-phprx-4`: legacy shell, no op -> `unclassified_repository_activity`,
  not task action.
- 929-session trace replay: counts reconcile; 615 control sessions yield 0
  false associations.
