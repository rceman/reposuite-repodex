# Configuration

RepoDex reads `~/reposuite/repodex/config.toml`. The file may not exist —
then every optional feature is disabled. `[system_one]` is the only table.

## `[system_one]`

```toml
[system_one]
enabled = false                 # master switch; default off

[system_one.roles]              # role -> named model (optional each)
query  = "local"
rerank = "jev"

[system_one.models.<name>]
protocol = "system-one-v1"      # required; explicit version
url      = "https://host/path"  # required; the FULL configured URL is used
model    = "model-id"           # required
timeout_ms = 5000               # required; >0

[system_one.models.<name>.auth]
type = "none"                   # none | bearer | header
# bearer:  token = "..."
# header:  header = "X-API-Key", token = "..."
```

- **Named models** are mandatory; the same model may serve several roles.
- **Role independence** — `query`, `rerank`, both, or neither may be set.
- `enabled=false` keeps model definitions but disables all behavior.
- Plaintext tokens in the file are accepted (per spec) but are never logged,
  never put in errors, never copied into query/RDX1 provenance.

## Auth

```toml
[system_one.models.local.auth]
type = "none"

[system_one.models.jev.auth]
type = "bearer"
token = "<KEY>"

[system_one.models.custom.auth]
type = "header"
header = "X-API-Key"
token = "<KEY>"
```

## Validation (`system-one status`, or at query time)

Rejected: role→unknown model, unknown protocol, missing url/model, `timeout_ms
<= 0`, bearer without token, header without header/token, unknown auth type.
Config errors gate only System One — deterministic query keeps working.

## Config CLI

```bash
repodex config system-one.enabled true
repodex config system-one.models.jev.protocol system-one-v1
repodex config system-one.models.jev.url https://api.typesafe.ai/v1/systemone
repodex config system-one.models.jev.model jev-latest
repodex config system-one.models.jev.auth.type bearer
repodex config system-one.models.jev.auth.token <KEY>
repodex config system-one.roles.query jev
repodex config system-one.roles.rerank jev
repodex config system-one.enabled            # read a key
```

Dashes in keys map to underscores (`system-one` -> `system_one`).

## Local compatible endpoint

```toml
[system_one.models.local]
protocol = "system-one-v1"
url = "http://127.0.0.1:8787/custom/systemone"   # path is NOT hardcoded
model = "repodex-router"
timeout_ms = 2000
[system_one.models.local.auth]
type = "none"
```

A local compatible endpoint is interchangeable with hosted Jev by changing
only configuration.

## Hosted Jev example (do NOT ship a key; not enabled by default)

```toml
[system_one]
enabled = true
[system_one.roles]
query = "jev"
rerank = "jev"
[system_one.models.jev]
protocol = "system-one-v1"
url = "https://api.typesafe.ai/v1/systemone"
model = "jev-latest"
timeout_ms = 5000
[system_one.models.jev.auth]
type = "bearer"
token = "<TYPE_SAFE_API_KEY>"
```
