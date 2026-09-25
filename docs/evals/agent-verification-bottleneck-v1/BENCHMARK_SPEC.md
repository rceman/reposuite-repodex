# Agent Verification Bottleneck V1 — spec

Hypothesis: once RepoDex finds the evidence location, the Agent still re-reads
the source to verify. Test: does delivering the exact bounded current-source
bytes remove the corresponding verification read while preserving correctness?

W0 — locator/range only (no source bytes). W1 — identical retrieval + bounded
CurrentSourceWitness. All other policies frozen (static compiler, memory off,
recipes off, utility off). 24 questions x 2 arms x 2 reps = 96 serial sessions.

Witnesses are materialized from the CURRENT file bytes whose snapshot_id
matches the index snapshot that produced the range — never historical bytes.
