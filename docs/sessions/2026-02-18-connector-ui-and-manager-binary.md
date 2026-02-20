# Session: Connector UI & Manager Binary

**Date:** 2026-02-18
**Task:** Phase 1, Task 6 + Gap Fix - UI Integration & Connector Manager Binary
**Reference:** ADR-005 lines 254-257
**Status:** ✅ COMPLETE

---

## Objective

Two deliverables:
1. **Gap Fix:** Binary entry point for connector-manager (was library-only, could not run)
2. **UI Integration:** Connectors panel in existing Flux UI

---

## Part A: Connector Manager Binary

### Files Created

**`connector-manager/src/main.rs`** - Binary entry point
- Reads env vars: `FLUX_API_URL` (default: http://localhost:3000), `FLUX_ENCRYPTION_KEY` (required, exits with error if missing), `FLUX_CREDENTIALS_DB` (default: credentials.db)
- Initializes `CredentialStore`
- Initializes `ConnectorManager`
- Calls `manager.start()` (logs available connectors)
- Waits for Ctrl+C via `tokio::signal::ctrl_c`
- Calls `manager.shutdown()` on signal

### Files Modified

**`connector-manager/Cargo.toml`**
- Added `[[bin]]` section pointing to `src/main.rs`
- Added `tracing-subscriber = { version = "0.3", features = ["env-filter"] }` dependency

### Behavior on Startup

Per known gap from previous session: `manager.start()` logs available connectors but starts 0 schedulers (credential enumeration not yet implemented). Schedulers start on-demand when OAuth flow completes. This is expected Phase 1 behavior.

---

## Part B: UI Connectors Section

### File Modified

**`ui/public/index.html`** - Added connectors panel

**CSS added:**
- `.connectors-toggle` — fixed-position button (bottom-right, offset left of Load Test button)
- `.connectors-panel` — collapsible panel matching loadtest-panel pattern
- `.connector-row`, `.connector-info`, `.connector-icon`, `.connector-name` — row layout
- `.connector-badge.connected` (green) and `.connector-badge.not-connected` (gray)
- `.connector-action a` — Connect/Disconnect link styles

**HTML added (after main layout, before loadtest button):**
- `<button class="connectors-toggle">` — toggles panel visibility
- `<div id="connectorsPanel">` — panel with `<div id="connectorsList">` as dynamic content

**JavaScript added:**
- `CONNECTOR_ICONS` — emoji map for github, gmail, linkedin, calendar
- `toggleConnectors()` — shows/hides panel
- `loadConnectorStatus()` — `GET /api/connectors`, calls `renderConnectors()` on success; shows "Unavailable" on error
- `renderConnectors(connectors)` — builds rows: icon + name + badge + Connect/Disconnect link
  - Connect → opens `/api/connectors/{name}/oauth/start` in new tab
  - Disconnect → no-op link (OAuth DELETE not yet implemented)
- `loadConnectorStatus()` called on page load
- `setInterval(loadConnectorStatus, 30000)` — refreshes every 30 seconds

---

## Testing

### Build
```bash
cd connector-manager && cargo build
# → Finished dev profile, binary at target/debug/connector-manager
```

### Tests
```bash
cd connector-manager && cargo test
# → 9/9 unit tests + 3/3 doc tests pass
```

### Run binary
```bash
FLUX_ENCRYPTION_KEY=<base64-32-bytes> ./target/debug/connector-manager
# → Logs: "Connector Manager starting...", "Configuration loaded", "Connector Manager started"
# → Ctrl+C → "Shutdown signal received", "Connector manager stopped"
```

### UI
```bash
# Open http://192.168.50.106:8082 in browser
# → "🔌 Connectors" button appears bottom-right (left of "🔥 Load Test")
# → Click → panel shows github/gmail/linkedin/calendar rows
# → Status: "Not Connected" (gray) when no credentials configured
# → "Connect" links point to /api/connectors/{name}/oauth/start (new tab)
# Note: OAuth will fail without real client IDs (expected Phase 1)
```

---

## Checklist Completion

### 1. READ FIRST
- [x] CLAUDE.md
- [x] ADR-005 (lines 254-257)
- [x] connector-manager session notes (2026-02-17)
- [x] connector-manager/src/manager.rs, lib.rs, Cargo.toml
- [x] ui/public/index.html (full file)
- [x] src/api/connectors.rs (API response shape)

### 2. VERIFY ASSUMPTIONS
- [x] `tokio::signal::ctrl_c` available (tokio "full" feature already present)
- [x] `CredentialStore::new(path, key)` signature confirmed
- [x] `ConnectorManager::new(store, url)` and `.start()` / `.shutdown()` confirmed
- [x] `/api/connectors` returns `{ connectors: [{ name, enabled, status }] }`
- [x] OAuth start URL pattern: `/api/connectors/{name}/oauth/start`

### 3. MAKE CHANGES
- [x] connector-manager/src/main.rs (created)
- [x] connector-manager/Cargo.toml ([[bin]] + tracing-subscriber)
- [x] ui/public/index.html (CSS + HTML + JS)

### 4. TEST & VERIFY
- [x] `cargo build` succeeds, binary produced
- [x] `cargo test` 12/12 pass (no regressions)

### 5. DOCUMENT
- [x] Session notes (this file)

### 6. REPORT
- [x] Summary to user
- [x] Scope not exceeded

---

## Files Summary

| File | Action |
|---|---|
| `connector-manager/src/main.rs` | Created |
| `connector-manager/Cargo.toml` | Modified ([[bin]], tracing-subscriber) |
| `ui/public/index.html` | Modified (connectors CSS + HTML + JS) |

---

## Session Metrics

- **Files created:** 1
- **Files modified:** 2
- **Tests:** 12/12 pass (unchanged from prior session)
- **Known gaps carried forward:** Credential enumeration on startup (Phase 2)
