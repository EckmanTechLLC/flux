# ADR-010: i3X Connector

**Status:** Proposed
**Date:** 2026-03-13
**Extends:** ADR-007 (Universal Connector Framework)

---

## Context

i3X (Industrial Information Interoperability eXchange) is an open, vendor-neutral API
standard by CESMII for contextualized manufacturing information platforms. Any platform
implementing i3X exposes a common REST API for discovering objects, querying values,
and subscribing to real-time changes via Server-Sent Events (SSE).

Flux is domain-agnostic. i3X is a well-defined standard. Adding i3X as a first-class
connector type means any i3X-compatible manufacturing platform feeds directly into
Flux state — no custom integration work per platform.

**i3X is not a competitor to Flux.** i3X is a query/subscription API layer over
existing manufacturing systems. Flux is a state engine that owns and derives state.
The i3X connector bridges the two: i3X-compatible platforms become data sources
for Flux's canonical world state.

**i3X API key facts (from OpenAPI spec at api.i3x.dev/v0/openapi.json):**
- `GET /namespaces` — discover available namespaces
- `GET /objects` — enumerate all objects (optionally filtered by type)
- `POST /subscriptions` — create a subscription
- `POST /subscriptions/{id}/register` — register objects for monitoring
- `GET /subscriptions/{id}/stream` — SSE stream of real-time value changes
- `POST /subscriptions/{id}/sync` — pull-and-clear queued updates (fallback)
- Auth: API key minimum (Bearer token in Authorization header)

**SSE event format:**
```json
{ "elementId": "...", "value": <any>, "quality": "Good", "timestamp": "2026-..." }
```

**Note:** Built against the i3X OpenAPI spec. No live i3X endpoint was available
for testing at time of writing (api.i3x.dev demo was returning 404). Testing
against a live endpoint is required before production use.

---

## Decision

Add `i3x` as a fourth connector type in the connector-manager framework, alongside
`builtin`, `generic`, and `named`.

### Why a dedicated type (not Generic/Named)

- **Generic (Bento):** polls a single URL on a schedule. i3X requires multi-step
  setup (discover → subscribe → register → stream SSE). Bento cannot do this.
- **Named (Singer):** batch-run subprocess, exits after sync. i3X is a long-lived
  SSE stream. Singer protocol does not apply.
- **Dedicated i3x runner:** long-lived async Tokio task, mirrors the SSE streaming
  model natively. Clean fit.

---

## Architecture

```
User configures i3X source in UI
  → base_url, api_key, namespace, flux_namespace_token
  → POST /api/connectors/i3x → stored in SQLite (i3x_config.db)

I3xRunner (connector-manager)
  → on start: GET /objects → discover all objects
  → POST /subscriptions → create subscription
  → POST /subscriptions/{id}/register → register all objects
  → GET /subscriptions/{id}/stream → open SSE stream (long-lived)
  → on SSE event: map elementId → entity_id, value → properties
  → POST http://flux:3000/api/events

  → on stream disconnect: reconnect with exponential backoff
  → on subscription expiry: recreate subscription, re-register, reconnect
```

---

## Data Model

**Config stored in SQLite (`i3x_config.db`):**

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Source ID |
| `name` | String | Display name |
| `base_url` | String | i3X endpoint base URL |
| `namespace` | String | Flux namespace to publish into |
| `flux_namespace_token` | String | Flux auth token for namespace |
| `created_at` | Timestamp | Creation time |

API key stored separately in the existing encrypted credential store under
`i3x/{source_id}`.

**i3X object → Flux entity mapping:**

| i3X | Flux |
|-----|------|
| `elementId` | `entity_id` = `{namespace}/{elementId}` (sanitized) |
| SSE `value` (scalar) | property `value` |
| SSE `value` (object) | each key → property |
| SSE `quality` | property `i3x_quality` |
| SSE `timestamp` | property `i3x_timestamp` |

**Flux event payload:**
```json
{
  "stream": "i3x",
  "source": "i3x.{source_id}",
  "timestamp": <millis>,
  "key": "<elementId>",
  "payload": {
    "entity_id": "{namespace}/{elementId}",
    "properties": {
      "value": <value>,
      "i3x_quality": "Good",
      "i3x_timestamp": "2026-..."
    }
  }
}
```

---

## Implementation Tasks

### Task 1: Config storage (`i3x_config.rs`)
- SQLite table for i3X source config
- CRUD: create, get, list, delete
- API key stored in existing CredentialStore under `i3x/{source_id}`

### Task 2: i3X runner (`runners/i3x.rs`)
- `I3xRunner` struct: manages one long-lived Tokio task per source
- `start_source`: discover objects → subscribe → register → stream SSE
- SSE parsing: `eventsource-client` crate or raw `reqwest` streaming
- On each SSE event: map to Flux event → POST to Flux API
- Reconnect on disconnect with exponential backoff (1s → 2s → 4s → max 60s)
- Status tracking: last event time, error, object count

### Task 3: API endpoints (`api.rs` extension)
- `POST /api/connectors/i3x` — add i3X source
- `DELETE /api/connectors/i3x/{id}` — remove source + credentials
- `POST /api/connectors/i3x/{id}/sync` — manual trigger (one-shot sync via `/sync` endpoint)
- Extend `GET /api/connectors` to include i3x type in status response

### Task 4: Wire into `main.rs`
- Initialize `I3xConfigStore`
- Initialize `I3xRunner`
- Load all stored i3X sources on startup, call `start_source` for each
- Pass runner to API state

### Task 5: UI (`index.html`)
- "Add i3X Source" form: Name, Base URL, API Key, Namespace
- Status display in connector panel: source name, object count, last event, error
- Delete button

---

## SSE Implementation Note

`reqwest` supports streaming responses. The SSE stream is a `text/event-stream`
response where each event is:

```
data: {"elementId":"...","value":...,"quality":"Good","timestamp":"..."}
```

Parse line by line: lines starting with `data:` contain the JSON payload.
Lines starting with `event:` contain the event type (ignore if not needed).
Empty lines separate events.

No external SSE crate required — raw streaming with `reqwest` is sufficient.

---

## Consequences

### Positive
- Any i3X-compatible manufacturing platform feeds into Flux with zero custom code
- Long-lived SSE stream — real-time updates, not polling
- First-class connector type: UI-configurable, status-visible, deletable
- Open standard: as i3X adoption grows, Flux gains more compatible sources automatically

### Negative
- No live i3X endpoint available for testing at time of writing
- SSE reconnection logic adds complexity vs. simple polling connectors
- Object discovery at startup could be slow for large i3X deployments

### Neutral
- i3X API key auth only (no OAuth) — simpler than GitHub connector
- Existing three connector types unaffected

---

## References

- [i3X GitHub](https://github.com/cesmii/i3X)
- [i3X OpenAPI spec](https://api.i3x.dev/v0/openapi.json)
- [CESMII](https://www.cesmii.org)
- ADR-007: Universal Connector Framework
