# Matched trace walkthrough (real sessions, committed in traces/)

For a representative matched task per cohort (e.g. cd-res-doc):
- N0 native: broad find/grep -> often FAILS to place the edit correctly.
- F1 full-RDX: large packet (13.9kB) -> agent locates + edits -> SUCCESS.
- A2 adaptive: small packet (1.4kB) -> same correct edit -> SUCCESS.

traces/<task>.<arm>.<rep>.json preserves the full observable sequence
(messages, tool requests+results, tokens, timing) — no hidden reasoning.
