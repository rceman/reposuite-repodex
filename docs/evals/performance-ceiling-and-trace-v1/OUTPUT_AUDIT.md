# Agent-facing output audit

The machine JSON response (`--json`) averages ~79KB (~19.8k est tokens) —
92% is the `related` edge dump and 31% is duplicated strings
(`repodex.callable_contains` x3224, `repodex.containment` x2683,
`file:src/cli.rs` x457). Field-name/syntax overhead is large.

The canonical AGENT-facing format is already `to_rdx()` — a compact
line-oriented protocol (~22KB mean, ~5.5k tokens). RDX is **72% smaller** than
the JSON response for the same evidence. Delivering RDX instead of JSON to the
Agent removes ~57KB (~14k est tokens) per packet at equal evidence content.
