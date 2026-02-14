# Flux Design Document

**Version:** 1.0
**Date:** 2026-02-10
**Status:** Historical (Project Complete 2026-02-14)

> **Note:** This is the original design document. The project is now complete and deployed.
> See `/CLAUDE.md` for current status and `/docs/architecture.md` for implemented architecture.
> Some features described as "future work" were intentionally not implemented.

---

## What Flux Is

**Flux is a persistent, shared, event-sourced world state engine.**

It ingests immutable events, derives live in-memory relational state from them, and exposes that evolving world to agents, services, and humans through subscriptions and replay.

**Critical distinction:** Flux owns state derivation and persistence semantics, not just event forwarding. Consumers receive state updates from Flux, not raw events. Flux maintains the canonical world state.

**Core characteristics:**
- Event-sourced: State is derived from immutable events
- Persistent: Events stored, state survives restarts
- Shared: Multiple systems observe the same world state
- Real-time: Updates propagate immediately to subscribers
- Replay-capable: Can reprocess history from any point
- Domain-agnostic: Works for any use case without encoding domain semantics
- State-owning: Flux derives and maintains state, consumers observe it

---

## What Flux Is NOT

To maintain focus and clarity:

- ❌ Just a message broker (it maintains state, not just routes messages)
- ❌ Just an event log (it derives current state from events)
- ❌ An event forwarder (consumers don't process events themselves, they observe Flux's state)
- ❌ Domain-specific (not for agents, SCADA, or any single use case)
- ❌ A query database (state is derived, not indexed for complex queries)
- ❌ A decision engine (no business logic, workflows, or interpretation)
- ❌ A protocol adapter (doesn't integrate with specific systems)

**Key principle:** Flux owns state semantics. Consumers observe Flux's canonical state, not raw events.

Flux is infrastructure. Applications define semantics.

---

## State Ownership Model

### Flux Owns State, Not Just Events

**What Flux does:**
1. **Ingests events** - Validates, persists immutable events
2. **Derives state** - Maintains canonical in-memory state from events
3. **Persists state** - Snapshots + event replay for recovery
4. **Exposes state** - Consumers subscribe to state updates, not raw events

**Contrast: Event forwarding (what Flux is NOT):**
```
Producer → Flux (validate) → Message Bus → Consumer
                                              ↓
                                    Consumer derives own state
```
Problem: Every consumer reimplements state derivation. No shared world.

**Flux approach (state ownership):**
```
Producer → Flux (ingest) → Events (internal) → Flux State Engine
                                                       ↓
                                                 Canonical State
                                                       ↓
                                              Consumers (subscribe)
```
Benefit: Flux maintains one canonical world state. Consumers observe it.

**What consumers receive:**
- Not: Raw events to process
- Not: Events to derive state from
- Yes: State updates (entity property changes)
- Yes: Current state snapshots
- Yes: Replay of state changes

**Why this matters:**
- Single source of truth (Flux owns state)
- Consumers don't reimplement state logic
- Consistent world view across all observers
- Flux controls state semantics and persistence

---

## Architecture Overview

### Single Cohesive System

Flux is one system with multiple internal layers:

```
Producers
    ↓
┌─────────────────────────────────────────┐
│ Event Ingestion (validate, authorize)  │
│     ↓                                   │
│ NATS/JetStream (internal transport)    │
│     ↓                                   │
│ State Engine (event-sourced, in-memory)│
│     ↓                                   │
│ APIs (WebSocket, REST, client libs)    │
└─────────────────────────────────────────┘
    ↓
Consumers (subscribe, query, replay)
```

### Components

**1. Event Ingestion**
- Validates event envelope structure
- Generates event IDs (UUIDv7) if missing
- Authorizes publish requests
- Publishes to internal NATS streams

**2. Message Bus (Internal)**
- NATS with JetStream for persistence
- Implementation detail, not exposed externally
- Provides durability, replay, at-least-once delivery

**3. State Engine**
- Event-sourced in-memory state
- Generic entity/property model
- Derives state from events
- Broadcasts state changes
- Snapshot/replay for persistence

**4. Subscription APIs**
- WebSocket for real-time updates
- REST for queries and historical access
- Client libraries (Python, JS, Go, Rust)
- Replay from any point in time

---

## Event Model

### Fixed Envelope, Opaque Payload

Every event has a standard envelope:

**Required fields:**
- `eventId` - UUIDv7 (time-ordered, unique)
- `stream` - Logical namespace (e.g., "observations.sensors")
- `source` - Producer identity (who emitted it)
- `timestamp` - Unix epoch milliseconds (producer time)
- `payload` - Domain-specific data (opaque JSON)

**Optional fields:**
- `key` - Ordering/grouping hint (consumer metadata)
- `schema` - Schema name + version (metadata only, no validation)

**Domain-agnostic:**
- Flux validates envelope structure only
- Payload is opaque (not inspected or validated)
- Schema field is metadata (no enforcement)

---

## State Model

### Generic Entity/Property Model

Flux maintains state as generic entities with properties:

```
Entity {
  id: string
  properties: Map<string, value>
  last_updated: timestamp
}
```

**Domain-agnostic:**
- Flux doesn't know what entities represent (agents, sensors, tasks, players)
- Applications define entity semantics
- Properties are key-value pairs (any JSON value)
- No built-in entity types or schemas

**State derivation:**
- State is derived from events
- Events are the source of truth
- State can be rebuilt by replaying events
- Snapshots for fast recovery

---

## Client Experience

### Multiple Connection Options

**Primary: Client Libraries**
- Python, JavaScript, Go, Rust packages
- Handle connection, retries, authentication
- Clean API: publish, subscribe, query, replay
- Like database drivers (Redis, PostgreSQL)

**Alternative: WebSocket**
- For browsers, simple clients
- Real-time bidirectional communication
- JSON message protocol

**Alternative: HTTP REST**
- For simple integrations, curl-able
- Publish events, query state, basic operations
- Stateless, request/response

**Flexibility:**
- All options supported
- Client libraries recommended for production
- WebSocket/REST for accessibility

---

## Deployment Models

### Flexible Deployment

**Model 1: Private Flux (Self-hosted, local)**
- Run Flux locally (Docker, binary)
- Private world for personal agents/systems
- Complete control, no external access
- Like running your own database

**Model 2: Shared Flux (Cloud, team)**
- Deploy to cloud (AWS, GCP, DigitalOcean)
- Multiple systems/agents connect
- Shared world state, collaborative
- Like a shared database server

**Model 3: Multiple Instances (Different worlds)**
- Run multiple Flux servers
- Each is independent world
- Clients connect to different instances
- Move between worlds by changing connection URL

**Model 4: Multi-tenant Flux (Future)**
- One Flux instance, multiple isolated tenants
- Namespace isolation (streams, state)
- Authorization enforcement
- Like hosted database service (SaaS)

**Deployment flexibility:**
- Docker container (easiest)
- Binary (direct, lightweight)
- Cloud deployment (scalable)
- Future: Managed hosting

---

## Use Cases (Examples, Not Prescriptive)

Flux is domain-agnostic. Example applications:

**Multi-agent LLM systems:**
- Agents publish observations/actions as events
- Flux derives shared world state
- Agents subscribe to see others' actions
- Coordination through shared state

**Industrial SCADA:**
- Sensors/PLCs publish measurements
- Flux maintains equipment state
- Operators subscribe to real-time updates
- Historical replay for diagnostics

**Virtual worlds/games:**
- Players publish actions (move, interact)
- Flux maintains game state
- Clients subscribe to see world changes
- Replay for time-travel debugging

**IoT platforms:**
- Devices publish telemetry
- Flux derives device state
- Applications subscribe to device updates
- Replay for analysis

**Collaborative systems:**
- Users publish edits/changes
- Flux maintains document/project state
- Collaborators see real-time updates
- History for undo/audit

**Key principle:** Flux doesn't encode these use cases. Applications bring the semantics.

---

## Core Principles

### 1. State Ownership
- Flux derives and maintains canonical world state
- Consumers observe state, not events
- Single source of truth for world model
- State semantics owned by Flux, not delegated

### 2. Domain Agnosticism
- No built-in entity types or schemas
- Payload is opaque
- Applications define semantics
- Works for any use case

### 2. Event Sourcing
- Events are immutable source of truth
- State is derived from events
- Can replay to rebuild state
- Time-travel capability

### 3. Shared World State
- All subscribers see same state
- Updates propagate in real-time
- Consistent view across systems
- Location-independent

### 4. Decoupled Architecture
- Producers don't know consumers
- Consumers don't know producers
- Coordination through shared state
- Message bus is internal detail

### 5. Flexible Deployment
- Run anywhere (local, cloud, hybrid)
- Multiple instances (different worlds)
- Multi-tenant capable (future)
- Like database infrastructure

### 6. Replay & Recovery
- Replay events from any point in time
- Snapshot/recovery for fast startup
- Audit trail of all events
- Debugging and analysis

### 7. Real-time Sync
- Immediate propagation of state changes
- WebSocket subscriptions
- Sub-second latency
- Scalable to thousands of subscribers

---

## Ordering Guarantees

**Per-stream ordering:**
- Events within a stream are totally ordered
- Timestamp + sequence number
- Replay preserves order

**Cross-stream:**
- No global ordering
- Independent streams
- Applications correlate via timestamp if needed

**State consistency:**
- State updates are atomic per entity
- Subscribers see consistent snapshots
- No torn reads

---

## Persistence & Durability

**Event persistence:**
- All events stored (NATS JetStream)
- Configurable retention (time, size, count)
- At-least-once delivery semantics
- Replay from beginning or any point

**State persistence:**
- Periodic snapshots (every N minutes)
- Snapshot + event replay for recovery
- Fast startup (restore snapshot + replay recent)
- State survives restarts

**Trade-offs:**
- Events are durable (disk storage)
- State is in-memory (performance)
- Snapshots for recovery (balance)

---

## Authorization & Multi-tenancy

**Phase 1: Basic authorization**
- Stream-level permissions (who can publish/subscribe)
- Simple authentication (tokens, certificates)
- Authorization checks on ingestion and subscription

**Phase 2: Multi-tenancy (future)**
- Tenant isolation (namespace streams)
- Tenant-specific authorization
- Cross-tenant coordination (if authorized)

---

## Technology Stack

**Flux Service:**
- Language: Go (ingestion, APIs) + Rust (state engine)
- Reason: Go for network services, Rust for in-memory performance

**Message Bus:**
- NATS with JetStream
- Internal only (not exposed)
- High performance, persistence, replay

**State Storage:**
- In-memory (DashMap or similar)
- Snapshots to disk
- Fast reads, consistent writes

**Client Libraries:**
- Python, JavaScript, Go, Rust
- Consistent API across languages
- Community-contributed for other languages

---

## Design Constraints

**What Flux optimizes for:**
- ✅ Real-time state propagation
- ✅ Replay capability
- ✅ Domain agnosticism
- ✅ Deployment flexibility
- ✅ Developer experience

**What Flux does not optimize for:**
- ⚠️ Complex queries (use external indexes)
- ⚠️ Exactly-once semantics (at-least-once)
- ⚠️ Cross-stream transactions (stream-level atomicity only)
- ⚠️ Massive state (in-memory limits apply)

**Scalability:**
- Thousands of events/second (single instance)
- Thousands of subscribers (single instance)
- Millions of entities (memory-limited)
- Future: Sharding for horizontal scale

---

## Relationship to flux-reactor

**flux-reactor** is a SCADA-specific implementation of similar concepts:
- Event-sourced state engine
- Real-time updates
- WebSocket API

**Flux** extracts and generalizes:
- Domain-agnostic (not SCADA-specific)
- Enforced event model (envelope structure)
- Multi-tenant capable
- Flexible deployment

**Flux can power systems like flux-reactor:**
- SCADA systems are one use case
- Agent coordination is another
- Virtual worlds, IoT, etc. are others

---

## Success Criteria

**Flux succeeds if:**
- Developers can deploy Flux in minutes (Docker)
- Agents/systems can publish/subscribe easily (client libraries)
- Multiple use cases emerge (domain agnostic works)
- State updates propagate in real-time (sub-second)
- Replay works reliably (debugging, catch-up)
- Community adopts for diverse applications

**Flux fails if:**
- It becomes domain-specific (loses generality)
- It's hard to deploy or use
- Performance is inadequate for real-time
- It tries to do too much (scope creep)

---

## Next Steps

**Phase 1: Foundation (Current)**
- Event ingestion with validation
- Stream management
- Publish API
- Basic state engine (in-memory)
- WebSocket subscriptions
- Client libraries (Python initially)

**Phase 2: Production**
- Snapshot/recovery
- Replay from any point
- Authorization framework
- REST API
- Client libraries (JS, Go, Rust)
- Performance optimization

**Phase 3: Scale**
- Multi-tenancy
- Sharding (horizontal scale)
- Advanced queries
- Monitoring/observability
- Managed hosting option

---

## Revision History

- **2026-02-10:** Initial design document - Foundation principles established
