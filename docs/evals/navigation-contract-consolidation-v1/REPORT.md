# Navigation Contract Consolidation V1 — final report

## Query contract answers
1. Yes — adaptive free-text previously ran generic Find then labeled the packet.
2. question+options -> adaptive::plan -> typed QueryIntent -> engine -> projection -> adaptive RDX.
3. Yes — explicit --intent/--target/--to always win (plan takes them first).
4. classify() NavIntent maps Callers->Callers, Callees->Callees, PathOrFlow->Paths, Relationship->Related, else Find.
5. Anchors = deterministic identifier-token extraction minus fixed stopword list; only feed typed ops.
6. Zero anchors for a typed intent -> honest Find fallback (no fabricated operation).
7. Multiple anchors: first->target, second->to for paths; engine-side unique-anchor resolution governs 0/1/>1 semantics (explicit no-result/ambiguity, never first-guess).
8. Yes — "What does ResolveKey call?" emits a real call_candidate edge (traces/).
9. Yes — "Who calls Encode?" executes callers machinery (traces/).
10. Yes — "How does engine reach Encode?" executes bounded path machinery -> honest NO_ROUTE bounded.
11. No — the nav label now comes from the same plan that drove retrieval; it cannot relabel Find output.

## Service parity answers
1. Previously dropped: intent, target, to, depth, mode/exhaustive, budgets, all policies, adaptive profile.
2. Now forwarded verbatim: intent, target, to, depth, max_results, memory_mode, context_policy, recipes, utility_policy, source_witness, vocab_bridge, vocab_native.
3-8. Yes — all preserved, verified byte-identical.
9. No — auto routing is byte-identical to direct/service (5-query live check).
10. Options accepted publicly are all forwarded; nothing is silently dropped (all supported policies covered).
11. Yes — EvidenceProjection round-trips (Deserialize) and is rendered by the identical code path.

## Adaptive packet answers
1. `out[..byte_budget]` sliced by raw byte index — a multibyte scalar crossing the boundary panicked; gaps/summary appended after the cut could also overflow the bound.
2. Yes — record-aware emission, no byte slicing anywhere.
3. Yes — trailer space reserved before body fill; S line emitted inside the bound.
4. Yes — gaps+summary inside the bound by construction.
5. No — whole records only.
6. Yes — R emits only when both endpoint F ids emitted; validator + test + 77 live packets: 0 dangling.
7. Yes — S counts emitted records; bytes= is a fixpoint equal to actual size (validate_packet).
8. Yes — complete reflects seed_selection_complete AND no relevant-evidence budget drop.
9. Yes — RESULT_LIMIT (bound) vs AMBIGUOUS_RESULT (identity) are distinct gaps.
10. Yes — UPSTREAM_TRUNCATED gap carries proj.truncated_reason verbatim.
11. Max observed (77 live packets): locate 693, definition 726, callers 381, callees 533, related 274, path_or_flow 645, manifest_or_config 2076, test_evidence 1905, ambiguous 1641 — all within envelopes.
12. 0.
13. 0.

## Benchmark answers (144 sessions, 0 infra-invalid)
1. ResolveKey gold was already source-backed (`enc := util.Encode(k)`); the OLD packet hid it via generic-Find — now correct mechanically.
2. Source shows ResolveKey calls util.Encode — confirmed.
3. Yes — direction-aware written_call/relation_direction obligations strengthened (cc cohort).
4. Yes — `reposuite dex`/`dex` classified as repodex usage.
5. Yes — exec commands decomposed per-segment.
6. N0: 46/48 (96%). 7. F1: 48/48 (100%). 8. A2: 47/48 (98%).
9. Cohorts: lk/cd/mc all 12/12 every arm; cc 10-12/12.
10. First-discovery adherence: F1 48/48, A2 48/48 repodex-first.
11. Native discovery ops: N0 116 enum+search (+108 source reads); F1 24; A2 32.
12. Source verification reads: N0 108, F1 108, A2 93.
13. Model calls: ~all sessions had model calls; see TOKEN_ACCOUNTING.
14. Tool calls: per SESSION_METRICS.csv.
15. Avg input tokens: N0 55.9k, F1 69.8k, A2 71.5k (per §50, interactive overhead dominates — not a packet-bytes proxy).
16. Avg wall: N0 28.0s, F1 33.8s, A2 36.5s.
17. Packet bytes: PACKET_METRICS max_by_intent (all within envelopes).
18. 0 candidate/fact violations. 19. 0 direct/service mismatches. 20. 0 infra-invalid.

## Product answers
1. Yes — adaptive RDX is safe as the default Agent profile.
2. Yes — full RDX remains the explicit detailed/debug/research profile.
3. Yes — canonical JSON is machine/internal.
4. Yes — one generic query surface remains appropriate.
5. Yes — transport-independent.
6. Yes — production core ready (PHP parity not a core requirement).
7. Largest remaining gap: PHP navigation (call candidates/composer) — honestly incomplete today.
8. Yes — PHP navigation extension is the next milestone.

## Verdicts
REPODEX_NAVIGATION_CONTRACT_CONFIRMED
ADAPTIVE_RDX_PRODUCTION_PROFILE_CONFIRMED
REPODEX_PRODUCTION_CORE_READY

Next milestone: PHP navigation extension (call candidates + package topology).
