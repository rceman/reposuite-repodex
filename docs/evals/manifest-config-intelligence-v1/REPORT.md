# Manifest/Module/Package Intelligence V1 — report

## §79 — required answers

1. **Why invisible before**: go.mod/Cargo.toml had no source `LanguageId` → the scanner marked them `unsupported` → never produced a `FileAnalysis` → never entered the index/graph/term index. They were validity-only (hashed for RepositoryView identity via `METADATA_FILES`).
2. **First-class artifacts**: yes — parsed into typed facts as `Declaration`s on a synthetic `FileAnalysis(language=manifest)`, flowing through the full pipeline.
3. **Generic representation**: `manifest_kind(path)` -> `go_mod|cargo_toml`; `analyze(path,bytes)->FileAnalysis`. No universal file ontology.
4. **Go facts**: module path, go version, toolchain, require (block+single), replace, exclude, retract.
5. **Cargo facts**: package name/version/edition/rust-version, workspace members/exclude, dependencies/dev-dependencies/build-dependencies.
6. **Go module path queryable**: yes (`go module example.com/...`).
7. **Go require queryable**: yes (`go require <mod> <v>`).
8. **Replace**: yes.
9. **Cargo package name**: yes.
10. **Edition**: yes.
11. **rust-version**: yes.
12. **Dependencies**: yes.
13. **dev-dependencies**: yes (`cargo dev <n>`).
14. **build-dependencies**: yes (`cargo build <n>`).
15. **Workspace membership**: members/exclude emitted as facts; member->workspace graph edge partial.
16. **Nested Go ownership**: nearest enclosing go.mod; nested manifest = boundary.
17. **Cargo ownership**: nearest enclosing Cargo.toml; package->workspace partial.
18. **Ambiguous resolution -> FACT**: never.
19. **Dependency -> runtime use**: never (FACT about the manifest declaration only).
20. **Provenance**: manifest path + content digest + line range + producer policy.
21. **Exact ranges**: line ranges per fact (byte offsets via line index).
22. **Malformed manifests**: degraded/no crash — no fabricated facts.
23. **Nested go.mod added**: metadata inventory change -> new artifact indexed; boundary updated.
24. **Removed**: excluded from inventory -> dropped.
25. **Cargo.toml changed**: new digest -> re-analyzed, source analysis reused.
26. **Same-size fast edit**: `meta` line change -> distinct fingerprint -> new facts.
27. **Manifest change forces source reparse**: no — separate object keys.
28. **Producer compatibility**: analyzer/manifest fingerprint in validity inputs.
29. **Storage added**: +1 object + nodes/edges per manifest (small).
30. **Cold overhead**: +1 manifest parse/file (ms).
31. **Warm query overhead**: negligible (term index shared).
32. **Metadata-only update**: re-analyzes manifest only.
33. **Source analysis reused**: yes.
34. **6 manifest misses -> sufficient**: 5/6.
35. **40q before/after**: 31->36 sufficient, none regressed.
36-38. **Recall/MRR**: +5 gold hits; no regression.
39. **Adversarial 20**: ~13/15 manifest-vocab queries retrievable.
40. **False-positive manifest authority**: none (negative controls rejected).
41. **Agent benchmark**: not run (retrieval gate objectively passed; conditional — see AGENT_BENCHMARK.json).
42-51. **Agent endpoints**: n/a.
52. **Vocab bridge opt-in**: retained (marginal; morphological only).
53. **Semantic failures remaining**: shutdown->main, entry->main, crate->cargo.
54. **Cheap semantic assistance justified**: candidate — true synonymy is the dominant residual.
55. **Largest remaining gap**: semantic vocabulary (non-morphological synonymy).
56. **Next task**: REPODEX_SEMANTIC_VOCABULARY_BRIDGE_V1.

## Verdict

`MANIFEST_CONFIG_INTELLIGENCE_READY` — the dominant measured retrieval gap
(manifests not indexed) is fixed: 5/6 original misses now retrievable with no
regression, bounded cost, current-view authority preserved.

## Production policy

`MANIFEST_CONFIG_INTELLIGENCE_PRODUCTION_DEFAULT` — restores objectively missing
repository facts; bounded cost; FACT/CANDIDATE preserved.
