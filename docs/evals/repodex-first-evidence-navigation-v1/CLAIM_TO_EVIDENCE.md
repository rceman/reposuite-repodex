# Claim -> evidence
| claim | evidence |
|---|---|
| A2 correctness preserved (52 >= N0 46) | PER_ARM.json + SESSION_METRICS.csv |
| A2 packet 90% smaller than F1 | PACKET_METRICS.json + PER_ARM.packet_bytes |
| A2 reduces tools (-20%) + searches (-58%) vs N0 | PER_ARM + PAIRED_DELTAS |
| intent classification deterministic | tests/adaptive_nav.rs + adaptive.rs |
| test-candidates not mislabeled callees | src/recipe/exec.rs + is_test_shaped |
| code-cohort repodex advantage (N0 5/15 -> 12/15) | PER_COHORT.json cd row |
| FACT/CANDIDATE preserved | traces + adaptive_rdx ev marker |
| CLI/service parity | same engine/projection for both paths |
