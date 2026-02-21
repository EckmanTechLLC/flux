# Session: ADR-007 Phase 3B Tasks 4+5 — Named Connector API + UI

**Date:** 2026-02-21
**Branch:** main
**Status:** Complete

---

## What Was Done

### Task 4: Named Connector API endpoints (`connector-manager/src/api.rs`)

Added two new endpoints:

- `POST /api/connectors/named` — creates a named Singer tap source
  - Body: `tap_name`, `namespace`, `entity_key_field`, `config_json`, `poll_interval_secs`
  - Generates UUID source ID, persists to `NamedConfigStore`, starts `NamedRunner` background task
  - Returns `{ source_id }`

- `DELETE /api/connectors/named/:source_id` — stops and removes a named source
  - Aborts background task, deletes config from SQLite, removes temp files

`GET /api/connectors` already included named sources (verified, no change needed).

Also added:
- `CreateNamedSourceRequest` and `CreateNamedSourceResponse` structs
- `handle_create_named_source` and `handle_delete_named_source` business logic functions (testable without HTTP layer)
- Import: `use crate::named_config::NamedSourceConfig`
- 2 new tests: `test_post_named_source_stores_config`, `test_delete_named_source_removes_config`

Routes registered in `create_router` before generic routes.

### Task 5: Named Connector UI (`ui/public/index.html`)

Added to connectors panel HTML:
- "Add Named Source" button (collapsible, same style as generic)
- `#namedSourceForm` with fields: Tap Name, Namespace, Entity Key Field, Poll Every (sec), Config JSON (textarea)
- Client-side JSON validation before submit

Added to `renderConnectors` JS:
- `ctype === 'named'` branch — same status-color logic as generic, Remove button calls `deleteNamedSource`
- Icon: 🎵 for named type

Added JS functions:
- `deleteNamedSource(sourceId)` — DELETEs to `/api/connector-manager/connectors/named/${sourceId}`
- `toggleNamedForm()` — show/hide form
- `addNamedSource()` — validates fields + JSON, POSTs to `/api/connector-manager/connectors/named`, resets form on success

No changes to `server.js` — existing proxy covers all `/api/connector-manager/*` paths including DELETE with params.

---

## Files Modified

- `connector-manager/src/api.rs` — API structs, handlers, routes, tests
- `ui/public/index.html` — named form HTML + renderConnectors + JS functions

---

## Test Results

```
56 passed; 0 failed (connector-manager)
3 doc tests passed
```

---

## Rebuild Required

```
docker compose build --no-cache connector-manager flux-ui && docker compose up -d connector-manager flux-ui
```

---

## Next Steps (Phase 3B complete after this)

- Phase 3C: webhooks, error alerting, manual trigger ("Sync now" button)
