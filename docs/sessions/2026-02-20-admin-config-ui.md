# Session: Admin Config Panel in Flux UI

**Date:** 2026-02-20
**ADR:** 006-security-hardening.md
**File changed:** `ui/public/index.html` only

---

## What Was Done

Added a runtime config panel to the Flux UI, following the existing Connectors panel pattern.

### Changes

**Header** — Added `⚙ Config` toggle button next to Connectors button (same `.connectors-toggle` class).

**CSS** — Added `.config-panel`, `.config-field`, `.config-label`, `.config-input`, `.config-checkbox-row`, `.config-divider`, `.config-save-btn`, `.config-feedback` styles (~75 lines). Positioned at `top: 64px; right: 16px` (same as connectors panel — panels close each other on open).

**HTML panel** — Fixed `id="configPanel"` with:
- Admin token password input (optional, used in `Authorization: Bearer` header)
- Divider
- `rate_limit_enabled` checkbox
- `rate_limit_per_namespace_per_minute` number input
- `body_size_limit_single_bytes` number input
- `body_size_limit_batch_bytes` number input
- Save button + inline feedback div

**JS** — Added/modified:
- `toggleConnectors()` — closes config panel when connectors opens
- `toggleConfig()` — closes connectors panel, calls `loadAdminConfig()` on open
- `loadAdminConfig()` — `GET /api/admin/config`, populates form fields, shows error inline
- `saveAdminConfig()` — `PUT /api/admin/config` with optional `Authorization: Bearer` header, shows success/error inline

---

## Behavior

- Panel opens → fetches current config and populates fields
- Two panels mutually exclusive (opening one closes the other)
- Save: sends only non-null fields; `rate_limit_enabled` always included (checkbox)
- Feedback: green "✓ Config saved" for 3s on success; red error message on failure
- Works with auth disabled (no token needed) and auth enabled (token required for PUT)
