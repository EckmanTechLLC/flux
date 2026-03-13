# Session: ADR-010 i3X Connector — Tasks 3 & 4

**Date:** 2026-03-13
**Status:** Complete ✅

---

## What Was Done

### Task 3: API Endpoints (`api.rs`)

**New request/response types:**
- `CreateI3xSourceRequest` — name, base_url, namespace, api_key, flux_namespace_token
- `CreateI3xSourceResponse` — source_id

**New business logic handlers (testable, decoupled from HTTP):**
- `handle_create_i3x_source` — generates UUID, inserts config, stores API key in CredentialStore under `"i3x"/{id}`, calls `I3xRunner::start_source`
- `handle_delete_i3x_source` — stops runner task, deletes config, removes credentials (best-effort)
- `handle_sync_i3x_source` — looks up config + credential, calls `I3xRunner::trigger_sync`

**New HTTP handlers:**
- `POST /api/connectors/i3x` → 201 Created
- `DELETE /api/connectors/i3x/:source_id` → 204 No Content
- `POST /api/connectors/i3x/:source_id/sync` → 202 Accepted

**Extended `GET /api/connectors`:**
- i3X sources appear with `connector_type: "i3x"`
- Status: "running" / "error" / "stopped" based on `I3xStatus.last_error`
- `last_started` mapped from `I3xStatus.last_event`

**Updated `ApiState`:**
- Added `i3x_config_store: Arc<I3xConfigStore>`
- Added `i3x_runner: Arc<I3xRunner>`

### Task 4: Wire into `main.rs`

**New env var:** `I3X_CONFIG_DB` (default: `/data/i3x_config.db`)

**Startup sequence:**
1. Initialize `I3xConfigStore`
2. Initialize `I3xRunner`
3. List persisted i3X sources
4. For each: fetch API key from `CredentialStore` under `"i3x"/{id}`, call `start_source`
5. Pass `i3x_config_store` and `i3x_runner` into `ApiState`

**Minor addition to `runners/i3x.rs`:**
- Added `trigger_sync` method (fire-and-forget spawn)
- Added `sync_once_http` private function: discover → subscribe → register → POST `/subscriptions/{id}/sync`
- Sync response body is discarded (format not specified in i3X OpenAPI spec at time of writing)

---

## Tests

16 API tests pass (9 pre-existing + 5 new i3X):
- `test_post_i3x_source_stores_config` — config persisted in SQLite
- `test_post_i3x_source_stores_credentials` — API key stored in CredentialStore
- `test_delete_i3x_source_removes_config` — config removed after delete
- `test_sync_i3x_source_not_found_returns_error` — 404-style error for unknown id
- `test_i3x_source_appears_in_list` — config visible in store after create

Total connector-manager tests: **79 passed, 0 failed**

---

## Files Modified

| File | Change |
|------|--------|
| `connector-manager/src/api.rs` | Added i3X types, handlers, routes, list extension, tests |
| `connector-manager/src/main.rs` | Wired I3xConfigStore + I3xRunner, startup restart loop |
| `connector-manager/src/runners/i3x.rs` | Added `trigger_sync` + `sync_once_http` |

---

## Next Step

**Task 5:** UI — add i3X panel to `flux-ui/index.html`
- "Add i3X Source" form: Name, Base URL, API Key, Namespace, Flux Token
- Status display: source name, object count, last event, error
- Delete button
