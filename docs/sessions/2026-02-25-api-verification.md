# Session: API & Docs Verification

**Date:** 2026-02-25
**Task:** Test every Flux API endpoint against live public instance, update docs/api.md with corrections.

---

## Method

1. Read all route files to build complete endpoint inventory
2. Cross-checked against docs/api.md
3. Tested unauthenticated endpoints live against https://api.flux-universe.com
4. Updated docs/api.md for every discrepancy

---

## Complete Endpoint Inventory (from source)

| Method | Path | File |
|--------|------|------|
| POST | /api/events | ingestion.rs |
| POST | /api/events/batch | ingestion.rs |
| GET | /api/events | history.rs |
| GET | /api/state/entities | query.rs |
| GET | /api/state/entities/:id | query.rs |
| DELETE | /api/state/entities/:id | deletion.rs |
| POST | /api/state/entities/delete | deletion.rs |
| POST | /api/namespaces | namespace.rs |
| GET | /api/namespaces/:name | namespace.rs |
| DELETE | /api/namespaces/:name | namespace.rs |
| GET | /api/connectors | connectors.rs |
| GET | /api/connectors/:name | connectors.rs |
| POST | /api/connectors/:name/token | connectors.rs |
| DELETE | /api/connectors/:name/token | connectors.rs |
| GET | /api/connectors/:name/oauth/start | oauth/mod.rs |
| GET | /api/connectors/:name/oauth/callback | oauth/mod.rs |
| GET | /api/admin/config | admin.rs |
| PUT | /api/admin/config | admin.rs |
| GET | /api/ws | websocket.rs |

---

## Live Test Results (https://api.flux-universe.com)

| Endpoint | Expected | Actual | Pass? |
|----------|----------|--------|-------|
| GET /api/state/entities?namespace=flux-iss | 200, array | 200, array | ✓ |
| GET /api/state/entities/flux-iss%2Fiss | 200, entity | 200, entity with `lastUpdated` | ✓ |
| GET /api/admin/config | 200, config object | 200, config object | ✓ |
| GET /api/namespaces/flux-iss | 200, namespace info | 200, `{"namespaceId":...,"createdAt":...}` | ✓ |
| GET /api/events?entity=flux-iss/iss&limit=2 | 200, array | 200, events array newest-first | ✓ |
| DELETE /api/namespaces/flux-iss (no token) | 401 | 401 | ✓ |
| POST /api/events (no auth) | 401 | 400 — `timestamp` field missing | see #2 below |
| GET /api/connectors (no token) | 401 | 401 | ✓ |
| POST /api/events/batch (no auth, with timestamp) | 401 | 401-equivalent in result | ✓ |

---

## Discrepancies Found & Fixed

### #1 — `last_updated` → `lastUpdated` (FIXED in docs)
- **Where:** `GET /api/state/entities` and `GET /api/state/entities/:id` response examples
- **Code:** `query.rs:32` — `#[serde(rename = "lastUpdated")]`
- **Docs (wrong):** `"last_updated": "2026-02-11T10:30:45.123Z"`
- **Actual (confirmed live):** `"lastUpdated": "2026-02-25T20:04:27.638770186+00:00"`
- **Fix:** Updated both response examples in docs.

### #2 — `timestamp` is required, not optional (FIXED in docs)
- **Where:** `POST /api/events` request fields description
- **Code:** `event/mod.rs:30` — `pub timestamp: i64` (no `#[serde(default)]`)
- **Docs (wrong):** "optional — Defaults to current time if omitted"
- **Confirmed live:** `{"error":"missing field 'timestamp'"}` when omitted
- **Fix:** Changed to "required" with format note.

### #3 — `GET /api/events` history endpoint entirely missing (FIXED in docs)
- **File:** `api/history.rs`
- **Params:** `entity` (required), `since` (ISO 8601, optional, default 24h ago), `limit` (int, optional, default 100, max 500)
- **Returns:** Array of raw FluxEvent objects, newest-first
- **Confirmed live:** Works as documented
- **Fix:** Added new section "GET /api/events" between batch events and State Query.

### #4 — `DELETE /api/namespaces/:name` missing (FIXED in docs)
- **File:** `api/namespace.rs:52-56` — `.delete(delete_namespace)` on the /:name route
- **Added in:** ADR-009 (task 1), session 2026-02-25-adr009-task1.md
- **Auth:** Requires admin token if `FLUX_ADMIN_TOKEN` set; unrestricted otherwise
- **Returns:** `204 No Content` (empty body — not JSON)
- **Confirmed live:** 401 when no token ✓
- **Fix:** Added new section after `GET /api/namespaces/:name`.

### #5 — OAuth endpoints missing (FIXED in docs)
- **File:** `api/oauth/mod.rs`
- **Routes:** `GET /api/connectors/:name/oauth/start`, `GET /api/connectors/:name/oauth/callback`
- **Fix:** Added two new sections to Connector Management.

### #6 — WebSocket auth docs outdated (FIXED in docs)
- **Removed 2026-02-23** per CLAUDE.md and `src/api/websocket.rs` (no auth check)
- **Docs (wrong):** Required `?token=<bearer-token>` query parameter, 401 on failure
- **Actual:** No auth, open to all connections
- **Fix:** Removed `?token` references and 401 note. Added note: "WebSocket is read-only — no authentication required."
- Also removed from Authentication overview section.

### #7 — `POST /api/namespaces` admin token requirement undocumented (FIXED in docs)
- **Code:** `namespace.rs:70-78` — requires admin token when `state.admin_token` is set
- **Docs (missing):** No mention of admin token requirement
- **Fix:** Added auth note and updated request example with optional `Authorization` header.

### #8 — `GET /api/state/entities/:id` 404 message (FIXED in docs)
- **Code:** `query.rs:127` — `"Entity not found"` (no entity ID appended)
- **Docs (wrong):** `{"error": "Entity not found: temp-sensor-99"}`
- **Fix:** Corrected to `{"error": "Entity not found"}`.

---

## Not Investigated (manual verify needed)

- **POST /api/events with valid auth token** — requires flux-iss token
- **POST /api/events/batch with valid auth** — requires flux-iss token
- **DELETE /api/state/entities/:id** — requires namespace token
- **POST /api/state/entities/delete** — requires namespace token
- **GET /api/connectors, GET /api/connectors/:name** — requires namespace token
- **POST /api/connectors/:name/token** — requires namespace token + credential store
- **OAuth flow (start/callback)** — requires FLUX_OAUTH_* env vars configured

---

## Files Modified

- `docs/api.md` — 8 discrepancies corrected, 3 missing endpoints added
