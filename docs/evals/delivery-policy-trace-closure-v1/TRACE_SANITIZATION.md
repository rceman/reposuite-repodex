# Trace sanitization

Per-session trace files under traces/ were produced from the raw Agent export
objects by stripping hidden reasoning (`reasoning_content` fields) and any
field whose name matches token/bearer/apikey/authorization/password/secret.
Messages are capped at 2000 chars. No source paths, tool arguments, RDX output,
metrics, or task IDs were removed. HIDDEN_CHAIN_OF_THOUGHT_CAPTURED=false.
No secrets are present in the committed traces.
