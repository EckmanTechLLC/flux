# Session: Connector Disconnect Endpoint

**Date:** 2026-02-19
**Status:** Complete

## Changes

### 1. `src/api/connectors.rs` — DELETE /api/connectors/:name/token

New endpoint deletes stored credentials for a connector.

- Request: `DELETE /api/connectors/{name}/token`
- Auth disabled: deletes under "default" namespace
- Auth enabled: deletes under bearer token namespace
- Response: `{ "success": true }`
- Errors: 404 (unknown connector or no credential found), 500 (no credential store), 401 (bad token)

New type: `DeleteTokenResponse { success: bool }`
Registered route: `.route("/api/connectors/:name/token", delete(delete_token))`

### 2. `ui/public/index.html` — Disconnect button

- Replaced no-op `onclick="return false"` with `onclick="confirmDisconnect('${c.name}');return false"`
- Added `confirmDisconnect(name)` function:
  - Shows `confirm()` dialog before proceeding
  - On confirm: `DELETE /api/connectors/{name}/token`
  - On success: calls `loadConnectorStatus()`
  - On error: `alert()` with error message

### 3. `src/api/connectors/tests.rs` — unit test

- `test_delete_token_response_serialization`

## Tests

`cargo test --lib`: 167 passed (was 166), 8 pre-existing NATS failures
`cargo test --test connector_api_test`: 12 passed (was 9)

New integration tests:
- `test_delete_token_success` — stores then deletes, returns `{ success: true }`
- `test_delete_token_not_found` — no credential stored, returns 404
- `test_delete_token_no_credential_store` — no store configured, returns 500

## Files Modified

- `src/api/connectors.rs`
- `src/api/connectors/tests.rs`
- `ui/public/index.html`
- `tests/connector_api_test.rs`
