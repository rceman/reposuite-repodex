# RepositoryView Dependency-Validity Benchmark Spec

Correctness/foundation benchmark. Verifies the invariant:

> A RepositoryView cache hit may be reused ONLY when all inputs that produced
> the reusable artifacts still describe the exact target view and producer
> contract.

## Oracle

`warm/reused result` ≡ `independent fresh rebuild of the same captured view`
(semantic equivalence for deterministic RepoDex results).

## Validity inputs (the gate)

`validity_key = hash( content_fingerprint(source+metadata), analyzer_fingerprint,
snapshot_config_digest, link_fingerprint, candidate_fingerprint, graph_fingerprint )`

The index directory is keyed on `validity_key` and a `validity.json` records the
components; a directory hit implies all inputs matched.

## Fixture matrix (see VALIDITY_MATRIX.json)

A unchanged / B source change / C same-size dirty edit / D metadata-only /
E nested metadata add / F metadata removal / G analyzer mismatch / H config
mismatch / I link / J candidate / K graph policy / L mid-build capture / M
identical-input cross-worktree.

## Producer/config identities validated

AnalyzerFingerprint (schema+ABI+grammar versions), SnapshotConfig
(max_file_size/gitignore/policy/schema), LinkFingerprint, CandidateFingerprint,
GraphFingerprint.

## Performance

See PERFORMANCE.json. Correctness is not traded for a microbenchmark; the
unchanged fast path is stat-only.
