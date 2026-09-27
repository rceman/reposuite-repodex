# Claim -> evidence

| claim | evidence |
|---|---|
| zero tool rejections/cancellations | TOOL_PREFLIGHT.json + VALIDATION_RESULTS.json (rejected=0 all) |
| code tasks validate from worktree | VALIDATION_RESULTS.json (file_check) + code-add/code-comment traces |
| full tool results preserved | traces/*.json TOOL_RESULT events with content |
| untruncated messages | traces/*.json MODEL_VISIBLE_RESPONSE |
| per-call token accounting | traces/*.json MODEL_TOKENS |
| correct model/tool counts | SESSION_METRICS.csv |
| repodex calls counted from actual requests | SESSION_METRICS.csv repodex_calls |
