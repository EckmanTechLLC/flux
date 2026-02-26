# Flux API Reference

**Last Updated:** 2026-02-25

---

## Overview

Flux exposes two primary APIs:

- **HTTP REST API** - Event ingestion, state queries, namespace management, connector management, admin config
- **WebSocket API** - Real-time state subscriptions

**Base URL (local):** `http://localhost:3000` / `ws://localhost:3000`

---

## Authentication

**Two modes (controlled by `FLUX_AUTH_ENABLED` / `config.toml`):**

**Internal mode** (`auth_enabled = false`, default):
- No authentication required on any endpoint
- Suitable for trusted networks, local development

**Public mode** (`auth_enabled = true`):
- Write operations require `Authorization: Bearer <token>` header
- Token is issued at namespace registration
- Read operations (GET state, WebSocket subscribe) remain open — no auth required
- Admin config writes require `Authorization: Bearer <admin-token>` (separate token via `FLUX_ADMIN_TOKEN`)

---

## HTTP REST API

### Event Ingestion

#### POST /api/events

Publish a single event to Flux.

**Request:**

```http
POST /api/events HTTP/1.1
Content-Type: application/json
Authorization: Bearer <token>  # Required when auth enabled

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
- `timestamp` (required) - Unix epoch milliseconds (e.g. `Date.now()` in JS, `int(time.time()*1000)` in Python).
- `key` (optional) - Grouping/ordering key
- `schema` (optional) - Schema metadata (not validated)
- `payload` (required) - Event data (must be JSON object). **Limit: 1 MB.**

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
{"error": "Validation error: missing required field 'stream'"}

// 400 Bad Request - Invalid stream name
{"error": "Validation error: stream must be lowercase with optional dots"}

// 401 Unauthorized - Missing or invalid token (auth enabled)
{"error": "Unauthorized"}

// 403 Forbidden - Token does not own entity's namespace (auth enabled)
{"error": "Forbidden"}

// 413 Payload Too Large - Body exceeds 1 MB limit
{"error": "payload too large"}

// 429 Too Many Requests - Rate limit exceeded (auth enabled)
{"error": "rate limit exceeded"}

// 500 Internal Server Error - NATS publish failure
{"error": "Failed to publish event to NATS"}
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
Authorization: Bearer <token>  # Required when auth enabled

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

- `events` (required) - Array of FluxEvent objects (same structure as POST /api/events). **Limit: 10 MB total.**

**Response (200 OK):**

```json
{
  "successful": 2,
  "failed": 0,
  "results": [
    {"eventId": "01933d7a-1234-7890-abcd-ef1234567890", "stream": "sensors"},
    {"eventId": "01933d7a-1234-7890-abcd-ef1234567891", "stream": "sensors"}
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
    {"eventId": "01933d7a-1234-7890-abcd-ef1234567890", "stream": "sensors"},
    {"error": "Validation error: missing required field 'stream'"}
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
      }
    ]
  }'
```

---

#### GET /api/events

Retrieve raw stored events for an entity from the event log (NATS JetStream), newest-first.

**Request:**

```http
GET /api/events?entity=flux-iss/iss&limit=50&since=2026-02-25T00:00:00Z HTTP/1.1
```

**Query parameters:**

- `entity` (required) - Entity ID to fetch events for (e.g. `flux-iss/iss`)
- `since` (optional) - ISO 8601 start timestamp. Default: 24 hours ago.
- `limit` (optional) - Max events to return. Default: 100. Max: 500.

**Response (200 OK):** Array of raw FluxEvent objects, newest-first.

```json
[
  {
    "eventId": "019c9523-08d7-7210-b479-867e167e939d",
    "stream": "generic",
    "source": "bento.abc123",
    "timestamp": 1772028627158,
    "key": "iss",
    "payload": {
      "entity_id": "flux-iss/iss",
      "properties": {"latitude": "51.3", "longitude": "-137.6"}
    }
  }
]
```

**Error responses:**

```json
// 400 Bad Request - Missing entity parameter
{"error": "entity parameter is required"}

// 400 Bad Request - Invalid since timestamp
{"error": "invalid `since` timestamp (expected ISO 8601)"}
```

**curl example:**

```bash
curl "http://localhost:3000/api/events?entity=flux-iss/iss&limit=10"
curl "http://localhost:3000/api/events?entity=flux-iss/iss&since=2026-02-25T00:00:00Z"
```

---

### State Query

#### GET /api/state/entities

List all entities in current state.

**Request:**

```http
GET /api/state/entities HTTP/1.1
```

**Query parameters (optional):**

- `?namespace=matt` - Filter by namespace
- `?prefix=matt/sensor` - Filter by entity ID prefix

**Response (200 OK):**

```json
[
  {
    "id": "temp-sensor-01",
    "properties": {
      "temperature": 22.5,
      "unit": "celsius"
    },
    "lastUpdated": "2026-02-11T10:30:45.123Z"
  }
]
```

**curl example:**

```bash
curl http://localhost:3000/api/state/entities
curl "http://localhost:3000/api/state/entities?namespace=matt"
```

---

#### GET /api/state/entities/:id

Get a specific entity by ID.

**Response (200 OK):**

```json
{
  "id": "temp-sensor-01",
  "properties": {
    "temperature": 22.5,
    "unit": "celsius",
    "status": "active"
  },
  "lastUpdated": "2026-02-11T10:30:45.123Z"
}
```

**Error responses:**

```json
// 404 Not Found
{"error": "Entity not found"}
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
Authorization: Bearer <token>  # Required when auth enabled
```

**Response (200 OK):**

```json
{
  "entity_id": "temp-sensor-01",
  "eventId": "019c5c88-5386-7ae0-ab4d-80a8c1ce631a"
}
```

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
Authorization: Bearer <token>  # Required when auth enabled

{
  "prefix": "loadtest-"
}
```

**Filter options (choose one):**

```json
{"namespace": "matt"}
{"prefix": "loadtest-"}
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

**curl example:**

```bash
curl -X POST http://localhost:3000/api/state/entities/delete \
  -H "Content-Type: application/json" \
  -d '{"prefix": "loadtest-"}'
```

---

### Namespace Management

Namespaces are only available when `auth_enabled = true`. Returns 404 when auth is disabled.

#### POST /api/namespaces

Register a new namespace. Returns the bearer token for use in subsequent requests.

**Auth:** When `FLUX_ADMIN_TOKEN` is set, requires `Authorization: Bearer <admin-token>`. Without it, registration is unrestricted.

**Request:**

```http
POST /api/namespaces HTTP/1.1
Content-Type: application/json
Authorization: Bearer <admin-token>  # Required when FLUX_ADMIN_TOKEN is set

{
  "name": "matt"
}
```

**Name rules:** 3–32 characters, `[a-z0-9-_]` only.

**Response (200 OK):**

```json
{
  "namespaceId": "ns_7x9f2a",
  "name": "matt",
  "token": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Note:** `token` is only returned at registration. Store it — it cannot be retrieved later.

**Error responses:**

```json
// 400 Bad Request - Invalid name
{"error": "Namespace name too short (minimum 3 characters)"}

// 409 Conflict - Name already taken
{"error": "Namespace name already exists"}
```

**curl example:**

```bash
curl -X POST http://localhost:3000/api/namespaces \
  -H "Content-Type: application/json" \
  -d '{"name": "matt"}'
```

---

#### GET /api/namespaces/:name

Look up an existing namespace (token is not included in response).

**Response (200 OK):**

```json
{
  "namespaceId": "ns_7x9f2a",
  "name": "matt",
  "createdAt": "2026-02-20T10:00:00Z",
  "entityCount": 42
}
```

**curl example:**

```bash
curl http://localhost:3000/api/namespaces/matt
```

---

#### DELETE /api/namespaces/:name

Delete a namespace. Admin-only.

**Auth:** Requires `Authorization: Bearer <admin-token>` when `FLUX_ADMIN_TOKEN` is set. Without it, unrestricted.

**Request:**

```http
DELETE /api/namespaces/matt HTTP/1.1
Authorization: Bearer <admin-token>
```

**Response (204 No Content):** Empty body.

**Error responses:**

```json
// 401 Unauthorized - Missing or wrong admin token
{"error": "Admin token required"}

// 404 Not Found - Namespace does not exist
{"error": "Namespace not found"}
```

**curl example:**

```bash
curl -X DELETE http://localhost:3000/api/namespaces/matt \
  -H "Authorization: Bearer <admin-token>"
```

---

### Connector Management

Connectors pull data from external APIs and publish events to Flux. Implemented: `github`. Planned (framework ready, connector not yet built): `gmail`, `linkedin`, `calendar`.

Credential storage requires `FLUX_ENCRYPTION_KEY` to be set. Without it, all connectors report `not_configured`.

#### GET /api/connectors

List all available connectors with status.

**Request:**

```http
GET /api/connectors HTTP/1.1
Authorization: Bearer <token>  # Required when auth enabled
```

**Response (200 OK):**

```json
{
  "connectors": [
    {"name": "github", "enabled": true, "status": "configured"},
    {"name": "gmail", "enabled": false, "status": "not_configured"},
    {"name": "linkedin", "enabled": false, "status": "not_configured"},
    {"name": "calendar", "enabled": false, "status": "not_configured"}
  ]
}
```

**Status values:** `configured`, `not_configured`, `error`

**curl example:**

```bash
curl http://localhost:3000/api/connectors
curl http://localhost:3000/api/connectors -H "Authorization: Bearer <token>"
```

---

#### GET /api/connectors/:name

Get detailed status for a specific connector.

**Response (200 OK):**

```json
{
  "name": "github",
  "enabled": true,
  "status": "configured",
  "last_poll": null,
  "last_error": null,
  "poll_interval_seconds": 300
}
```

**Poll intervals:** github=300s (implemented). gmail/linkedin/calendar intervals are planned defaults, not yet active.

**curl example:**

```bash
curl http://localhost:3000/api/connectors/github
```

---

#### POST /api/connectors/:name/token

Store a Personal Access Token (PAT) for a connector. Enables the connector.

**Request:**

```http
POST /api/connectors/github/token HTTP/1.1
Content-Type: application/json
Authorization: Bearer <token>  # Required when auth enabled

{
  "token": "ghp_xxxxxxxxxxxxxxxxxxxx"
}
```

**Response (200 OK):**

```json
{"success": true}
```

**Error responses:**

```json
// 404 Not Found - Unknown connector name
{"error": "Connector 'unknown' not found"}

// 500 Internal Server Error - FLUX_ENCRYPTION_KEY not set
{"error": "Credential storage not available (FLUX_ENCRYPTION_KEY not set)"}
```

**curl example:**

```bash
curl -X POST http://localhost:3000/api/connectors/github/token \
  -H "Content-Type: application/json" \
  -d '{"token": "ghp_xxxxxxxxxxxxxxxxxxxx"}'
```

---

#### DELETE /api/connectors/:name/token

Remove stored credentials for a connector. Disables the connector.

**Request:**

```http
DELETE /api/connectors/github/token HTTP/1.1
Authorization: Bearer <token>  # Required when auth enabled
```

**Response (200 OK):**

```json
{"success": true}
```

**Error responses:**

```json
// 404 Not Found - No credentials stored
{"error": "No credentials found for connector 'github'"}
```

**curl example:**

```bash
curl -X DELETE http://localhost:3000/api/connectors/github/token
```

---

#### GET /api/connectors/:name/oauth/start

Initiate OAuth 2.0 authorization flow for a connector. Redirects to the provider's authorization page.

**Auth:** Requires `Authorization: Bearer <token>` when auth enabled.

**Response:** HTTP `302` redirect to provider authorization URL.

**Error responses:**

```json
// 404 Not Found - Unknown connector
{"error": "Connector 'unknown' not found"}

// 500 Internal Server Error - OAuth env vars not set
{"error": "OAuth not configured for connector 'github'. Set FLUX_OAUTH_GITHUB_CLIENT_ID and FLUX_OAUTH_GITHUB_CLIENT_SECRET environment variables."}
```

**curl example:**

```bash
# Opens in browser — redirect follows to provider
curl -L "http://localhost:3000/api/connectors/github/oauth/start" \
  -H "Authorization: Bearer <token>"
```

---

#### GET /api/connectors/:name/oauth/callback

OAuth 2.0 callback endpoint. Called by the provider after user authorization. Exchanges code for token, stores encrypted credentials.

This endpoint is called by the OAuth provider, not directly by clients.

**Query parameters (provided by OAuth provider):**

- `code` - Authorization code
- `state` - CSRF state token (generated by `/oauth/start`)

**Response (200 OK):**

```json
{
  "success": true,
  "message": "Successfully connected github",
  "connector": "github"
}
```

**Error responses:**

```json
// 400 Bad Request - OAuth denied by user
{"error": "OAuth authorization failed: access_denied - User cancelled"}

// 401 Unauthorized - Invalid/expired state token
{"error": "Invalid or expired OAuth state (possible CSRF attack)"}
```

---

### Admin Config

Runtime configuration for security limits. Changes take effect immediately — no restart required.

`GET` is readable by any authenticated user. `PUT` requires the admin bearer token (`FLUX_ADMIN_TOKEN`). When `FLUX_ADMIN_TOKEN` is not set, `PUT` is unrestricted (dev mode).

#### GET /api/admin/config

Read current runtime configuration.

**Response (200 OK):**

```json
{
  "rate_limit_enabled": true,
  "rate_limit_per_namespace_per_minute": 10000,
  "body_size_limit_single_bytes": 1048576,
  "body_size_limit_batch_bytes": 10485760
}
```

**curl example:**

```bash
curl http://localhost:3000/api/admin/config
```

---

#### PUT /api/admin/config

Update one or more runtime config fields. Only fields present in the request body are changed.

**Request:**

```http
PUT /api/admin/config HTTP/1.1
Content-Type: application/json
Authorization: Bearer <admin-token>

{
  "rate_limit_per_namespace_per_minute": 5000
}
```

**Configurable fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `rate_limit_enabled` | bool | true | Enable/disable rate limiting (auth mode only) |
| `rate_limit_per_namespace_per_minute` | u64 | 10000 | Max events per namespace per minute |
| `body_size_limit_single_bytes` | usize | 1048576 | Max body for POST /api/events (1 MB) |
| `body_size_limit_batch_bytes` | usize | 10485760 | Max body for POST /api/events/batch (10 MB) |

**Response (200 OK):** Returns full updated config (same format as GET).

**Error responses:**

```json
// 401 Unauthorized - Missing or invalid admin token
{"error": "Unauthorized"}
```

**curl example:**

```bash
curl -X PUT http://localhost:3000/api/admin/config \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <admin-token>" \
  -d '{"rate_limit_per_namespace_per_minute": 5000}'
```

---

## WebSocket API

### Connection

**Endpoint:** `GET /api/ws`

Upgrade HTTP connection to WebSocket.

**Auth:** WebSocket is read-only — no authentication required regardless of `auth_enabled` mode.

**JavaScript example:**

```javascript
const ws = new WebSocket('ws://localhost:3000/api/ws');

ws.onopen = () => console.log('Connected to Flux');
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  console.log('Received:', message);
};
ws.onerror = (error) => console.error('WebSocket error:', error);
ws.onclose = () => console.log('Disconnected from Flux');
```

**Python example:**

```python
import asyncio
import websockets
import json

async def connect():
    uri = "ws://localhost:3000/api/ws"
    async with websockets.connect(uri) as websocket:
        await websocket.send(json.dumps({
            "type": "subscribe",
            "entity_id": "temp-sensor-01"
        }))
        async for message in websocket:
            data = json.loads(message)
            print(f"Received: {data}")

asyncio.run(connect())
```

---

### Messages

#### Client → Server: Subscribe

Subscribe to updates for a specific entity.

```json
{
  "type": "subscribe",
  "entity_id": "temp-sensor-01"
}
```

- `entity_id`: Use `"*"` to subscribe to all entities.
- Multiple subscriptions allowed.

---

#### Client → Server: Unsubscribe

Stop receiving updates for a specific entity.

```json
{
  "type": "unsubscribe",
  "entity_id": "temp-sensor-01"
}
```

---

#### Server → Client: State Update

Sent when a subscribed entity property changes.

```json
{
  "type": "state_update",
  "entity_id": "temp-sensor-01",
  "property": "temperature",
  "value": 22.5,
  "timestamp": "2026-02-14T10:30:45.123Z"
}
```

One message per property update (not batched).

---

#### Server → Client: Metrics Update

Real-time metrics broadcast (every 2 seconds by default).

```json
{
  "type": "metrics_update",
  "timestamp": "2026-02-14T14:30:45.123Z",
  "entities": {"total": 1543},
  "events": {"total": 458392, "rate_per_second": 45.2},
  "websocket": {"connections": 3},
  "publishers": {"active": 12}
}
```

---

#### Server → Client: Entity Deleted

Notification when an entity is deleted (sent to all connected clients).

```json
{
  "type": "entity_deleted",
  "entity_id": "temp-sensor-01",
  "timestamp": "2026-02-14T14:30:45.123Z"
}
```

---

### Usage Patterns

#### Pattern 1: Subscribe and Stream

```javascript
const ws = new WebSocket('ws://localhost:3000/api/ws');

ws.onopen = () => {
  ws.send(JSON.stringify({type: 'subscribe', entity_id: 'temp-sensor-01'}));
  ws.send(JSON.stringify({type: 'subscribe', entity_id: '*'}));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'state_update') updateUI(msg);
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
  ws.send(JSON.stringify({type: 'subscribe', entity_id: '*'}));
};
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'state_update') updateEntity(msg);
};
```

---

## Error Handling

### HTTP API Error Codes

| Code | Meaning |
|------|---------|
| 400 | Bad Request — invalid JSON, missing fields, validation failure |
| 401 | Unauthorized — missing or invalid bearer token |
| 403 | Forbidden — token valid but not authorized for this resource |
| 404 | Not Found — entity, connector, or namespace doesn't exist |
| 409 | Conflict — namespace name already taken |
| 413 | Payload Too Large — body exceeds configured size limit |
| 429 | Too Many Requests — rate limit exceeded (`Retry-After: 60` header included) |
| 500 | Internal Server Error — NATS failure, state engine error |

**Error response format:**

```json
{"error": "Human-readable error message"}
```

### WebSocket Errors

- **Invalid JSON message:** Silently ignored by server
- **Unknown message type:** Silently ignored by server

**Reconnection handling:**

```javascript
ws.onclose = (event) => {
  if (!event.wasClean) {
    setTimeout(() => reconnect(), 5000);
  }
};
```

---

## Rate Limits

**Active only when `auth_enabled = true`.** No limits in internal mode.

- **Per namespace:** 10,000 events/minute (~167 eps) — configurable via admin API
- **Granularity:** Per namespace (one namespace cannot starve others)
- **State:** In-memory (resets on restart)
- **Exceeded:** `429 Too Many Requests` with `Retry-After: 60` header

**Body size limits (always enforced):**

- Single event (`POST /api/events`): 1 MB
- Batch events (`POST /api/events/batch`): 10 MB
- Exceeded: `413 Payload Too Large`

---

## Best Practices

### Event Publishing

1. **Include meaningful source:** Identify the producer clearly
2. **Use consistent stream names:** Namespace by domain (e.g., "sensors.temperature")
3. **Always include entity_id in payload:** Required for state derivation
4. **Send only changed properties:** Properties are merged per-entity — each event updates only the properties it includes, existing properties are preserved
5. **Use batch API for multiple events:** Better performance

### State Subscription

1. **Get snapshot first:** HTTP GET before WebSocket subscribe for initial state
2. **Subscribe selectively:** Only subscribe to entities you need
3. **Handle reconnections:** WebSocket may disconnect, implement retry logic
4. **Unsubscribe when done:** Free server resources

### State Queries

1. **Use WebSocket for real-time:** HTTP is for snapshot only
2. **Cache locally:** Minimize repeated queries
3. **Check `lastUpdated`:** Detect stale data

---

## Examples

See `/examples/` directory for complete examples:

- `python/` - Python client examples
- `javascript/` - Browser and Node.js examples
- `bash/` - Shell script examples (curl)
