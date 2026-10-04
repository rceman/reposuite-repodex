# NativeFallbackAssociation semantics

ASSOCIATION, not causation. For each `context_artifact_presented` at sequence
S in one session, scan forward ≤64 events until the earliest of: next
`context_artifact_presented`, an edit/write/build/test/runtime tool action
(Shell/other/uncorrelated repo tool), session end.

Window classification (precedence order):

1. `native_discovery_after_artifact` — any Search / DirectoryList /
   uncorrelated repository tool in window
2. `another_context_artifact` — a further artifact presentation ended the window
3. `source_verification_only` — only file_read/source_observed events
4. `task_action_started` — a task action ended the window
5. `no_followup_discovery` — nothing else

`verified_before` records whether exact reads preceded the outcome.

Boundaries: session-local and sequence-local only — `investigation_id`
never merges windows; `repository_id`/`repo_head` are carried, not merged.
The producer-correlated tool call (artifact.tool_call_id) never counts as
native discovery.
