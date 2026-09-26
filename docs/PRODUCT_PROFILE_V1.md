# RepoDex Product Profile V1

Canonical binary: `reposuite-repodex` (single public executable).

## Lifecycle

`serve` (foreground, supervisor entrypoint) / `start` (detached, idempotent,
waits ready) / `stop` (graceful, idempotent) / `restart` / `status`
(observational only, never starts the service). No systemd/launchd/boot
management. Runtime dir: `~/reposuite/repodex` (or `REPOSUITE_REPODEX_HOME` /
`--state-dir`).

## Machine API

`GET /v1/status`, `POST /v1/query`, `POST /v1/events`, `POST /v1/events/batch`,
`POST /v1/shutdown`. Bearer auth (token at `<state>/service.token`).

## Capability policy

| capability | policy | why |
|---|---|---|
| manifest intelligence | DEFAULT | restores missing facts, bounded |
| morphology vocab | OPT-IN | +6/40 wording recovery |
| repository-native vocab | OPT-IN | +2/40 domain-term recovery |
| cheap-model vocab | OFFLINE-EXPERIMENTAL | unreliable, not production |
| symbol memory | OPT-IN | view-bound, correctness-neutral |
| adaptive context | OPT-IN | measured bounded gain |
| guarded recipes | OPT-IN | bounded replayable evidence |
| utility policy | SHADOW-ONLY | telemetry, not applied |
| source witness | OPT-IN | exact-source delivery, no agent gain |

## Query authority

`root` (explicit RepositoryView) is authoritative — current bytes + metadata.
Branch is metadata only. No cwd fallback for service queries.
