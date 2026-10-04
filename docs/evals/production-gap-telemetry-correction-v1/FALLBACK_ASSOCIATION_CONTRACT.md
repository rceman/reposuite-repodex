# Fallback association contract (corrected)

Window: from `context_artifact_presented` at sequence S, forward <=64 events.
Ordering: `sequence` only. Sessions analyzed per-file; repository_id/repo_head
carried, never merged. Association is observational, NOT causal.

Window events:
- context_artifact_presented -> stop (another_artifact)
- definite task action (op edit/build/test/runtime) -> stop (task_action)
- discovery -> mark, continue
- verification -> mark, continue
- unclassified -> mark, continue (never ends the window)
- producer-correlated tool_call_id -> skipped entirely

Precedence: discovery > another_artifact > verification > task_action >
unclassified > no_followup; `verified_before` keeps read-before detail.

Reconciliation (required top-level):
presentations_with_gap + presentations_without_gap = artifact_presentations;
sum of the six dispositions = artifact_presentations;
by_gap_family sums may exceed presentations (families overlap).
