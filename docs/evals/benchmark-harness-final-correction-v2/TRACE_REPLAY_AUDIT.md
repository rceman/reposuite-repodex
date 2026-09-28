# Trace replay audit
Each traces/<session>.json: execution_status + validation_status (separate),
executor_mode, per-step events (MODEL_VISIBLE_RESPONSE untruncated, TOOL_REQUEST
full args + category, TOOL_RESULT full content + transport status + process exit,
MODEL_TOKENS), repodex_prepare_wall_ms, agent/combined wall, validation (obligations
+ worktree proof + validator build/test exit code). Reconstructable end-to-end.
