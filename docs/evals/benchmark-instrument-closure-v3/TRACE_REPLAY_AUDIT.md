# Trace replay audit
Each trace: execution_status + validation_status, executor_mode, full events
(untruncated messages, TOOL_REQUEST args, TOOL_RESULT content + transport status
+ process exit, MODEL_TOKENS), repodex/agent/combined wall, worktree proof
(diff digest, changed paths, validator exit code). Reconstructable from Git.
