# Telemetry architecture

```text
query execution
      ↓
canonical EvidenceProjection
      ↓                                   ↓
adaptive/full rendering            RepoQueryObservation
(query::adaptive::adaptive_rdx     (query::observation::observe
 _observed → packet +              → digest/bytes/counts/
  AdaptiveRendered accounting)      gap_signatures/dispositions)
      ↓                                   ↓
Agent-visible RDX          --emit-observation → obs-*.json
(stdout, unchanged)        (future Gateway/Relay handoff)
```

One execution, one truth. The observation is built from the projection and
renderer accounting — never reparsed from RDX text.

Future flow: Gateway (query authority) runs the RepoDex query, gets packet +
observation, hands both to the Agent runtime; Relay observes
`tool_call_started`/`tool_call_completed`/`source_observed` and emits
`context_artifact_presented` carrying the observation's allowlisted metadata.
Relay never parses RDX.
