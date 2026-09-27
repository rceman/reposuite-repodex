# Trace replay audit

Each committed traces/<session>.json contains: session metadata, the task text,
executor_mode, final_metrics, per-step events (MODEL_VISIBLE_RESPONSE,
TOOL_REQUEST with full args, TOOL_RESULT with full content, MODEL_TOKENS),
validation record, session_status. A reviewer can reconstruct the observable
session end-to-end: user task -> tool requests+results -> final answer ->
validation. No hidden reasoning. Results are not truncated; large payloads are
preserved in the event content.
