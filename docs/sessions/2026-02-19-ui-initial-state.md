# Session: UI Initial State Load

**Date:** 2026-02-19
**Status:** Complete ✅
**Branch:** main

---

## Objective

Fix: entities did not appear on page load — only populated when a WebSocket `state_update` message arrived.

## Root Cause

The entity store (`entities = {}`) was only populated via `updateProperty()` from live WebSocket messages. On initial load, the page sat empty until the first event came in.

## Fix

Added `loadState()` — fetches current state from `GET /api/state/entities` and feeds it through the existing rendering pipeline.

**File Modified:** `ui/public/index.html`

### `loadState()` function (line ~1130)

```javascript
async function loadState() {
  try {
    const url = activeServerPrefix
      ? `/api/state/entities?prefix=${encodeURIComponent(activeServerPrefix)}`
      : '/api/state/entities';
    const resp = await fetch(url);
    const data = await resp.json();
    for (const e of data) {
      entities[e.id] = { properties: e.properties, lastUpdated: e.lastUpdated };
    }
    detectEntityTypes();
    renderFilterBar();
    renderEntities();
    updatePipeline();
  } catch (e) {
    console.error('Failed to load state:', e);
  }
}
```

### Invocation points

1. **Page load** — called immediately before `connectWS()`:
   ```javascript
   loadState();
   connectWS();
   ```
2. **WebSocket reconnect** — called inside `ws.onopen` after resetting state (handles Flux restarts):
   ```javascript
   entities = {};
   eventCount = 0;
   loadState();
   ```
3. **Server prefix filter** — called inside `applyServerPrefix()` after resetting state.

## Behavior After Fix

- Page loads → HTTP GET `/api/state/entities` → entities rendered immediately
- WebSocket connects → subscribes to live updates → incremental `state_update` messages update individual entities via `updateProperty()`
- On WebSocket reconnect (e.g., Flux restart) → state re-synced from server

## API Contract

The `GET /api/state/entities` endpoint returns `Array<{ id, properties, lastUpdated }>` (matching `EntityResponse` in `src/api/query.rs`). The `lastUpdated` field is an RFC3339 string (serialized with `#[serde(rename = "lastUpdated")]`).

## Files Modified

- `ui/public/index.html` — added `loadState()`, added calls at page load and reconnect

## Files NOT Modified

- `src/` — no server-side changes
- `ui/server.js` — proxy already forwards `GET /api/` correctly

## Testing

No automated tests. Manual verification:

1. Load page → entities appear without waiting for a WebSocket message
2. Disconnect/reconnect WebSocket → entities re-sync from server
3. Apply server prefix → entities re-fetched with prefix filter
