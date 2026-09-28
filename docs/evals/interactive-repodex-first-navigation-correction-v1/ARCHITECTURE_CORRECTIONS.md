# Adaptive corrections
- NavIntent::Ambiguous reachable (no cue + not structural) -> explicit gap.
- Fake-regex cues ("does .* call") removed; explicit phrase logic.
- Precedence: TestEvidence > Callers > PathOrFlow > Relationship > Manifest >
  Callees > Definition > Locate > GeneralStructural > Ambiguous. A bare "call"
  no longer steals Test/Relationship/Callers.
- Callers/Callees require call-evidence kinds (call_candidate/call) + correct
  direction; structural edges (contains/member_of/owned_by_manifest) rejected.
- ManifestConfig filters real canonical kinds; only real truncation -> gap;
  serialized byte budget enforced with no mid-line cuts.
