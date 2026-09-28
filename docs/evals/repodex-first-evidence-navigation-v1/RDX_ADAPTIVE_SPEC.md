# Adaptive RDX (agent-facing, #RDX1 adaptive v2)
Q query | I intent | F <id> <kind> <key> <label> | D <id> rank= path= decl= body= |
R <kind> <from> <to> <f|c> [cs=] [rule=] | P route | G gap | S summary

Semantic dedup: one F line per node identity (seed kind wins over a node alias);
relations deduped by kind+endpoints+rule+candidate_set. Gaps are explicit:
NO_RESULT / AMBIGUOUS_RESULT / TRUNCATED_CONTINUATION. FACT/CANDIDATE preserved.
