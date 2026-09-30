# Transport parity (defect B)

query --direct, --service, and auto-routed query execute the identical
normalized request. The service forwarder now sends:
  intent, target, to, depth, max_results, memory_mode, context_policy,
  recipes, utility_policy, source_witness, vocab_bridge, vocab_native
verbatim (defaults off/static). With --nav adaptive the client plans, forwards
the resolved typed intent+anchors, and renders the service's canonical
EvidenceProjection through the identical adaptive_rdx code path —
byte-identical output verified live (direct==service==auto).
