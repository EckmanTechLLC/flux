# Phase 1 Task 5: WebSocket Subscription API

**Date:** 2026-02-11
**Status:** ✅ Complete
**Branch:** main

---

## Objective

Implement WebSocket API for state subscriptions, connecting state engine to NATS events.

---

## Implementation

### 1. Dependencies Added (Cargo.toml)
- `axum` with "ws" feature for WebSocket support
- `tokio-stream` for async stream utilities
- `futures` for async utilities

### 2. State Engine Integration (src/state/engine.rs)

**Added `process_event()` method:**
- Extracts `entity_id` from event payload
- Extracts `properties` object from event payload
- Calls `update_property()` for each property
- Domain-agnostic: works with any payload structure

**Expected payload format:**
```json
{
  "entity_id": "...",
  "properties": {
    "prop1": value1,
    "prop2": value2
  }
}
```

**Added `run_subscriber()` method:**
- Creates durable NATS consumer "flux-state-engine"
- Subscribes to "flux.events.>" stream
- Deserializes FluxEvent messages
- Calls `process_event()` for each event
- Acknowledges messages after processing
- Runs as background task

### 3. Subscription Module (src/subscription/)

**Created module structure:**
- `src/subscription/mod.rs` - Module exports
- `src/subscription/protocol.rs` - WebSocket message types
- `src/subscription/manager.rs` - Connection lifecycle

**Protocol (src/subscription/protocol.rs):**
- `ClientMessage::Subscribe { entity_id }` - Client subscribes to entity
- `ClientMessage::Unsubscribe { entity_id }` - Client unsubscribes from entity
- `StateUpdateMessage` - Server pushes state updates
- `ErrorMessage` - Server error responses

**Connection Manager (src/subscription/manager.rs):**
- `ConnectionManager` - Manages single WebSocket connection
- Maintains set of subscribed entity IDs
- Filters state updates based on subscriptions
- If no subscriptions, forwards all updates
- Handles WebSocket lifecycle (messages, pings, close)
- Handles broadcast channel lag gracefully

### 4. WebSocket API (src/api/websocket.rs)

**WebSocket endpoint:**
- `GET /api/ws` - WebSocket upgrade handler
- Creates `ConnectionManager` for each connection
- Subscribes to state engine updates
- Handles connection lifecycle

**State shared:**
- `WsAppState { state_engine }` - Shared state for WebSocket handlers

### 5. Main Application (src/main.rs)

**Initialization sequence:**
1. Connect to NATS
2. Create event publisher
3. Create state engine
4. Start state engine subscriber (background task)
5. Create ingestion API router (event publishing)
6. Create WebSocket API router (state subscriptions)
7. Merge routers
8. Start HTTP server

**Key changes:**
- StateEngine created as `Arc<StateEngine>`
- Subscriber runs in `tokio::spawn()` background task
- Two separate app states: `AppState` (ingestion) and `WsAppState` (WebSocket)
- Routers merged for single server on port 3000

---

## Files Created

1. `src/subscription/mod.rs` - Subscription module exports
2. `src/subscription/protocol.rs` - WebSocket message protocol
3. `src/subscription/manager.rs` - WebSocket connection manager
4. `src/api/websocket.rs` - WebSocket upgrade handler

---

## Files Modified

1. `Cargo.toml` - Added WebSocket and async dependencies
2. `src/state/engine.rs` - Added event processing and NATS subscriber
3. `src/api/mod.rs` - Exported websocket module
4. `src/main.rs` - Wired up state engine, subscriber, and WebSocket API

---

## Test Results

**Build:**
```bash
cargo build
# ✅ Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.04s
```

**Manual test plan:**
1. Start NATS: `docker compose up -d nats`
2. Start Flux: `cargo run`
3. Connect WebSocket client to `ws://localhost:3000/api/ws`
4. Send subscribe message: `{"type": "subscribe", "entity_id": "test_entity"}`
5. Publish event via HTTP:
   ```bash
   curl -X POST http://localhost:3000/api/events \
     -H "Content-Type: application/json" \
     -d '{
       "stream": "test.events",
       "source": "test_client",
       "timestamp": 1673450000000,
       "payload": {
         "entity_id": "test_entity",
         "properties": {
           "value": 42
         }
       }
     }'
   ```
6. Verify WebSocket client receives state update:
   ```json
   {
     "type": "state_update",
     "entity_id": "test_entity",
     "property": "value",
     "value": 42,
     "timestamp": "2026-02-11T..."
   }
   ```

---

## Architecture Flow

```
Producer → HTTP POST /api/events
              ↓
      Event Ingestion (validate, UUIDv7)
              ↓
      NATS Stream (flux.events.>)
              ↓
    State Engine Subscriber (run_subscriber)
              ↓
    State Engine (process_event → update_property)
              ↓
    Broadcast Channel (StateUpdate)
              ↓
    Connection Manager (filter by subscription)
              ↓
    WebSocket Client (state_update message)
```

---

## Key Design Decisions

1. **Domain-Agnostic Payload:**
   - Events require `entity_id` and `properties` in payload
   - No validation of property names or values
   - Works with any domain (sensors, users, inventory, etc.)

2. **Subscription Filtering:**
   - Clients can subscribe to specific entities
   - No subscriptions = receive all updates
   - Per-connection subscription management

3. **State Broadcast:**
   - Single broadcast channel for all state updates
   - Each WebSocket connection subscribes to channel
   - Channel handles lag gracefully (skips old updates)

4. **Error Handling:**
   - Malformed events logged and skipped (not fatal)
   - Missing `entity_id` or `properties` logged as warnings
   - WebSocket errors close connection gracefully

5. **Durable Consumer:**
   - NATS consumer named "flux-state-engine"
   - Survives Flux restarts (resumes from last ack)
   - Ensures no events are lost

---

## Next Steps

**Task 6: HTTP Query API & Integration**
- GET /api/state/entities - List all entities
- GET /api/state/entities/:id - Get specific entity
- Update docker-compose.yml with Flux service
- Integration testing
- Documentation

---

## Notes

- Clean build with no warnings
- All code follows existing patterns from state engine (Task 3)
- WebSocket lifecycle handles edge cases (lag, close, errors)
- State engine subscriber runs independently of HTTP server
- Ready for integration testing once NATS is running
