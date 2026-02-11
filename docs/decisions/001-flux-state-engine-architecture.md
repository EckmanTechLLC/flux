# ADR-001: Flux State Engine Architecture

**Status:** Accepted
**Date:** 2026-02-11
**Deciders:** Architecture Team

---

## Context

Flux is a **persistent, shared, event-sourced world state engine**. It ingests immutable events, derives live in-memory state from them, and exposes that evolving world to agents, services, and humans through subscriptions and replay.

### Why State Engine (vs Event Backbone)

Previous implementation (archived in `archive/event-backbone`) took an event backbone approach where:
- Flux validated and forwarded events to NATS
- Consumers subscribed to NATS directly
- Each consumer derived its own state from events

**Problems:**
- Every consumer reimplemented state derivation logic
- No shared world view (inconsistent state across systems)
- No canonical source of truth for current state
- Consumers exposed to NATS directly (tight coupling)

**State Engine Approach:**
- Flux owns state derivation and persistence semantics
- Consumers receive **state updates** from Flux, not raw events
- Single canonical world state maintained by Flux
- NATS is internal transport only

**Critical Distinction:** Flux maintains the world state. Consumers observe it, they don't derive it.

**Reference:** `flux-reactor` at `/projects/flux-reactor/` provides proven patterns (state engine, WebSocket API, entity/property model). Flux generalizes these for domain-agnostic use.

---

## Decision

### Architecture Overview

```
Producers (HTTP/WebSocket)
         ↓
┌─────────────────────────────────────────┐
│         Event Ingestion Layer           │
│  - Validate envelope                    │
│  - Generate UUIDv7                      │
│  - Persist to NATS (internal)           │
└─────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────┐
│      NATS JetStream (Internal)          │
│  - Event persistence                    │
│  - NOT exposed to consumers             │
│  - Enables replay and recovery          │
└─────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────┐
│          State Engine (Rust)            │
│  - Subscribe to NATS events             │
│  - Derive in-memory state (DashMap)     │
│  - Apply events → update entities       │
│  - Broadcast state changes              │
└─────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────┐
│       Subscription Manager              │
│  - Manage WebSocket connections         │
│  - Filter updates per subscription      │
│  - Push state changes (not events)      │
└─────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────┐
│              APIs                       │
│  - WebSocket: Real-time state updates   │
│  - HTTP REST: Query current state       │
└─────────────────────────────────────────┘
         ↓
    Consumers
```

### Core Components

#### 1. Event Ingestion Layer

Accept events from producers, validate envelope, persist to NATS.

**API:** `POST /api/events` (single), `POST /api/events/batch` (multiple)
**Validation:** Required: `stream`, `source`, `timestamp`, `payload`. Generate `eventId` (UUIDv7) if missing.
**Flow:** Validate → Generate ID → Publish to NATS → Return confirmation

#### 2. State Engine (Rust)

**Responsibility:** Derive and maintain canonical in-memory state from events.

**Data Model:**
```rust
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// Entity represents a domain-agnostic object in the world state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    /// Unique entity identifier (e.g., "agent_001", "sensor_42")
    pub id: String,

    /// Key-value properties (domain-specific)
    pub properties: HashMap<String, Value>,

    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

/// State engine maintains in-memory world state
pub struct StateEngine {
    /// Lock-free concurrent map for fast reads
    entities: Arc<DashMap<String, Entity>>,

    /// Broadcast channel for state change events
    state_tx: broadcast::Sender<StateUpdate>,

    /// NATS client for subscribing to events
    nats_client: async_nats::Client,
}

impl StateEngine {
    /// Update entity property (core state mutation)
    pub async fn update_property(
        &self,
        entity_id: &str,
        property: &str,
        value: Value,
    ) -> StateUpdate {
        let now = Utc::now();

        // Get or create entity
        let mut entity = self.entities.entry(entity_id.to_string())
            .or_insert_with(|| Entity {
                id: entity_id.to_string(),
                properties: HashMap::new(),
                last_updated: now,
            });

        // Get old value for delta tracking
        let old_value = entity.properties.get(property).cloned();

        // Update property
        entity.properties.insert(property.to_string(), value.clone());
        entity.last_updated = now;

        // Create state update
        let update = StateUpdate {
            entity_id: entity_id.to_string(),
            property: property.to_string(),
            old_value,
            new_value: value,
            timestamp: now,
        };

        // Broadcast to subscribers
        let _ = self.state_tx.send(update.clone());

        update
    }

    /// Process event from NATS and update state
    pub async fn process_event(&self, event: FluxEvent) {
        // Extract entity_id and properties from event payload
        // This is domain-agnostic: payload structure determines state shape

        if let Some(entity_id) = event.payload.get("entity_id").and_then(|v| v.as_str()) {
            if let Some(properties) = event.payload.get("properties").and_then(|v| v.as_object()) {
                for (key, value) in properties {
                    self.update_property(entity_id, key, value.clone()).await;
                }
            }
        }
    }

    /// Subscribe to NATS events and process them
    pub async fn run(&self) -> Result<()> {
        let mut subscriber = self.nats_client
            .subscribe("flux.events.>")
            .await?;

        while let Some(msg) = subscriber.next().await {
            if let Ok(event) = serde_json::from_slice::<FluxEvent>(&msg.payload) {
                self.process_event(event).await;
            }
            msg.ack().await?;
        }

        Ok(())
    }
}
```

**State Update Message:**
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateUpdate {
    pub entity_id: String,
    pub property: String,
    pub old_value: Option<Value>,
    pub new_value: Value,
    pub timestamp: DateTime<Utc>,
}
```

**Key Features:**
- DashMap for lock-free concurrent reads (critical for performance)
- Generic entity/property model (no hardcoded entity types)
- Events drive state changes (event-sourced)
- Broadcasts state updates to subscribers (not raw events)

#### 3. Event Model (Reused from Archive)

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FluxEvent {
    /// UUIDv7 identifier (time-ordered, globally unique)
    #[serde(rename = "eventId")]
    pub event_id: String,

    /// Logical stream/namespace
    pub stream: String,

    /// Producer identity
    pub source: String,

    /// Unix epoch milliseconds (producer time)
    pub timestamp: i64,

    /// Optional ordering/grouping key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,

    /// Optional schema metadata (not validated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Domain-specific event data (opaque)
    pub payload: Value,
}
```

**Validation Rules:**
- Required: `stream`, `source`, `timestamp`, `payload`
- Auto-generate: `eventId` (UUIDv7) if missing
- No payload schema validation (domain-agnostic)

#### 4. Subscription Manager

Manage WebSocket connections, filter state updates, push to clients.

**Subscription Targets:** All entities, specific entity, specific property.

**WebSocket Protocol:**
- Client → Server: `subscribe`, `unsubscribe`, `query`
- Server → Client: `state_update` (property changes), `snapshot` (full entity)

#### 5. HTTP REST API

**Query Endpoints:**
```
GET  /api/state/entities              # List all entities
GET  /api/state/entities/:id          # Get specific entity
GET  /api/state/entities/:id/:prop    # Get specific property
```

**Response Format:**
```json
{
  "entity_id": "sensor_001",
  "properties": {
    "temperature": 72.5,
    "humidity": 45.2,
    "status": "active"
  },
  "last_updated": "2026-02-11T10:30:00Z"
}
```

### Technology Stack

**Rust:** State engine (DashMap), APIs (Axum/Tokio). Performance, safety, no GC pauses.
**NATS JetStream (Internal):** Event persistence, at-least-once delivery. Not exposed to consumers.
**Deployment:** Docker Compose (Flux + NATS). Ports: Flux (3000), NATS (4222, internal).
**Clients (Future):** Python/JavaScript examples (Phase 1), full SDKs (Phase 2+).

---

## Phase 1: Implementation Tasks

### Task 1: Project Structure & Dependencies
**Files:**
- `Cargo.toml` - Rust project with dependencies
- `src/main.rs` - Binary entry point
- `src/lib.rs` - Library structure
- `.gitignore` - Rust/IDE exclusions

**Dependencies:**
- `tokio` - Async runtime
- `axum` - Web framework
- `async-nats` - NATS client
- `dashmap` - Concurrent HashMap
- `serde`, `serde_json` - Serialization
- `uuid` - UUIDv7 generation
- `chrono` - Timestamps

### Task 2: Event Model & Validation
**Files:**
- `src/event.rs` - FluxEvent struct
- `src/event/validator.rs` - Envelope validation
- `src/event/tests.rs` - Unit tests

**Scope:**
- Reuse event model from archive (Go → Rust)
- Envelope validation (required fields)
- UUIDv7 generation
- Test coverage

### Task 3: State Engine Core
**Files:**
- `src/state/engine.rs` - StateEngine struct
- `src/state/entity.rs` - Entity, StateUpdate structs
- `src/state/tests.rs` - Unit tests

**Scope:**
- In-memory state (DashMap)
- Entity/property model
- update_property() method
- process_event() method
- State change broadcasting

### Task 4: Event Ingestion API
**Files:**
- `src/api/ingestion.rs` - HTTP routes
- `src/nats/publisher.rs` - NATS publish logic

**Scope:**
- POST /api/events (single)
- POST /api/events/batch (multiple)
- Validate → Publish to NATS → Return confirmation
- Error handling

### Task 5: WebSocket Subscription API
**Files:**
- `src/api/websocket.rs` - WebSocket handler
- `src/subscription/manager.rs` - Connection management
- `src/subscription/filter.rs` - Update filtering

**Scope:**
- WebSocket upgrade
- Subscribe/unsubscribe messages
- State update push
- Connection lifecycle

### Task 6: HTTP Query API & Integration
**Files:**
- `src/api/query.rs` - REST endpoints
- `src/main.rs` - Wire up all components
- `docker-compose.yml` - Flux + NATS

**Scope:**
- GET /api/state/entities
- GET /api/state/entities/:id
- Integration test (publish event → query state)
- Docker Compose setup

---

## Consequences

### Positive
- Single source of truth (Flux owns canonical state)
- Simplified consumers (observe updates, no state derivation)
- High performance (Rust/DashMap: 100k+ updates/sec, lock-free reads)
- Real-time sync (WebSocket: sub-second latency)
- Domain agnostic (generic entity/property model)
- Event sourcing (rebuild state from events)
- Proven patterns (based on flux-reactor)

### Negative
- Stateful (in-memory state, not stateless) → *Phase 2: snapshots*
- Memory bounded (100k-1M entities target) → *Future: sharding*
- NATS dependency (lightweight, reliable)
- No snapshot/recovery in Phase 1 → *Phase 2*

### Neutral
- NATS is internal (consumers only see Flux APIs)
- At-least-once delivery (consumers handle duplicates)
- No schema validation (payload is opaque)

---

## What's NOT in Phase 1

**Out of Scope (Future Phases):**

- ❌ **Snapshot/Recovery:** State rebuilds from NATS on restart
  - Phase 1: Restart loses state, must replay all events
  - Phase 2: Periodic snapshots + incremental replay

- ❌ **Advanced Authorization:** No auth in Phase 1
  - Phase 1: Open access (dev environment)
  - Phase 2: Token-based auth, stream-level permissions

- ❌ **Replay from Arbitrary Point:** Only start-from-beginning
  - Phase 1: State engine reads all events on startup
  - Phase 2: Time-based and sequence-based replay

- ❌ **Client SDK Libraries:** Examples only
  - Phase 1: Python/JavaScript examples (manual WebSocket)
  - Phase 2: Packaged client libraries

- ❌ **Multi-tenancy:** Single world state
  - Phase 1: One shared state for all clients
  - Phase 2: Namespace isolation, tenant separation

- ❌ **Monitoring Dashboard:** Basic logs only
  - Phase 1: Stdout logs, no metrics
  - Phase 2: Prometheus metrics, Grafana dashboards

---

## References

- `/FLUX-DESIGN.md` - Complete Flux vision and principles
- `/projects/flux-reactor/PRODUCTION-ARCHITECTURE.md` - State engine patterns
- `archive/event-backbone` - Previous event model (reused)
- [DashMap](https://docs.rs/dashmap) - Lock-free concurrent HashMap
- [NATS JetStream](https://docs.nats.io/nats-concepts/jetstream) - Event persistence

---

## Next Steps

1. Review and approve ADR-001
2. Implement Phase 1 tasks (1-6) sequentially
3. Test integration: Publish event → State updates → WebSocket receives
4. Document API in `/docs/api/`
5. Create Python client example
6. Plan Phase 2 (snapshots, auth, replay)
