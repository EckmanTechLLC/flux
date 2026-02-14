# Flux API Reference

**Status:** Phase 1 Implementation
**Last Updated:** 2026-02-11

---

## Overview

Flux exposes two primary APIs:

- **HTTP REST API** - Event ingestion and state queries
- **WebSocket API** - Real-time state subscriptions

**Base URL (local):** `http://localhost:3000` / `ws://localhost:3000`

---

## HTTP REST API

### Event Ingestion

#### POST /api/events

Publish a single event to Flux.

**Request:**

```http
POST /api/events HTTP/1.1
Content-Type: application/json

{
  "stream": "sensors",
  "source": "sensor-01",
  "payload": {
    "entity_id": "temp-sensor-01",
    "properties": {
      "temperature": 22.5,
      "unit": "celsius"
    }
  }
}
```

**Request fields:**

- `eventId` (optional) - UUIDv7 identifier. Auto-generated if omitted.
- `stream` (required) - Logical namespace (e.g., "sensors", "observations")
- `source` (required) - Producer identity (e.g., "sensor-01", "agent-42")
- `timestamp` (optional) - Unix epoch milliseconds. Defaults to current time if omitted.
- `key` (optional) - Grouping/ordering key
- `schema` (optional) - Schema metadata (not validated)
- `payload` (required) - Event data (must be JSON object)

**Payload structure for state derivation:**

For Flux to update state, payload must include:
```json
{
  "entity_id": "entity-identifier",
  "properties": {
    "property1": "value1",
    "property2": "value2"
  }
}
```

**Response (200 OK):**

```json
{
  "eventId": "01933d7a-1234-7890-abcd-ef1234567890",
  "stream": "sensors"
}
```

**Error responses:**

```json
// 400 Bad Request - Invalid event envelope
{
  "error": "Validation error: missing required field 'stream'"
}

// 400 Bad Request - Invalid stream name
{
  "error": "Validation error: stream must be lowercase with optional dots"
}

// 500 Internal Server Error - NATS publish failure
{
  "error": "Failed to publish event to NATS"
}
```

**curl example:**

```bash
curl -X POST http://localhost:3000/api/events \
  -H "Content-Type: application/json" \
  -d '{
    "stream": "sensors",
    "source": "sensor-01",
    "payload": {
      "entity_id": "temp-sensor-01",
      "properties": {
        "temperature": 22.5
      }
    }
  }'
```

---

#### POST /api/events/batch

Publish multiple events in a single request.

**Request:**

```http
POST /api/events/batch HTTP/1.1
Content-Type: application/json

{
  "events": [
    {
      "stream": "sensors",
      "source": "sensor-01",
      "payload": {
        "entity_id": "temp-sensor-01",
        "properties": {"temperature": 22.5}
      }
    },
    {
      "stream": "sensors",
      "source": "sensor-02",
      "payload": {
        "entity_id": "temp-sensor-02",
        "properties": {"temperature": 23.0}
      }
    }
  ]
}
```

**Request fields:**

- `events` (required) - Array of FluxEvent objects (same structure as POST /api/events)

**Response (200 OK):**

```json
{
  "successful": 2,
  "failed": 0,
  "results": [
    {
      "eventId": "01933d7a-1234-7890-abcd-ef1234567890",
      "stream": "sensors"
    },
    {
      "eventId": "01933d7a-1234-7890-abcd-ef1234567891",
      "stream": "sensors"
    }
  ]
}
```

**Partial success:**

If some events fail validation, successful events are still processed:

```json
{
  "successful": 1,
  "failed": 1,
  "results": [
    {
      "eventId": "01933d7a-1234-7890-abcd-ef1234567890",
      "stream": "sensors"
    },
    {
      "error": "Validation error: missing required field 'stream'"
    }
  ]
}
```

**curl example:**

```bash
curl -X POST http://localhost:3000/api/events/batch \
  -H "Content-Type: application/json" \
  -d '{
    "events": [
      {
        "stream": "sensors",
        "source": "sensor-01",
        "payload": {
          "entity_id": "temp-sensor-01",
          "properties": {"temperature": 22.5}
        }
      },
      {
        "stream": "sensors",
        "source": "sensor-02",
        "payload": {
          "entity_id": "temp-sensor-02",
          "properties": {"temperature": 23.0}
        }
      }
    ]
  }'
```

---

### State Query

#### GET /api/state/entities

List all entities in current state.

**Request:**

```http
GET /api/state/entities HTTP/1.1
```

**Response (200 OK):**

```json
[
  {
    "id": "temp-sensor-01",
    "properties": {
      "temperature": 22.5,
      "unit": "celsius"
    },
    "last_updated": "2026-02-11T10:30:45.123Z"
  },
  {
    "id": "temp-sensor-02",
    "properties": {
      "temperature": 23.0,
      "unit": "celsius"
    },
    "last_updated": "2026-02-11T10:31:12.456Z"
  }
]
```

**Empty state:**

```json
[]
```

**curl example:**

```bash
curl http://localhost:3000/api/state/entities
```

---

#### GET /api/state/entities/:id

Get a specific entity by ID.

**Request:**

```http
GET /api/state/entities/temp-sensor-01 HTTP/1.1
```

**Response (200 OK):**

```json
{
  "id": "temp-sensor-01",
  "properties": {
    "temperature": 22.5,
    "unit": "celsius",
    "status": "active"
  },
  "last_updated": "2026-02-11T10:30:45.123Z"
}
```

**Error responses:**

```json
// 404 Not Found - Entity doesn't exist
{
  "error": "Entity not found: temp-sensor-99"
}
```

**curl example:**

```bash
curl http://localhost:3000/api/state/entities/temp-sensor-01
```

---

### Entity Management

#### DELETE /api/state/entities/:id

Delete a single entity by ID.

**Request:**

```http
DELETE /api/state/entities/temp-sensor-01 HTTP/1.1
Authorization: Bearer <token>  # Only if auth enabled
```

**Response (200 OK):**

```json
{
  "entity_id": "temp-sensor-01",
  "eventId": "019c5c88-5386-7ae0-ab4d-80a8c1ce631a"
}
```

**Authorization:**
- Auth disabled: Anyone can delete
- Auth enabled: Must provide bearer token that owns the entity's namespace

**curl example:**

```bash
# Without auth
curl -X DELETE http://localhost:3000/api/state/entities/temp-sensor-01

# With auth
curl -X DELETE http://localhost:3000/api/state/entities/matt/sensor-01 \
  -H "Authorization: Bearer <your-token>"
```

---

#### POST /api/state/entities/delete

Batch delete entities by filter.

**Request:**

```http
POST /api/state/entities/delete HTTP/1.1
Content-Type: application/json
Authorization: Bearer <token>  # Only if auth enabled

{
  "prefix": "loadtest-"
}
```

**Filter options (choose one):**

```json
// Delete by namespace
{"namespace": "matt"}

// Delete by prefix
{"prefix": "loadtest-"}

// Delete specific entities
{"entity_ids": ["id1", "id2", "id3"]}
```

**Response (200 OK):**

```json
{
  "deleted": 3,
  "failed": 0,
  "errors": []
}
```

**Limits:**
- Maximum batch size: 10,000 entities (configurable via `max_batch_delete`)

**Authorization:**
- Auth disabled: Can delete any entities matching filter
- Auth enabled: Can only delete entities in owned namespace

**curl example:**

```bash
# Delete by prefix
curl -X POST http://localhost:3000/api/state/entities/delete \
  -H "Content-Type: application/json" \
  -d '{"prefix": "loadtest-"}'

# Delete by namespace (with auth)
curl -X POST http://localhost:3000/api/state/entities/delete \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <your-token>" \
  -d '{"namespace": "matt"}'
```

---

## WebSocket API

### Connection

**Endpoint:** `GET /api/ws`

Upgrade HTTP connection to WebSocket.

**JavaScript example:**

```javascript
const ws = new WebSocket('ws://localhost:3000/api/ws');

ws.onopen = () => {
  console.log('Connected to Flux');
};

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  console.log('Received:', message);
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = () => {
  console.log('Disconnected from Flux');
};
```

**Python example:**

```python
import asyncio
import websockets
import json

async def connect():
    uri = "ws://localhost:3000/api/ws"
    async with websockets.connect(uri) as websocket:
        print("Connected to Flux")

        # Subscribe
        await websocket.send(json.dumps({
            "type": "subscribe",
            "entity_id": "temp-sensor-01"
        }))

        # Receive updates
        async for message in websocket:
            data = json.loads(message)
            print(f"Received: {data}")

asyncio.run(connect())
```

---

### Messages

#### Client → Server: Subscribe

Subscribe to updates for a specific entity.

**Message:**

```json
{
  "type": "subscribe",
  "entity_id": "temp-sensor-01"
}
```

**Fields:**

- `type` (required) - Must be "subscribe"
- `entity_id` (required) - Entity ID to subscribe to (use "*" for all entities)

**Effect:**

- Client will receive `state_update` messages when entity properties change
- Multiple subscriptions allowed (send multiple subscribe messages)

**JavaScript example:**

```javascript
ws.send(JSON.stringify({
  type: 'subscribe',
  entity_id: 'temp-sensor-01'
}));

// Subscribe to all entities
ws.send(JSON.stringify({
  type: 'subscribe',
  entity_id: '*'
}));
```

---

#### Client → Server: Unsubscribe

Stop receiving updates for a specific entity.

**Message:**

```json
{
  "type": "unsubscribe",
  "entity_id": "temp-sensor-01"
}
```

**Fields:**

- `type` (required) - Must be "unsubscribe"
- `entity_id` (required) - Entity ID to unsubscribe from

**Effect:**

- Client stops receiving updates for that entity
- Other subscriptions unaffected

**JavaScript example:**

```javascript
ws.send(JSON.stringify({
  type: 'unsubscribe',
  entity_id: 'temp-sensor-01'
}));
```

---

#### Server → Client: State Update

State update notification sent when subscribed entity property changes.

**Message:**

```json
{
  "type": "state_update",
  "entity_id": "temp-sensor-01",
  "property": "temperature",
  "value": 22.5,
  "timestamp": "2026-02-14T10:30:45.123Z"
}
```

**Fields:**

- `type` - Always "state_update"
- `entity_id` - Entity identifier
- `property` - Property name that changed
- `value` - New property value
- `timestamp` - Update timestamp

**When sent:**

- Whenever any property of a subscribed entity changes
- One message per property update (not batched)

**Note:** Updates are per-property, not full entity state.

**JavaScript handler:**

```javascript
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.type === 'state_update') {
    console.log(`${message.entity_id}.${message.property} = ${message.value}`);
  }
};
```

---

#### Server → Client: Metrics Update

Real-time metrics broadcast (every 2 seconds by default).

**Message:**

```json
{
  "type": "metrics_update",
  "timestamp": "2026-02-14T14:30:45.123Z",
  "entities": {
    "total": 1543
  },
  "events": {
    "total": 458392,
    "rate_per_second": 45.2
  },
  "websocket": {
    "connections": 3
  },
  "publishers": {
    "active": 12
  }
}
```

**Fields:**

- `type` - Always "metrics_update"
- `timestamp` - Server timestamp
- `entities.total` - Current entity count
- `events.total` - Total events processed
- `events.rate_per_second` - Current event rate (5-second window)
- `websocket.connections` - Active WebSocket connections
- `publishers.active` - Active publishers (within configured window)

**JavaScript handler:**

```javascript
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.type === 'metrics_update') {
    console.log(`Entities: ${message.entities.total}`);
    console.log(`Event rate: ${message.events.rate_per_second} eps`);
    console.log(`Active publishers: ${message.publishers.active}`);
  }
};
```

---

#### Server → Client: Entity Deleted

Notification when an entity is deleted.

**Message:**

```json
{
  "type": "entity_deleted",
  "entity_id": "temp-sensor-01",
  "timestamp": "2026-02-14T14:30:45.123Z"
}
```

**Fields:**

- `type` - Always "entity_deleted"
- `entity_id` - ID of deleted entity
- `timestamp` - Deletion timestamp

**When sent:**

- When entity is deleted via DELETE API
- Sent to all connected WebSocket clients

**JavaScript handler:**

```javascript
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.type === 'entity_deleted') {
    console.log(`Entity ${message.entity_id} was deleted`);
    // Remove from local cache/UI
  }
};
```

---

### Usage Patterns

#### Pattern 1: Subscribe and Stream

```javascript
// Connect and subscribe
const ws = new WebSocket('ws://localhost:3000/api/ws');

ws.onopen = () => {
  // Subscribe to multiple entities
  ws.send(JSON.stringify({
    type: 'subscribe',
    entity_id: 'temp-sensor-01'
  }));

  ws.send(JSON.stringify({
    type: 'subscribe',
    entity_id: 'temp-sensor-02'
  }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'update') {
    updateUI(msg.entity);
  }
};
```

#### Pattern 2: Snapshot + Subscription

```javascript
// 1. Get initial snapshot via HTTP
const response = await fetch('http://localhost:3000/api/state/entities');
const entities = await response.json();
renderEntities(entities);

// 2. Subscribe via WebSocket for updates
const ws = new WebSocket('ws://localhost:3000/api/ws');

ws.onopen = () => {
  entities.forEach(entity => {
    ws.send(JSON.stringify({
      type: 'subscribe',
      entity_id: entity.id
    }));
  });
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'update') {
    updateEntity(msg.entity);
  }
};
```

#### Pattern 3: Publish and Subscribe

```javascript
// Subscribe first
ws.onopen = () => {
  ws.send(JSON.stringify({
    type: 'subscribe',
    entity_id: 'my-entity'
  }));
};

// Publish event via HTTP
async function updateEntity(properties) {
  await fetch('http://localhost:3000/api/events', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({
      stream: 'myapp',
      source: 'client-01',
      payload: {
        entity_id: 'my-entity',
        properties: properties
      }
    })
  });

  // Will receive update via WebSocket automatically
}

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'update') {
    console.log('State updated:', msg.entity);
  }
};
```

---

## Error Handling

### HTTP API Errors

**400 Bad Request**
- Invalid JSON body
- Missing required fields
- Invalid field values
- Malformed event envelope

**404 Not Found**
- Entity doesn't exist (GET /api/state/entities/:id)

**500 Internal Server Error**
- NATS connection failure
- State engine error
- Unexpected server error

**Error response format:**

```json
{
  "error": "Human-readable error message"
}
```

### WebSocket Errors

**Connection failures:**
- Network timeout
- Server unavailable
- Invalid WebSocket upgrade

**Runtime errors:**
- Invalid JSON message
- Unknown message type
- Malformed message structure

**Handling in JavaScript:**

```javascript
ws.onerror = (error) => {
  console.error('WebSocket error:', error);
  // Implement reconnection logic
};

ws.onclose = (event) => {
  if (!event.wasClean) {
    console.error('Connection closed unexpectedly');
    // Reconnect after delay
    setTimeout(() => reconnect(), 5000);
  }
};
```

---

## Rate Limits

**Phase 1:** No rate limiting implemented.

**Future phases:**
- Per-client event ingestion limits
- Per-client subscription limits
- Token-based quotas

---

## Authentication & Authorization

**Phase 1:** No authentication.

**Future phases:**
- Token-based authentication (JWT, API keys)
- Stream-level authorization
- Role-based access control

---

## Best Practices

### Event Publishing

1. **Include meaningful source:** Identify the producer clearly
2. **Use consistent stream names:** Namespace by domain (e.g., "sensors.temperature")
3. **Always include entity_id in payload:** Required for state derivation
4. **Include all properties:** Properties are replaced, not merged
5. **Use batch API for multiple events:** Better performance

### State Subscription

1. **Get snapshot first:** HTTP GET before WebSocket subscribe for initial state
2. **Subscribe selectively:** Only subscribe to entities you need
3. **Handle reconnections:** WebSocket may disconnect, implement retry logic
4. **Unsubscribe when done:** Free server resources

### State Queries

1. **Use WebSocket for real-time:** HTTP is for snapshot only
2. **Cache locally:** Minimize repeated queries
3. **Check last_updated:** Detect stale data

---

## Examples

See `/examples/` directory for complete examples:

- `python/` - Python client examples
- `javascript/` - Browser and Node.js examples
- `bash/` - Shell script examples (curl)

---

## Client Libraries

**Phase 1:** No official client libraries. Use HTTP and WebSocket directly.

**Future phases:**
- Python SDK
- JavaScript SDK
- Go SDK
- Rust SDK
