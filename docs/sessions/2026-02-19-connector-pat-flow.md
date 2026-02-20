# Session: GitHub PAT Connection Flow

**Date:** 2026-02-19
**Status:** Complete

## Changes

### 1. `src/api/connectors.rs` — POST /api/connectors/:name/token

New endpoint stores a personal access token as connector credentials.

- Request: `POST /api/connectors/{name}/token` with `{ "token": "<pat>" }`
- Auth disabled: stores under "default" namespace
- Auth enabled: stores under bearer token namespace
- Response: `{ "success": true }`
- Errors: 404 (unknown connector), 500 (no credential store), 401 (bad token)

New types: `TokenRequest { token: String }`, `StoreTokenResponse { success: bool }`
Added `InternalServerError` variant to `AppError`.

### 2. `connector-manager/src/manager.rs` — periodic credential discovery

Added background task spawned by `start()` that runs every 60 seconds:
- Calls `list_all()` on credential store
- Finds `(user_id, connector)` pairs not already in `status_map`
- Starts a `ConnectorScheduler` for each new pair
- Pushes new handles to `bg_scheduler_handles` (separate from main `scheduler_handles`)

New field: `bg_scheduler_handles: Arc<tokio::sync::Mutex<Vec<JoinHandle<()>>>>`
`shutdown()` now also drains `bg_scheduler_handles`.
`Drop` does best-effort abort via `try_lock`.

### 3. `ui/public/index.html` — PAT input form

Replaced OAuth start link with inline PAT form:
- "Connect" link shows form (hidden initially)
- Form: password input + "Connect" button + "Cancel" link
- On success: clears form, reloads connector status via `loadConnectorStatus()`
- On error: shows inline error message below input

New JS functions: `showPatForm(name)`, `cancelPatForm(name)`, `submitPat(name)`
No existing styles, colors, or layout changed.

## Tests

`flux/`:
- `cargo test --lib`: 166 passed (was 164), 8 pre-existing NATS failures
- `cargo test --test connector_api_test`: 9 passed (was 5)

New integration tests:
- `test_store_token_success` — stores PAT, returns `{ success: true }`
- `test_store_token_invalid_connector` — returns 404
- `test_store_token_no_credential_store` — returns 500
- `test_store_token_then_list_shows_configured` — stores token, verifies list shows "configured"

New unit tests in `src/api/connectors/tests.rs`:
- `test_token_request_deserialization`
- `test_store_token_response_serialization`

`connector-manager/`:
- `cargo test manager`: 4/4 pass (all existing tests unchanged)
- 3 pre-existing transformer test failures (unrelated to session scope)

## Files Modified

- `src/api/connectors.rs`
- `src/api/connectors/tests.rs`
- `connector-manager/src/manager.rs`
- `ui/public/index.html`
- `tests/connector_api_test.rs`
