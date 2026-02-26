---
name: flux
description: Publish events and query shared world state via Flux state engine. Use when agents need to share observations, coordinate on shared data, or track entity state across systems.
---

# Flux Skill

Flux is a persistent, shared, event-sourced world state engine. Agents publish immutable events, and Flux derives canonical state that all agents can observe.

## Key Concepts

- **Events**: Immutable observations (temperature readings, status changes, etc.)
- **Entities**: State objects derived from events (sensors, devices, agents)
- **Properties**: Key-value attributes of entities (merged on update — only changed properties need to be sent)
- **Streams**: Logical event namespaces (sensors, agents, system)
- **Namespaces**: Multi-tenant isolation with token auth (optional, for public instances)

## Prerequisites

Flux running at: `http://localhost:3000` (default, override with `FLUX_URL` env var)

Authentication: Optional. Set `FLUX_TOKEN` env var when connecting to auth-enabled instances.

## Testing

Verify Flux connection:
```bash
./scripts/flux.sh health
```

## Scripts

Use the provided bash script in the `scripts/` directory:
- `flux.sh` - Main CLI tool

## Common Operations

### Publish Event
```bash
./scripts/flux.sh publish <stream> <source> <entity_id> <properties_json>

# Example: Publish sensor reading
./scripts/flux.sh publish sensors agent-01 temp-sensor-01 '{"temperature":22.5,"unit":"celsius"}'
```

### Query Entity State
```bash
./scripts/flux.sh get <entity_id>

# Example: Get current sensor state
./scripts/flux.sh get temp-sensor-01
```

### List All Entities
```bash
./scripts/flux.sh list

# Filter by prefix
./scripts/flux.sh list --prefix scada/
```

### Delete Entity
```bash
./scripts/flux.sh delete <entity_id>

# Example: Remove old test entity
./scripts/flux.sh delete test/old-entity
```

### Batch Publish Events
```bash
./scripts/flux.sh batch '[
  {"stream":"sensors","source":"agent-01","payload":{"entity_id":"sensor-01","properties":{"temp":22}}},
  {"stream":"sensors","source":"agent-01","payload":{"entity_id":"sensor-02","properties":{"temp":23}}}
]'
```

### Check Connector Status
```bash
./scripts/flux.sh connectors
```

### Admin Config
```bash
# Read runtime config
./scripts/flux.sh admin-config

# Update (requires FLUX_ADMIN_TOKEN)
./scripts/flux.sh admin-config '{"rate_limit_per_namespace_per_minute": 5000}'
```

## Use Cases

### Multi-Agent Coordination
Agents publish observations to shared entities:
```bash
# Agent A observes temperature
flux.sh publish sensors agent-a room-101 '{"temperature":22.5}'

# Agent B queries current state
flux.sh get room-101
# Returns: {"temperature":22.5,...}
```

### Status Tracking
Track service/system state:
```bash
# Publish status change
flux.sh publish system monitor api-gateway '{"status":"healthy","uptime":3600}'

# Query current status
flux.sh get api-gateway
```

## API Endpoints

**Event Ingestion:**
- `POST /api/events` — Publish single event (1 MB limit)
- `POST /api/events/batch` — Publish multiple events (10 MB limit)

**State Query:**
- `GET /api/state/entities` — List all entities (supports `?prefix=` and `?namespace=` filters)
- `GET /api/state/entities/:id` — Get specific entity

**Entity Management:**
- `DELETE /api/state/entities/:id` — Delete single entity
- `POST /api/state/entities/delete` — Batch delete (by namespace/prefix/IDs)

**Real-time Updates:**
- `GET /api/ws` — WebSocket subscription

**Connectors:**
- `GET /api/connectors` — List connectors and status
- `POST /api/connectors/:name/token` — Store PAT credential
- `DELETE /api/connectors/:name/token` — Remove credential

**Admin:**
- `GET /api/admin/config` — Read runtime config
- `PUT /api/admin/config` — Update runtime config (requires FLUX_ADMIN_TOKEN)

**Namespaces (auth mode only):**
- `POST /api/namespaces` — Register namespace (returns auth token)

## Notes

- Events auto-generate UUIDs (no need to provide eventId)
- Properties **merge** on updates — only send changed properties, existing ones are preserved
- Timestamp field must be epoch milliseconds (i64) if provided, defaults to current time
- State persists in Flux (survives restarts via NATS JetStream + snapshots)
- Entity IDs support `/` for namespacing (e.g., `scada/pump-01`)
