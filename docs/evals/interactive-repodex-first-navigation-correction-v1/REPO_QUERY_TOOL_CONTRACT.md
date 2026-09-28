# repo_query contract (benchmark mirrors future Gateway)
repo_query(question) -> executes `reposuite-repodex query --root <bound-frozen-
root> --nav adaptive --query <q>` -> returns adaptive RDX only. No JSON.
Root bound externally by the harness (Gateway will own it). The benchmark shim is
a `./repo_query` executable in the worktree; counted as a repodex call.
