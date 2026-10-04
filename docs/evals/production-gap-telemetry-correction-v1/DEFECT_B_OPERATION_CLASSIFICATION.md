# Defect B — fallback classifier was too coarse

## Root cause

`is_native_discovery` accepted only `Search`/`DirectoryList`; every
uncorrelated `Shell`/`OtherRepositoryTool` call became task action —
misclassifying `rg`/`grep`/`find` executed through a shell, and any
repository-search tool behind a generic category.

## Fix

- `ToolCallStarted.repository_operation` — optional, additive, producer-
  normalized observation (not judgment): `discovery_search`,
  `discovery_list`, `source_read`, `edit`, `build`, `test`, `runtime`,
  `git_inspection`, `other`. The EMITTER (Relay/runtime adapter) knows what
  the Agent actually did; RepoDex consumes the normalized value.
- Legacy streams without it: `Search`/`DirectoryList` -> discovery,
  `FileRead` -> verification, `Shell`/`OtherRepositoryTool`/
  `GitInspection`/`NonRepositoryTool` -> `unclassified`, never task action.
- New disposition `unclassified_repository_activity`; unclassified events do
  NOT end the window — a later discovery can still resolve it.
- Unknown future operation codes -> `unclassified`, never guessed.
- Producer-correlated calls (`tool_call_id`) stay excluded regardless of op.

## Precedence (documented + tested)

discovery > another_artifact > verification > task_action > unclassified >
no_followup. `verified_before` preserves read-before detail.
