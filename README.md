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
```

Consumers observe Flux's canonical state. They never see raw events.

## Use Cases (Domain-Agnostic)

Flux is infrastructure that works for any domain:

- **Multi-agent LLM systems:** Agents coordinate through shared state
- **Industrial SCADA:** Real-time equipment state
- **Virtual worlds/games:** Shared game state, time-travel debugging
- **IoT platforms:** Device state aggregation
- **Collaborative systems:** Real-time document/project state

## Status

**Fresh start:** Building state engine from scratch using flux-reactor patterns.

Previous work (event backbone approach) archived in `archive/event-backbone` branch.

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
- **Deployment:** Docker Compose

## Quick Start

### For OpenClaw Users

**Install the skill:**
```bash
clawhub install flux
```

**Using a hosted instance:**

Contact [@EckmanTechLLC](https://github.com/EckmanTechLLC) for access to a test instance, or run your own (see below).

---

### Running Your Own Flux

**Prerequisites:**
- Docker and Docker Compose
- (Optional) curl for testing

### Running Flux

```bash
# Start Flux + NATS
docker-compose up -d

# Check logs
docker-compose logs -f flux

# Stop services
docker-compose down
```

Flux will be available at `http://localhost:3000`.

### Configuration

Flux uses `config.toml` for configuration. Default settings work for most cases.

**Snapshot configuration:**
```toml
[snapshot]
enabled = true
interval_minutes = 5          # Snapshot frequency
directory = "/var/lib/flux/snapshots"  # Snapshot storage
keep_count = 10               # Number of snapshots to retain
```

**NATS configuration:**
```toml
[nats]
url = "nats://localhost:4222"
stream_name = "FLUX_EVENTS"
```

**Recovery configuration:**
```toml
[recovery]
auto_recover = true  # Load snapshot on startup
```

Snapshots enable fast recovery (<10 seconds for 100k entities) and state persistence across restarts.

### Publishing Events

Events auto-generate UUIDs (eventId optional):

```bash
# POST event to Flux
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

**Note:** `eventId` is auto-generated if omitted. `payload` must include `entity_id` and `properties` for state derivation.

### Querying State

```bash
# Get all entities
curl http://localhost:3000/api/state/entities

# Get specific entity
curl http://localhost:3000/api/state/entities/temp-sensor-01
```

### WebSocket Subscription

```javascript
const ws = new WebSocket('ws://localhost:3000/api/ws');

ws.onopen = () => {
  // Subscribe to entity updates
  ws.send(JSON.stringify({
    type: 'subscribe',
    entityId: 'temp-sensor-01'
  }));
};

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  console.log('State update:', update);
};
```

## Authentication & Multi-tenancy

Flux supports two deployment modes:

**Internal Mode (default):**
- No authentication required
- Simple entity IDs (`sensor-01`)
- For trusted environments (VPN, internal network)

**Public Mode:**
- Token-based write authorization
- Namespaced entity IDs (`matt/sensor-01`)
- Open reading (anyone can query/subscribe)

### Enabling Authentication

Set `FLUX_AUTH_ENABLED=true` in environment or `config.toml`:

```toml
[auth]
enabled = true
```

### Using a Public Instance

**1. Register a namespace:**
```bash
curl -X POST http://localhost:3000/api/namespaces \
  -H "Content-Type: application/json" \
  -d '{"name": "matt"}'

# Response: {"namespace_id": "ns_7x9f2a", "name": "matt", "token": "550e8400-..."}
```

**2. Publish events with your token:**
```bash
curl -X POST http://localhost:3000/api/events \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer 550e8400-..." \
  -d '{
    "stream": "sensors",
    "source": "sensor-01",
    "payload": {
      "entity_id": "matt/sensor-01",
      "properties": {"temperature": 22.5}
    }
  }'
```

**3. Query entities (no auth required):**
```bash
# All entities
curl http://localhost:3000/api/state/entities

# Filter by namespace
curl http://localhost:3000/api/state/entities?namespace=matt

# Filter by prefix
curl http://localhost:3000/api/state/entities?prefix=matt/sensor
```

**Note:** Tokens control writes only. Reading is always open for observation and coordination.

## Web UI

Flux includes a real-time web UI for monitoring and management.

**Start UI:**
```bash
cd ui
node server.js
```

**Access:**
- Local: `http://localhost:8082`
- Remote: `http://<your-server-ip>:8082`

**Features:**
- Real-time metrics (EPS, entity count, active publishers)
- Live entity viewer with grouping and filtering
- Load testing controls
- Dark theme optimized for monitoring

**Configuration:**
```bash
# Custom port
UI_PORT=9000 node server.js

# Connect to different Flux instance
FLUX_API=http://other-host:3000 FLUX_WS=ws://other-host:3000/api/ws node server.js
```

## Integrations

### OpenClaw Skill

Flux includes an [OpenClaw](https://openclaw.ai) skill for agent integration:

**Installation:**
```bash
# Copy skill to OpenClaw workspace
cp -r examples/openclaw-skill ~/.openclaw/workspace/skills/flux-interact
```

**Usage:**
OpenClaw agents can naturally interact with Flux:
- "Test Flux connection and show entities"
- "Publish observation: temperature is 22.5 celsius in room-101"
- "Check Flux for the current state of sensor-01"

See `/examples/openclaw-skill/` for full documentation.

---

## API Summary

**Event Ingestion:**
- `POST /api/events` - Publish single event
- `POST /api/events/batch` - Publish multiple events

**State Query:**
- `GET /api/state/entities` - List all entities
- `GET /api/state/entities/:id` - Get specific entity

**Entity Management:**
- `DELETE /api/state/entities/:id` - Delete single entity
- `POST /api/state/entities/delete` - Batch delete (by namespace/prefix/IDs)

**Real-time Updates:**
- `GET /api/ws` - WebSocket subscription (state updates, metrics, deletions)

For detailed API documentation with examples, see [API Reference](docs/api.md).

## License

MIT License - see [LICENSE](LICENSE) file for details.
