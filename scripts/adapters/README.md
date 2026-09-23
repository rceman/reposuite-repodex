# scripts/adapters — prototype/test/replay converters

Adapters here are **prototype, test, replay, and E2E converters**. They are NOT
part of RepoDex runtime semantics.

Every adapter's only job is to emit **canonical `reposuite.agent-event.v1`**
JSONL from a native harness/runtime format. RepoDex production code (`src/**`)
must not depend on `scripts/adapters/` or on any concrete harness format.

```text
native harness format
      │  (these adapters, or RepoSuite Relay)
      ▼
reposuite.agent-event.v1      <-- RepoDex boundary starts here
      ▼
RepoDex generic ingest -> Agent Event Store -> derived layers
```

Adapters contain NO RepoDex ranking, memory scoring, AgentActivity scoring,
query semantics, Jev semantics, or other production behavior.

## devin-atif.py

Converts a Devin `--export` ATIF trajectory to canonical AgentEvent v1 JSONL:

```bash
python3 scripts/adapters/devin-atif.py --input trajectory.json --output events.jsonl
cat trajectory.json | python3 scripts/adapters/devin-atif.py > events.jsonl
# optional canonical identity for benchmark replay:
#   --investigation-id --project-id --repository-id --repo-head --repo-root
```

Then ingest canonically:

```bash
reposuite-repodex agent-events ingest events.jsonl --store <dir>
```

## Future adapters

May add e.g. `codex-app-server.py`, `opencode-acp.py` for experimentation.
Their existence must never create production dependencies inside RepoDex.
Production runtime adapters belong in **RepoSuite Relay** — RepoDex must not
need modification when a new runtime is added there.
