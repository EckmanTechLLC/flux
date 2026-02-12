# Flux Architecture

**Status:** Phase 1 Implementation
**Last Updated:** 2026-02-11

---

## Overview

Flux is a **persistent, shared, event-sourced world state engine**. It ingests events, derives in-memory state, and exposes that state to consumers via WebSocket and HTTP APIs.

**Key principle:** Flux owns state derivation. Consumers observe state updates, not raw events.

---

## System Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                         Producers                            │
│              (Apps, Agents, Sensors, Services)               │
└────────────────┬─────────────────────────────────────────────┘
                 │ HTTP POST /api/events
                 │
                 ▼
┌──────────────────────────────────────────────────────────────┐
│                   Event Ingestion Layer                      │
│  ┌────────────────────────────────────────────────────┐     │
│  │  - Validate envelope (stream, source, timestamp)   │     │
│  │  - Generate UUIDv7 event ID if missing             │     │
│  │  - Publish to NATS JetStream                       │     │
│  │  - Return confirmation to producer                 │     │
│  └────────────────────────────────────────────────────┘     │
│                          (Rust/Axum)                         │
└────────────────┬─────────────────────────────────────────────┘
                 │ NATS publish
                 │
                 ▼
┌──────────────────────────────────────────────────────────────┐
│              NATS JetStream (Internal Only)                  │
│  ┌────────────────────────────────────────────────────┐     │
│  │  - Event persistence (durable stream)              │     │
│  │  - At-least-once delivery                          │     │
│  │  - Enables event replay                            │     │
│  │  - NOT exposed to consumers                        │     │
│  └────────────────────────────────────────────────────┘     │
│                    (Stream: FLUX_EVENTS)                     │
└────────────────┬─────────────────────────────────────────────┘
                 │ NATS subscribe (flux.events.>)
                 │
                 ▼
┌──────────────────────────────────────────────────────────────┐
│                      State Engine                            │
│  ┌────────────────────────────────────────────────────┐     │
│  │  - Subscribe to NATS event stream                  │     │
│  │  - Extract entity_id and properties from payload   │     │
│  │  - Update in-memory state (DashMap)                │     │
│  │  - Broadcast StateUpdate to subscribers            │     │
│  └────────────────────────────────────────────────────┘     │
│                                                              │
│  In-Memory State:                                            │
│  ┌────────────────────────────────────────────────────┐     │
│  │ DashMap<entity_id, Entity>                         │     │
│  │   - Lock-free concurrent reads                     │     │
│  │   - Entity { id, properties, last_updated }        │     │
│  └────────────────────────────────────────────────────┘     │
│                          (Rust)                              │
└────────────────┬─────────────────────────────────────────────┘
                 │ Broadcast channel (state updates)
                 │
                 ▼
┌──────────────────────────────────────────────────────────────┐
│                   Subscription Manager                       │
│  ┌────────────────────────────────────────────────────┐     │
│  │  - Manage WebSocket connections                    │     │
│  │  - Filter updates per subscription                 │     │
│  │  - Push StateUpdate messages to clients            │     │
│  └────────────────────────────────────────────────────┘     │
│                          (Rust)                              │
└────────────────┬─────────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────────┐
│                          APIs                                │
│  ┌───────────────────────┐  ┌──────────────────────────┐    │
│  │  WebSocket API        │  │  HTTP REST API           │    │
│  │  GET /api/ws          │  │  GET /api/state/entities │    │
│  │                       │  │  GET /api/state/.../:id  │    │
│  │  - Real-time updates  │  │  - Query current state   │    │
│  │  - Subscribe/filter   │  │  - Snapshot retrieval    │    │
│  └───────────────────────┘  └──────────────────────────┘    │
│                       (Rust/Axum)                            │
└────────────────┬─────────────────────────────────────────────┘
                 │
                 ▼
┌──────────────────────────────────────────────────────────────┐
│                         Consumers                            │
│           (Apps, Agents, Dashboards, Services)               │
└──────────────────────────────────────────────────────────────┘
```

---

## Component Descriptions

### 1. Event Ingestion Layer

**Responsibility:** Accept events from producers, validate, and persist to NATS.

**Implementation:** Rust with Axum web framework

**Key operations:**

1. **Validate envelope** - Check required fields (stream, source, timestamp, payload)
2. **Generate event ID** - Create UUIDv7 if not provided
3. **Publish to NATS** - Send to `flux.events.{stream}` subject
4. **Return confirmation** - Respond with eventId and stream

**API endpoints:**
- `POST /api/events` - Publish single event
- `POST /api/events/batch` - Publish multiple events

**Error handling:**
- 400 Bad Request: Invalid event envelope
- 500 Internal Server Error: NATS publish failure

**Why separate layer:**
- Decouples producers from NATS
- Provides validation before persistence
- Enables future authorization checks

---

### 2. NATS JetStream (Internal Transport)

**Responsibility:** Durable event storage and delivery.

**Implementation:** NATS with JetStream persistence

**Key characteristics:**

- **Stream name:** `FLUX_EVENTS`
- **Subjects:** `flux.events.*` (one subject per stream)
- **Retention:** Based on limits (configurable)
- **Delivery:** At-least-once semantics
- **Not exposed:** Consumers never access NATS directly

**Why NATS:**
- High throughput (100k+ msgs/sec)
- Built-in persistence (JetStream)
- Replay capability
- Mature, production-ready

**Why internal only:**
- Loose coupling (can swap transport)
- Simplified consumer interface
- Flux controls delivery semantics

---

### 3. State Engine

**Responsibility:** Derive and maintain canonical in-memory world state from events.

**Implementation:** Rust with DashMap for lock-free concurrent access

**Key operations:**

1. **Subscribe to events** - NATS consumer on `flux.events.>`
2. **Extract state data** - Parse `entity_id` and `properties` from event payload
3. **Update in-memory state** - Upsert entity properties in DashMap
4. **Broadcast changes** - Send StateUpdate to subscribers

**Data structures:**

```rust
// In-memory state storage
DashMap<String, Entity>

// Entity structure
Entity {
    id: String,
    properties: HashMap<String, Value>,
    last_updated: DateTime<Utc>,
}

// State update broadcast
StateUpdate {
    entity_id: String,
    property: String,
    old_value: Option<Value>,
    new_value: Value,
    timestamp: DateTime<Utc>,
}
```

**Why DashMap:**
- Lock-free concurrent reads (critical for performance)
- Safe concurrent writes
- No global lock contention
- 100k+ reads/sec per core

**State derivation logic:**
```
For each event:
  1. Extract entity_id from payload
  2. Extract properties object from payload
  3. For each property in properties:
     - Get old value (if exists)
     - Update entity.properties[key] = value
     - Update entity.last_updated = now
     - Broadcast StateUpdate(entity_id, key, old, new, timestamp)
```

**Phase 1 limitation:** No snapshot persistence. State rebuilt from events on restart.

---

### 4. Subscription Manager

**Responsibility:** Manage WebSocket connections and filter state updates.

**Implementation:** Rust with Tokio async tasks

**Key operations:**

1. **Accept WebSocket connections** - HTTP upgrade to WebSocket
2. **Handle subscribe/unsubscribe** - Track which entities/properties client wants
3. **Filter updates** - Only send relevant StateUpdate messages
4. **Handle disconnects** - Clean up subscriptions

**Subscription types:**

- **All entities:** Receive updates for any entity
- **Specific entity:** Receive updates for one entity only
- **Specific property:** (Future) Receive updates for one property

**Message flow:**

```
Client → Server: {"type": "subscribe", "entityId": "sensor-01"}
Server → Client: {"type": "update", "entity": {...}}
Server → Client: {"type": "update", "entity": {...}}
```

**Why WebSocket:**
- Bidirectional communication
- Low latency (sub-second updates)
- Persistent connection (no polling)
- Standard protocol (works in browsers)

---

### 5. HTTP REST API

**Responsibility:** Query current state (snapshot access).

**Implementation:** Rust with Axum

**Endpoints:**

- `GET /api/state/entities` - List all entities
  - Returns array of Entity objects
  - Use for initial state snapshot

- `GET /api/state/entities/:id` - Get specific entity
  - Returns single Entity object
  - Returns 404 if not found
  - Use for checking specific entity

**Response format:**

```json
{
  "id": "sensor-01",
  "properties": {
    "temperature": 22.5,
    "humidity": 45.0
  },
  "last_updated": "2026-02-11T10:30:00.123Z"
}
```

**Use cases:**

- Initial state snapshot before subscribing
- Polling for clients without WebSocket
- Manual inspection (curl, browser)
- Integration with existing HTTP-based systems

---

## Data Flow

### Publishing an Event

```
1. Producer creates event payload:
   {
     "stream": "sensors",
     "source": "sensor-01",
     "payload": {
       "entity_id": "sensor-01",
       "properties": {"temperature": 22.5}
     }
   }

2. POST to /api/events

3. Event Ingestion validates and adds eventId (UUIDv7)

4. Event published to NATS: flux.events.sensors

5. Response: {"eventId": "01933d7a-...", "stream": "sensors"}

6. State Engine receives event from NATS

7. State Engine updates:
   - entities["sensor-01"].properties["temperature"] = 22.5
   - entities["sensor-01"].last_updated = now

8. State Engine broadcasts:
   StateUpdate {
     entity_id: "sensor-01",
     property: "temperature",
     old_value: null,
     new_value: 22.5,
     timestamp: "2026-02-11T10:30:00.456Z"
   }

9. Subscription Manager sends update to subscribed WebSocket clients
```

### Subscribing to State Updates

```
1. Client connects: WebSocket to ws://localhost:3000/api/ws

2. Client subscribes:
   → {"type": "subscribe", "entityId": "sensor-01"}

3. Server acknowledges subscription

4. When state changes (from event processing):
   ← {"type": "update", "entity": {...}}

5. Client processes update locally

6. To unsubscribe:
   → {"type": "unsubscribe", "entityId": "sensor-01"}
```

### Querying State

```
1. Client sends: GET /api/state/entities/sensor-01

2. State Engine reads from DashMap (lock-free)

3. Response:
   {
     "id": "sensor-01",
     "properties": {"temperature": 22.5},
     "last_updated": "2026-02-11T10:30:00.456Z"
   }

4. No side effects (read-only)
```

---

## Design Principles

### 1. State Ownership

**Flux owns canonical state. Consumers observe it.**

- State engine derives state from events
- Consumers receive state updates, not raw events
- Single source of truth
- No consumer-side state derivation

### 2. Domain Agnosticism

**Flux doesn't know what entities represent.**

- Generic entity/property model
- No built-in entity types
- Payload is opaque
- Applications define semantics

### 3. Internal NATS

**NATS is an implementation detail.**

- Not exposed to consumers
- Can be replaced without API changes
- Loose coupling
- Simplified consumer interface

### 4. Lock-Free Reads

**State reads must not block.**

- DashMap for concurrent access
- No global locks
- Critical for real-time performance
- Thousands of subscribers supported

### 5. Event Sourcing

**Events are immutable source of truth.**

- State is derived, not primary
- Can rebuild by replaying events
- Audit trail of all changes
- Time-travel debugging (future)

---

## Technology Stack

**State Engine & APIs:** Rust
- Performance: No GC pauses, predictable latency
- Safety: Memory safety, thread safety
- Concurrency: Tokio async runtime
- Libraries: Axum (web), DashMap (state), async-nats (events)

**Event Transport:** NATS with JetStream
- Throughput: 100k+ msgs/sec
- Persistence: Durable streams
- Replay: Built-in time-travel
- Maturity: Production-ready

**Deployment:** Docker Compose
- Easy setup: `docker-compose up`
- Non-invasive: Specific ports
- Includes: Flux + NATS
- Development-friendly

---

## Performance Characteristics

**Throughput (Phase 1, single instance):**
- Event ingestion: 10k-50k events/sec
- State updates: 10k-50k updates/sec
- WebSocket subscribers: 1k-10k concurrent
- HTTP queries: 50k-100k req/sec (read-only)

**Latency:**
- Event → State update: <10ms (median)
- State update → WebSocket push: <1ms
- HTTP query response: <1ms

**Scalability:**
- Entities: Limited by memory (1M-10M typical)
- Properties per entity: No hard limit
- Subscribers: Limited by CPU/network
- Events: Limited by NATS storage

**Future improvements:**
- Sharding for horizontal scale
- Snapshot persistence for fast recovery
- Read replicas for query scaling

---

## Deployment Architecture

**Phase 1 (Docker Compose):**

```
┌─────────────────────────────────────────┐
│  Host Machine (localhost)               │
│                                         │
│  ┌──────────────┐  ┌─────────────────┐ │
│  │   Flux       │  │   NATS          │ │
│  │              │  │                 │ │
│  │   Port 3000  │  │   Port 4222     │ │
│  │   (HTTP/WS)  │  │   (internal)    │ │
│  │              │  │                 │ │
│  │              │  │   Port 8223     │ │
│  │              │  │   (monitoring)  │ │
│  └──────┬───────┘  └────────┬────────┘ │
│         │                   │          │
│         └───────────────────┘          │
│              (internal network)        │
└─────────────────────────────────────────┘

Clients connect to: localhost:3000
```

**Future deployment (Kubernetes):**
- StatefulSet for Flux (persistent state)
- Deployment for NATS (separate service)
- Ingress for external access
- Horizontal pod autoscaling

---

## Limitations & Future Work

**Phase 1 limitations:**

- No snapshot persistence (state rebuilds from events on restart)
- In-memory state only (memory-bounded)
- Single instance (no horizontal scaling)
- No authentication/authorization
- Basic error handling

**Planned improvements:**

- **Phase 2:** Snapshots, auth, replay from arbitrary point
- **Phase 3:** Sharding, multi-tenancy, advanced queries
- **Future:** Read replicas, CDC, external indexes
