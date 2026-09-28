# Claim -> evidence
| claim | evidence |
|---|---|
| R2 actually calls repodex (interactive) | repodex_calls>0 in SESSION_METRICS + TRACE_INDEX |
| 94% first-query adherence | DISCOVERY_COVERAGE numerator/denominator |
| R2 preserves correctness vs N0/P1 | PER_ARM success |
| callers emit call edges not containment | tests/adaptive_nav.rs + RELATION_SEMANTICS |
| false-truncation impossible | tests + PACKET_METRICS.FALSE_TRUNCATION_SIGNALS=0 |
| byte budget enforced | adaptive.rs budget + tests |
