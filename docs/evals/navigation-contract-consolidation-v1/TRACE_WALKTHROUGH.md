# Trace walkthrough

- cc-what-call.A2.r1: `repo_query` first (repodex op) → `call_candidate` edge returned with cs=+rule= → two source reads verify → TASK_SUCCESS. Textbook RepoDex-first.
- cc-reach.A2.r1: path question -> bounded path machinery -> honest NO_ROUTE bounded rather than a mislabeled neighborhood.
- cc-who-calls.A2.r1: "Who calls Cache.Get?" -> real callers query -> empty result -> agent correctly reports "No code calls Cache.Get" (bounded absence, honestly reported).
