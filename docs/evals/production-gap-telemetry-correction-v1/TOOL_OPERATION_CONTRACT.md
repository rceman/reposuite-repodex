# repository_operation contract

Optional field on `tool_call_started` (AgentEvent v1, additive — no v2).

```text
repository_operation:
    discovery_search | discovery_list | source_read |
    edit | build | test | runtime | git_inspection | other
```

Emitter responsibility (future Relay/runtime adapter): it observed the
action, it normalizes it — shell `rg foo` -> `discovery_search`,
`cargo test` -> `test`, native search API -> `discovery_search`.

RepoDex classification:

| event                              | class         |
|------------------------------------|---------------|
| op=discovery_search/discovery_list | discovery     |
| op=source_read / category=file_read| verification  |
| op=edit/build/test/runtime         | task action   |
| op=git_inspection/other/unknown    | unclassified  |
| category=search/directory_list     | discovery     |
| legacy shell/other repo tool       | unclassified  |

Absent field => conservative legacy rules, never silent task action.
