# Session: Generic Source UI Form

**Date:** 2026-02-20
**Task:** ADR-007 Phase 3A Task 4 — Add Generic Source UI form

---

## What Was Done

### ui/server.js
- Added `CONNECTOR_MANAGER_API` constant (default `http://localhost:3001`)
- Added `/api/connector-manager/*` proxy route **before** the existing `/api/` proxy
- Strips `/connector-manager` prefix and forwards to `http://localhost:3001/api/*`
- Route order matters: connector-manager check runs first

### ui/public/index.html (CSS)
- Widened `.connectors-panel` from 300px → 380px (form needs the space)
- Added `max-height: calc(100vh - 100px)` + `overflow-y: auto` to prevent off-screen overflow

### ui/public/index.html (HTML)
Added inside `#connectorsPanel` below `#connectorsList`:
- Divider + "Add Generic Source" toggle button
- Collapsible form (`#genericSourceForm`) with fields:
  - Source Name, URL, Poll Every, Entity Key, Namespace
  - Auth Type select (None / Bearer Token / API Key Header)
  - Token field (shown when auth ≠ None)
  - API Key Header Name field (shown only when API Key Header selected)
  - Add Source button + feedback div

### ui/public/index.html (JS)
- `loadConnectorStatus`: changed URL from `/api/connectors` → `/api/connector-manager/connectors`
- `renderConnectors`: updated to handle flat array (connector-manager returns `[]` not `{connectors:[]}`)
  - Added `type` badge (builtin/generic) shown on every row
  - Generic connectors: show status badge (green=running, red=error, gray=stopped), last_error if present
  - Builtin connectors: keep PAT form behavior; updated condition to accept `status === 'running'` as connected
- New `toggleGenericForm()`: collapses/expands the form
- New `onAuthTypeChange()`: shows/hides token and header-name fields based on auth select
- New `addGenericSource()`: POSTs to `/api/connector-manager/connectors/generic`, shows source_id on success, clears form, refreshes connector list

---

## Files Modified
- `ui/server.js`
- `ui/public/index.html`

## Files Created
- `docs/sessions/2026-02-20-generic-source-ui.md` (this file)

---

## Notes
- Connector-manager Rust code was NOT modified (per scope constraint)
- The connector-manager's `GET /api/connectors` returns a flat array — handled in `renderConnectors`
- Generic connector delete is not surfaced in the UI yet (source_id not returned by list endpoint)
- `c.type` in JS accesses the `type` JSON field (serialized from Rust `connector_type` via `#[serde(rename = "type")]`)
