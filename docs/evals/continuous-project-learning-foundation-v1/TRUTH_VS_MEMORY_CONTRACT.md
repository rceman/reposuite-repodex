# Plane A (current truth) vs Plane B (learned navigation knowledge)
Memory stores HOW to investigate (families, anchors, routes, dependencies) —
never a natural-language answer as current truth. rebind_memory() returns
CurrentValid/Partial/Stale/Ambiguous/Missing/Unsupported; only CurrentValid is
usable, and even then it supplies a route hint, not a FACT. Memory cannot
upgrade a current CANDIDATE/UNKNOWN/STALE to FACT (§2-§5). 
