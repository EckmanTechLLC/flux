# Flux Context for Claude

**Last Updated:** 2026-02-11
**Current Phase:** Fresh Start - State Engine Architecture

---

## Quick Reference

**Project:** Flux - Persistent, shared, event-sourced world state engine
**Architecture:** Event ingestion → NATS (internal) → State Engine → WebSocket API
**Key Docs:**
- `/FLUX-DESIGN.md` - Complete vision and design principles
- `/docs/workflow/multi-session-workflow.md` - Development workflow
- Reference: `/projects/flux-reactor/` - SCADA implementation with similar patterns

---

## User Preferences (Apply Always)

- **Never be verbose** - clear, concise responses
- **Small, incremental changes** - don't build everything at once
- **Ask before expanding scope** - stay within defined task
- **Document as you go** - ADRs for decisions, session notes for progress
- **Show, don't just talk about it**
- **Always check the codebase** - read files, never rely on memory
- **Research before suggesting** - check docs/libraries, don't guess
- **Plan before implementing** - get approval on approach first

---

## Current Status

### Phase 1: State Engine MVP (IN PROGRESS 🟡)
- [x] Git repository initialized
- [x] Previous work archived (branch: archive/event-backbone)
- [x] Clean slate for state engine implementation
- [x] ADR-001: State Engine Architecture
- [x] Task 1: Project structure & dependencies
- [x] Task 2: Event model & validation
- [ ] Task 3: State engine core
- [ ] Task 4: Event ingestion API
- [ ] Task 5: WebSocket subscription API
- [ ] Task 6: HTTP query API & integration

**Infrastructure Ports:**
- NATS (internal): 4222
- NATS monitoring: 8223
- Flux WebSocket API: 3000
- Flux HTTP API: 3000

---

## What Flux IS

**Flux is a persistent, shared, event-sourced world state engine.**

It ingests immutable events, derives live in-memory state from them, and exposes that evolving world to agents, services, and humans through subscriptions and replay.

**Critical Distinction:** Flux owns state derivation and persistence semantics, not just event forwarding. Consumers receive state updates from Flux, not raw events. Flux maintains the canonical world state.

**Core Characteristics:**
- **Event-sourced** - State is derived from immutable events
- **Persistent** - Events stored, state survives restarts
- **Shared** - Multiple systems observe the same world state
- **Real-time** - Updates propagate immediately to subscribers
- **Replay-capable** - Can reprocess history from any point
- **Domain-agnostic** - Works for any use case without encoding domain semantics
- **State-owning** - Flux derives and maintains state, consumers observe it

---

## What Flux IS NOT

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

## Key Decisions

**Architecture Pattern (from flux-reactor):**
- Event ingestion → NATS (internal) → State Engine → Subscription APIs
- NATS is internal transport, NOT exposed to consumers
- State engine in Rust (performance, safety, no GC)
- Generic entity/property model (domain-agnostic)
- WebSocket for real-time state updates
- HTTP REST for queries and operations

**Technology Stack:**
- **State Engine:** Rust (DashMap for lock-free reads, Tokio async)
- **Event Transport (Internal):** NATS with JetStream
- **APIs:** Rust with Axum framework
- **Persistence:** Snapshots + event replay
- **Deployment:** Docker Compose (initial), Kubernetes (future)
- **Client Libraries:** Python, JavaScript (future)

**Event Model:**
- Fixed envelope: `eventId`, `stream`, `source`, `timestamp`, `key`, `schema`, `payload`
- UUIDv7 event IDs (time-ordered)
- Opaque payload (domain-agnostic)
- Events stored in NATS (internal only)

**State Model:**
- Generic entities with properties: `Entity { id, properties: Map<string, value>, last_updated }`
- No built-in entity types or schemas
- State derived from events
- Snapshots + event replay for persistence

**Related Projects:**
- `/projects/flux-reactor/` - SCADA-specific implementation with similar state engine patterns
- Flux generalizes flux-reactor's state engine for any domain

**Development Environment:**
- This is a shared dev server with multiple active projects
- Must not disrupt other running services
- Use specific ports, document resource usage
- Easy to start/stop without affecting other projects
- Docker Compose must be non-invasive

---

## What NOT to Do

- Don't implement without explicit plan approval
- Don't guess at solutions - research first (check flux-reactor for patterns)
- Don't use memory - always read files
- Don't add features not requested
- Don't be verbose in responses or documentation
- Don't interpret event payloads (Flux is payload-agnostic)
- Don't add schema validation (out of scope)
- Don't expose NATS to consumers (internal only)
- Don't make Flux domain-specific (keep it generic)

---

## Implementation Session Checklist

**For EVERY code change, follow this checklist:**

### 1. READ FIRST
- [ ] Read CLAUDE.md (this file) to understand context
- [ ] Read relevant ADR(s) for the feature area
- [ ] Read existing code files you'll modify
- [ ] Verify your understanding of current state

### 2. VERIFY ASSUMPTIONS
- [ ] Check if files/functions exist where you expect
- [ ] Verify API endpoints/interfaces match your assumptions
- [ ] Confirm dependencies are available
- [ ] List what you'll change BEFORE making changes

### 3. MAKE CHANGES
- [ ] Make ONE logical change at a time
- [ ] Keep changes small and focused
- [ ] Follow existing code patterns and style
- [ ] Add/update tests for changed behavior
- [ ] Update relevant documentation

### 4. TEST & VERIFY
- [ ] Tests pass, no regressions
- [ ] Document expected test results
- [ ] Provide test commands for user to run

### 5. DOCUMENT
- [ ] Update session notes with what was done
- [ ] Note any issues encountered and how resolved
- [ ] List files created/modified
- [ ] Update CLAUDE.md if status changed

### 6. REPORT
- [ ] Provide concise summary to user
- [ ] Report any blockers or questions
- [ ] Confirm scope was not exceeded
- [ ] List next steps if applicable

**If you skip any step, you're doing it wrong.**

---

## Session Start Protocol

1. **User provides task**: Specific, bounded scope
2. **You confirm**: Restate task, list files you'll touch, ask for approval
3. **You work**: Follow checklist above
4. **You report**: Summary + session notes location
5. **Foundation verifies**: Reads files, confirms correctness
6. **Next task**: Foundation provides prompt for next session

---

## Phase 1 Scope (State Engine MVP)

**Goal:** Working state engine that derives and exposes state from events

**Core Deliverables:**
1. **State Engine Core** (Rust)
   - In-memory state storage (DashMap, entity/property model)
   - Event ingestion (validate, generate UUIDv7, persist to NATS)
   - State derivation (read events from NATS, apply to state)
   - State change broadcasting

2. **Subscription API** (Rust/Axum)
   - WebSocket API for real-time state updates
   - Subscribe to entities/properties
   - Receive state change events (not raw events)
   - HTTP REST for queries (get entity, get all entities)

3. **Infrastructure**
   - Docker Compose (Flux + NATS, non-invasive)
   - NATS internal only (not exposed)
   - Port allocation (avoid conflicts)
   - Easy start/stop

4. **Client Examples** (Python)
   - Publish events (HTTP or WebSocket)
   - Subscribe to state updates (WebSocket)
   - Query current state (HTTP)

5. **Documentation**
   - Getting Started (run Flux, publish events, observe state)
   - API documentation (WebSocket messages, HTTP endpoints)
   - State model specification
   - Architecture diagram

**What We're NOT Building (Phase 1):**
- ❌ Snapshot/recovery (state rebuilds from events on restart)
- ❌ Advanced auth (basic or none)
- ❌ Client SDK libraries (examples only)
- ❌ Replay from arbitrary point (start from beginning only)
- ❌ Monitoring dashboard
- ❌ Multi-tenancy

**Language Decision:**
- **State Engine + APIs:** Rust (based on flux-reactor patterns)
- **Client Examples:** Python

---

## Architecture: State Engine Pattern

**Flux is a state engine**, not an event forwarder.

**Architecture Flow:**
```
Producer → Event Ingestion (validate, persist)
                ↓
           NATS Stream (internal)
                ↓
        State Derivation (read events, apply to state)
                ↓
          State Engine (in-memory, DashMap)
                ↓
      Subscription Manager (broadcast state changes)
                ↓
           WebSocket API
                ↓
    Consumers (receive state updates)
```

**Key Components:**

**1. Event Ingestion**
- Validates event envelope
- Generates UUIDv7 if missing
- Publishes to NATS (internal stream)
- Returns confirmation to producer

**2. NATS (Internal Transport)**
- Persists events (JetStream)
- NOT exposed to consumers
- State engine reads from it
- Enables replay and recovery

**3. State Engine**
- In-memory state (Rust DashMap)
- Generic entity/property model
- Subscribes to NATS event stream
- Applies events to derive state
- Broadcasts state changes

**4. Subscription Manager**
- Manages WebSocket connections
- Filters state updates per subscription
- Pushes state changes (not raw events)

**5. APIs**
- WebSocket: Real-time state updates
- HTTP REST: Query current state

**Key Principle:** Consumers observe Flux's state, they never see raw events.

**Why this matters:**
- Single source of truth (Flux owns state)
- Consumers don't reimplement state logic
- Consistent world view across all observers
- State semantics controlled by Flux

---

## Known Issues

None (fresh start)

## Previous Work

Previous implementation (event backbone approach) archived in Git:
- Branch: `archive/event-backbone`
- Reason: Created tech debt (wrong consumer interface, no state engine)
- Pivot: Building state engine from scratch using flux-reactor patterns

---

## Code Quality Standards

**General:**
- No TODOs in code (track in issues/ADRs)
- No commented-out code
- Clear variable/function names
- Keep functions small (<100 lines)

**Example Code (Python/Go/Rust):**
- Follow language conventions
- Include error handling
- Add inline comments for complex logic
- Provide usage examples in README
