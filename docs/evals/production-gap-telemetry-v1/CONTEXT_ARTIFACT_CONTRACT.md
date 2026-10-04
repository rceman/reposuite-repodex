# context_artifact_presented — extended contract

Additive v1 fields (old streams remain valid):

| field | req | meaning |
|---|---|---|
| artifact_id | yes | stable identity (digest) |
| artifact_kind | yes | e.g. `rdx1_packet` |
| producer | yes | e.g. `repodex` |
| tool_call_id | new | correlates to `tool_call_started` — RepoDex evidence is identified by this correlation, never by shell-command strings |
| presentation | new | profile, e.g. `adaptive` |
| content_digest / content_bytes | — | payload digest/size; payload itself NEVER embedded |
| presented_at | — | optional; `sequence` is order authority |
| references | — | structured repo references |
| metadata | new | allowlisted structured telemetry (below) |

`metadata` allowlist: `query_intent`, `navigation_profile`,
`evidence_complete`, `gap_signatures[]={family,reason_code}`, `seed_count`,
`candidate_count`, `fact_count`, `relation_count`, `packet_bytes`,
`query_digest`. Forbidden: secrets, auth headers, env values, raw query/task
text dumps, full artifact payloads.
