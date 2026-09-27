# Sanitization

Stripped: `reasoning_content` (hidden CoT) and any field name matching
token/bearer/apikey/authorization/password/secret. Preserved: full tool
arguments, full tool results, RDX packets, source excerpts, timestamps, metrics.
NORMAL_TOOL_RESULTS_PRESERVED=true, AGENT_VISIBLE_MESSAGES_UNTRUNCATED=true,
HIDDEN_CHAIN_OF_THOUGHT_CAPTURED=false, SECRETS_COMMITTED=false.
