# Session: Connector Manager Core

**Date:** 2026-02-17
**Task:** Phase 1, Task 4 - Connector Manager Core
**Reference:** ADR-005 lines 244-248
**Status:** ✅ COMPLETE (with known gap documented below)

---

## Objective

Implement the core connector manager that polls connectors on schedule, fetches data, and publishes events to Flux.

## Files Created

1. **connector-manager/src/registry.rs** - Connector registry
   - `get_all_connectors()` - Returns all available connectors (Phase 1: just mock GitHub)
   - `MockGitHubConnector` - Implements Connector trait, returns 2 fake events per poll
   - No real GitHub API calls (pure mock data)
   - 3 unit tests

2. **connector-manager/src/scheduler.rs** - Per-connector polling scheduler
   - `ConnectorScheduler` - Tokio interval loop per connector instance
   - `ConnectorStatus` - Tracks last_poll, last_error, poll_count, error_count
   - `fetch_and_publish_with_retry()` - Exponential backoff: 60s, 120s, 240s (max 3 retries)
   - `publish_events()` - HTTP POST to Flux API with Authorization header
   - 2 unit tests

3. **connector-manager/src/manager.rs** - ConnectorManager orchestration
   - `ConnectorManager::new(credential_store, flux_api_url)` - Constructor
   - `start()` - Logs available connectors (see Known Gap below)
   - `start_connector_for_user(user_id, connector_name)` - Starts scheduler for one user
   - `shutdown()` - Aborts all running scheduler tasks
   - Status map: `Arc<Mutex<HashMap<String, Arc<Mutex<ConnectorStatus>>>>>` for future Status API
   - 4 unit tests

## Files Modified

1. **connector-manager/Cargo.toml** - Added dependencies
   ```toml
   tokio = { version = "1.0", features = ["full"] }
   reqwest = { version = "0.11", features = ["json"] }
   uuid = { version = "1.0", features = ["v7", "serde"] }
   tracing = "0.1"
   [dev-dependencies]
   tempfile = "3.0"
   ```

2. **connector-manager/src/lib.rs** - Exported new modules
   - Added: `pub mod manager;`, `pub mod registry;`, `pub mod scheduler;`
   - Re-exported: `pub use manager::ConnectorManager;`

3. **connector-manager/src/connector.rs** - Fixed imports
   - Changed `use crate::types::{Credentials, OAuthConfig};` → separate imports
   - `Credentials` re-exported from crate root (via flux)

## Architecture Note: Separate Binary

**Important:** The connector manager is a separate binary, not embedded in `flux`.

Per ADR-005, the connector manager runs independently and communicates with Flux via the HTTP API. It must NOT be imported into the flux crate as a dependency — this would create a circular dependency (flux → connector-manager → flux).

The connector manager binary will have its own `main.rs` in `connector-manager/src/`. For Phase 1, it is started manually. In Phase 2+, it will be managed by Docker Compose alongside flux.

## Known Gap: Credential Enumeration on Startup

`ConnectorManager::start()` cannot auto-start schedulers on startup because `CredentialStore` has no method to list all stored credentials.

**Current behavior:** `start()` logs available connectors but starts 0 schedulers. Schedulers are started on-demand via `start_connector_for_user()`.

**Impact:** After a connector-manager restart, polling does not resume automatically for existing users.

**Fix needed (Phase 2):** Add `list_all_credentials() -> Vec<(user_id, connector)>` to `CredentialStore` in flux crate. The manager can then iterate and start schedulers for each entry on startup.

**Workaround (Phase 1):** For testing, call `start_connector_for_user()` directly with known user/connector pairs. In production, users re-authorize via OAuth on restart (acceptable for Phase 1).

## Mock GitHub Connector

Returns 2 fake events per poll:

1. `github/repo/test-repo` (schema: `github.repository`)
   - Properties: name, full_name, stars (42), forks (10), language, description
2. `github/notifications/count` (schema: `github.notifications`)
   - Properties: unread_count (3), last_checked

Both events use `stream: "connectors"`, `source: "connector-manager"`.

## Event Publishing

Events published one at a time to `POST {FLUX_API_URL}/api/events`.

Authorization header: `Bearer {user_id}` (namespace token).

Batch-per-fetch: all events from one `fetch()` call are published before the next poll.

## Configuration

| Variable | Default | Description |
|---|---|---|
| `FLUX_API_URL` | `http://localhost:3000` | Flux API base URL |
| `FLUX_CONNECTOR_MANAGER_ENABLED` | `true` | Enable/disable manager |

## Testing

### Build
```bash
cd connector-manager && cargo build
```
Result: ✅ Clean (warnings: deprecated `base64::encode` in test code only)

### Unit Tests
```bash
cd connector-manager && cargo test
```
Result: ✅ 9/9 unit tests + 3/3 doc tests

**Tests:**
- ✅ registry: mock GitHub connector (name, interval, oauth_config)
- ✅ registry: mock fetch returns 2 events with correct structure
- ✅ registry: get_all_connectors returns 1 connector
- ✅ scheduler: status initializes empty
- ✅ scheduler: fetch_and_publish fails cleanly when Flux unreachable
- ✅ manager: creation initializes with no schedulers
- ✅ manager: start_connector_for_user with valid credentials starts scheduler
- ✅ manager: start_connector_for_user with missing credentials returns error
- ✅ manager: shutdown aborts all tasks

### Flux Tests (no regression)
```bash
cargo test --lib  # in flux/
```
Result: 164/172 pass; 8 fail (pre-existing: namespace tests require NATS)

## Session Notes

### Git State Fix
At the start of this session, a stash was discovered (`git stash list`):
- `stash@{0}: WIP on main: 897b6b3 Implement delimiter-aware grouping thresholds`

The stash contained the tracked file changes for the connector framework (credential_store init, OAuth router, connector API router in main.rs). After popping the stash, main.rs was already in the correct state — no connector_manager import needed.

### What Worked Well
- Tokio interval loop is clean and easy to reason about
- Exponential backoff implemented simply with explicit delay array
- Status tracking with shared `Arc<Mutex>` is straightforward
- Test coverage with tempfile is effective for credential store tests

### What Didn't Work
- **FluxEvent field types:** Initial mock connector used wrong types (String instead of Option<String> for event_id/key/schema, DateTime instead of i64 for timestamp). Fixed after reading `src/event/mod.rs`.
- **Circular dependency concern:** Almost added `connector_manager` as a dep in `flux/Cargo.toml` + `src/main.rs`. Caught and corrected — separate binary is the right architecture.

## Checklist Completion

### 1. READ FIRST
- [x] Read CLAUDE.md
- [x] Read ADR-005 (lines 244-291)
- [x] Read all prior session notes (connector-interface, credential-storage, oauth-flow)
- [x] Read existing code (lib.rs, connector.rs, main.rs, FluxEvent)

### 2. VERIFY ASSUMPTIONS
- [x] Confirmed FluxEvent field types before writing mock
- [x] Confirmed Credentials is re-exported from crate root
- [x] Confirmed CredentialStore::get() interface
- [x] Listed files before creating them

### 3. MAKE CHANGES
- [x] registry.rs (MockGitHubConnector)
- [x] scheduler.rs (ConnectorScheduler, ConnectorStatus)
- [x] manager.rs (ConnectorManager)
- [x] Updated lib.rs exports
- [x] Updated Cargo.toml dependencies

### 4. TEST & VERIFY
- [x] Build succeeds
- [x] 12/12 connector-manager tests pass
- [x] No new test failures in flux
- [x] Namespace failures confirmed pre-existing (NATS)

### 5. DOCUMENT
- [x] Session notes (this file)
- [x] Known gap (credential enumeration) documented
- [x] Architectural note (separate binary) documented

### 6. REPORT
- [x] Summary provided
- [x] Blockers documented (list_all_credentials)
- [x] Scope not exceeded

---

## Next Steps (Phase 1)

### Task 5: Connector Status API (Small)
- Files: `flux/src/api/connectors/` (already scaffolded)
- Scope: `GET /api/connectors` - list available connectors with status
- Integration: Read from ConnectorManager status_map

### Phase 2 Pre-requisite
- Add `list_all_credentials()` to `CredentialStore` (resolves Known Gap above)
- Add `main.rs` entry point to connector-manager binary
- Wire into Docker Compose

---

## Session Metrics

- **Files created:** 3 (registry.rs, scheduler.rs, manager.rs)
- **Files modified:** 3 (lib.rs, Cargo.toml, connector.rs)
- **Tests added:** 9 unit + 3 doc = 12
- **Tests passing:** 12/12 connector-manager; 164/172 flux (8 pre-existing NATS failures)
- **Known gaps:** 1 (credential enumeration on startup)
