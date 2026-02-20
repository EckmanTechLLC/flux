# Flux

**Persistent, shared, event-sourced world state engine**

Flux ingests immutable events, derives live in-memory state from them, and exposes that evolving world to agents, services, and humans through subscriptions and replay.

## What Flux Is

**Flux is a state engine, not just an event log.**

- **Event-sourced:** State is derived from immutable events
- **Persistent:** Events stored, state survives restarts
- **Shared:** Multiple systems observe the same world state
- **Real-time:** Updates propagate immediately to subscribers
- **Replay-capable:** Can reprocess history from any point
- **Domain-agnostic:** Works for any use case without encoding domain semantics

**Critical distinction:** Flux owns state derivation and persistence semantics. Consumers receive state updates from Flux, not raw events.

## Architecture

```
Producer → Event Ingestion → NATS (internal) → State Engine → WebSocket API → Consumers
                                                     ↑
                                          Connector Manager (polls external APIs)
```

Consumers observe Flux's canonical state. They never see raw events.

**Services (Docker Compose):**
- `nats` — JetStream event backbone (internal transport only)
- `flux` — State engine + HTTP/WebSocket API
- `flux-ui` — Web monitoring and management UI
- `connector-manager` — Polls external APIs (GitHub, etc.), publishes events to Flux

## Use Cases (Domain-Agnostic)

Flux is infrastructure that works for any domain:

- **Multi-agent LLM systems:** Agents coordinate through shared state
- **Industrial SCADA:** Real-time equipment state
- **Virtual worlds/games:** Shared game state, time-travel debugging
- **IoT platforms:** Device state aggregation
- **Personal life state:** GitHub, Gmail, Calendar → unified state via connectors

## Status

**Core engine stable. Connector framework in active development (ADR-005).**

## Documentation

**Core Docs:**
- [State Model](docs/state-model.md) - Entity/property model and event-to-state derivation
- [Architecture](docs/architecture.md) - System architecture and components
- [API Reference](docs/api.md) - HTTP and WebSocket API documentation

**Design & Context:**
- [FLUX-DESIGN.md](FLUX-DESIGN.md) - Complete vision and design principles
- [CLAUDE.md](CLAUDE.md) - Development context for Claude Code
- [Architecture Decision Records](docs/decisions/) - Key design decisions
- [Development Workflow](docs/workflow/) - Multi-session workflow

## Technology

- **State Engine:** Rust (performance, safety, no GC pauses)
- **Event Transport (Internal):** NATS with JetStream
- **APIs:** Rust with Axum (WebSocket + HTTP REST)
- **Connector Manager:** Rust (separate binary, same repo)
- **Deployment:** Docker Compose

## Quick Start

**Prerequisites:** Docker and Docker Compose

**1. Create your `.env` file:**

```bash
cp .env.example .env
# Edit .env — at minimum set FLUX_ENCRYPTION_KEY (see below)
```

**2. Generate a `FLUX_ENCRYPTION_KEY`:**

```bash
openssl rand -base64 32
```

Paste the output into `.env` as `FLUX_ENCRYPTION_KEY`. This key encrypts stored OAuth credentials. Required for the connector framework — without it, connectors are disabled but Flux starts normally.

**3. Start all services:**

```bash
docker compose up -d
```

**4. Check logs:**

```bash
docker compose logs -f flux
docker compose logs -f connector-manager
```

**5. Stop services:**

```bash
docker compose down
```

**Ports:**
- Flux API: `http://localhost:3000`
- Flux UI: `http://localhost:8082`
- NATS (external): `localhost:4223`

> **Note on startup time:** On first start (and after restarts), Flux replays all events from NATS JetStream to rebuild state. This is expected behavior, not a bug. Replay time scales with event history — snapshots are used to reduce replay window.

## Configuration

All configuration is via environment variables (`.env` file for Docker Compose).

### Required

| Variable | Description |
|---|---|
| `FLUX_ENCRYPTION_KEY` | Base64-encoded 32 random bytes. Encrypts stored OAuth tokens. Generate with `openssl rand -base64 32`. |

### Connector OAuth (required per connector)

| Variable | Description |
|---|---|
| `FLUX_OAUTH_GITHUB_CLIENT_ID` | GitHub OAuth App client ID |
| `FLUX_OAUTH_GITHUB_CLIENT_SECRET` | GitHub OAuth App client secret |
| `FLUX_OAUTH_CALLBACK_BASE_URL` | Public base URL for OAuth callbacks (e.g. `https://flux.example.com`) |

### Optional

| Variable | Default | Description |
|---|---|---|
| `FLUX_CREDENTIALS_DB` | `/data/credentials.db` | Path to encrypted credentials SQLite database |
| `FLUX_ADMIN_TOKEN` | _(none)_ | Token for admin API access (`PUT /api/admin/config`). If unset, admin writes are disabled. |
| `FLUX_AUTH_ENABLED` | `false` | Enable namespace token auth for writes. Internal deployments leave this false. |
| `PORT` | `3000` | Flux API port |

### NATS

NATS runs as an internal Docker service. The connector-manager and flux containers connect to it via `nats://nats:4222` (Docker internal network). External access (e.g. for debugging) is available at `localhost:4223`.

Do not expose port 4222 externally — NATS has no auth in this configuration.

## Publishing Events

```bash
curl -X POST http://localhost:3000/api/events \
  -H "Content-Type: application/json" \
  -d '{
    "stream": "sensors",
    "source": "sensor-01",
    "payload": {
      "entity_id": "temp-sensor-01",
      "properties": {
        "temperature": 22.5,
        "unit": "celsius"
      }
    }
  }'
```

`eventId` is auto-generated if omitted. `payload` must include `entity_id` and `properties` for state derivation.

## Querying State

```bash
# Get all entities
curl http://localhost:3000/api/state/entities

# Get specific entity
curl http://localhost:3000/api/state/entities/temp-sensor-01

# Filter by namespace
curl http://localhost:3000/api/state/entities?namespace=matt
```

## WebSocket Subscription

```javascript
const ws = new WebSocket('ws://localhost:3000/api/ws');

ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'subscribe', entity_id: 'temp-sensor-01' }));
};

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  console.log('State update:', update);
};
```

When `FLUX_AUTH_ENABLED=true`, pass token as query param: `ws://host/api/ws?token=<token>`

## Authentication & Multi-tenancy

**Internal mode (default, `FLUX_AUTH_ENABLED=false`):**
- No authentication required
- Simple entity IDs (`sensor-01`)
- For trusted environments (VPN, internal network)

**Public mode (`FLUX_AUTH_ENABLED=true`):**
- Token-based write authorization per namespace
- Namespaced entity IDs (`matt/sensor-01`)
- Reads remain open (anyone can query/subscribe)

### Enabling Auth

```bash
FLUX_AUTH_ENABLED=true  # in .env
```

### Register a Namespace

```bash
curl -X POST http://localhost:3000/api/namespaces \
  -H "Content-Type: application/json" \
  -d '{"name": "matt"}'

# Response: {"namespace_id": "ns_7x9f2a", "name": "matt", "token": "550e8400-..."}
```

Use the returned token as `Authorization: Bearer <token>` on write requests.

## Admin Config API

Runtime limits are configurable without restart via the admin API.

```bash
# Read current config (any authenticated user when auth enabled, open otherwise)
curl http://localhost:3000/api/admin/config

# Update limits (requires FLUX_ADMIN_TOKEN)
curl -X PUT http://localhost:3000/api/admin/config \
  -H "Authorization: Bearer <admin-token>" \
  -H "Content-Type: application/json" \
  -d '{
    "rate_limit_enabled": true,
    "rate_limit_per_namespace_per_minute": 10000,
    "body_size_limit_single_bytes": 1048576,
    "body_size_limit_batch_bytes": 10485760
  }'
```

## Connectors

Flux can pull data from external APIs via the Connector Framework (ADR-005). Connectors are managed through the UI or API.

**GitHub connector (available now):** Syncs repos, issues, PRs, and notifications as Flux entities.

**Setup via UI:**
1. Open `http://localhost:8082`
2. Navigate to **Connectors** panel
3. Click **Connect GitHub**
4. Complete OAuth flow

**API:**
```bash
# List connectors and status
curl http://localhost:3000/api/connectors

# Start OAuth flow
curl http://localhost:3000/api/connectors/github/oauth/start

# Get connector status
curl http://localhost:3000/api/connectors/github
```

**GitHub OAuth setup:** Create an OAuth App at [github.com/settings/developers](https://github.com/settings/developers). Set the callback URL to `<FLUX_OAUTH_CALLBACK_BASE_URL>/api/connectors/github/oauth/callback`.

## Web UI

Flux UI runs as a Docker container (included in `docker-compose.yml`).

```bash
docker compose up -d flux-ui
```

Access at `http://localhost:8082`.

**Features:**
- Real-time metrics (EPS, entity count, active publishers)
- Live entity viewer with grouping and filtering
- Connector management (OAuth setup, enable/disable, status)
- Admin config panel

## Integrations

### OpenClaw Skill

```bash
clawhub install flux
```

Agents can interact naturally: "Check Flux for the current state of sensor-01", "Publish observation: temperature is 22.5 in room-101".

See `/examples/openclaw-skill/` for full documentation.

---

## API Summary

**Event Ingestion:**
- `POST /api/events` — Publish single event
- `POST /api/events/batch` — Publish multiple events

**State Query:**
- `GET /api/state/entities` — List all entities (filterable by namespace, prefix)
- `GET /api/state/entities/:id` — Get specific entity

**Entity Management:**
- `DELETE /api/state/entities/:id` — Delete single entity
- `POST /api/state/entities/delete` — Batch delete (by namespace/prefix/IDs)

**Real-time Updates:**
- `GET /api/ws` — WebSocket subscription (state updates, metrics, deletions)

**Namespaces:**
- `POST /api/namespaces` — Register namespace (returns auth token)

**Connectors:**
- `GET /api/connectors` — List connectors and status
- `GET /api/connectors/:name` — Connector status
- `GET /api/connectors/:name/oauth/start` — Begin OAuth flow
- `GET /api/connectors/:name/oauth/callback` — OAuth callback (set as redirect URI in provider)

**Admin:**
- `GET /api/admin/config` — Read runtime config
- `PUT /api/admin/config` — Update runtime config (requires `FLUX_ADMIN_TOKEN`)

For detailed API documentation, see [API Reference](docs/api.md).

## License

MIT License — see [LICENSE](LICENSE) file for details.
