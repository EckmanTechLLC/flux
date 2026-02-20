# Session: Generic OAuth Token Refresh

**Date:** 2026-02-20
**Status:** Complete

## Changes

### `connector-manager/src/scheduler.rs`

**New field:** `credential_store: Arc<CredentialStore>`

**Constructor:** `ConnectorScheduler::new()` now requires `credential_store: Arc<CredentialStore>` as the final parameter.

**`needs_refresh()` (private)**
- Returns true when `expires_at` is within 90 seconds (or past) AND `refresh_token` is `Some`
- PAT connectors (no `expires_at` or no `refresh_token`) return false → unaffected

**`try_refresh_token()` (private async &mut self)**
- POSTs `grant_type=refresh_token` + `refresh_token` to `connector.oauth_config().token_url`
- Includes `client_id` / `client_secret` from environment if `FLUX_OAUTH_{CONNECTOR}_CLIENT_ID` / `FLUX_OAUTH_{CONNECTOR}_CLIENT_SECRET` are set (follows existing provider.rs pattern)
- On success: updates `self.credentials` in memory + persists via `credential_store.store()`
- Keeps existing refresh token if provider response omits it (no rotation)
- Returns `Err` on non-2xx response or network failure

**`start()` loop**
- Changed `self` usage to `let mut scheduler = self` for mutability
- Before each poll: calls `scheduler.needs_refresh()` → if true, calls `scheduler.try_refresh_token()`
- On refresh failure: logs error, increments `error_count`, `continue` (skips poll)
- Fetch/publish logic unchanged

### `connector-manager/src/manager.rs`

Three `ConnectorScheduler::new()` call sites updated to pass `Arc::clone(&self.credential_store)` or `Arc::clone(cred_store)`:
- `start_connector_for_user()`
- `run_discovery_cycle()` restart branch
- `run_discovery_cycle()` new pairs branch

### `connector-manager/src/types.rs`

No change — `OAuthConfig.token_url` already present.

## Tests

`cargo test` in `connector-manager/`: **31 passed** (was 24), 0 failed, 3 doc tests pass.

### New tests (scheduler module)

**`needs_refresh` logic (5 tests):**
- `test_needs_refresh_no_refresh_token` — no refresh_token → false
- `test_needs_refresh_no_expiry` — no expires_at → false
- `test_needs_refresh_far_future` — expires in 2 hours → false
- `test_needs_refresh_near_expiry` — expires in 30 seconds → true
- `test_needs_refresh_already_expired` — expired 1 second ago → true

**`try_refresh_token` behavior (2 tests, using mockito):**
- `test_try_refresh_token_success` — mock returns 200 + token; verifies `credentials.access_token` updated, original refresh_token retained, credential store updated
- `test_try_refresh_token_http_failure` — mock returns 400; verifies Err returned, credentials unchanged

### Updated existing tests
- `test_scheduler_status` — passes `make_store()` to constructor
- `test_fetch_and_publish_no_server` — passes `make_store()` to constructor

## Files Modified

- `connector-manager/src/scheduler.rs`
- `connector-manager/src/manager.rs` (3 constructor call sites)
