# Session: Stale Scheduler Eviction on Credential Change

**Date:** 2026-02-19
**Status:** Complete

## Problem

The 60s discovery loop skipped `(user_id, connector)` pairs already in `status_map`.
If credentials were deleted and re-added, the old scheduler kept running with stale
cached credentials. New credentials were never picked up without restarting the container.

## Changes

### `connector-manager/src/manager.rs`

**Structural change:** Replaced `bg_scheduler_handles: Arc<Mutex<Vec<JoinHandle<()>>>>` with
`connector_handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>`.

- HashMap is keyed by `"user_id:connector"` — enables per-key abort/restart
- `scheduler_handles: Vec<JoinHandle<()>>` now holds only the discovery loop task

**`start_connector_for_user()`**
- Stores handle in `connector_handles` by key instead of `scheduler_handles`
- Aborts any existing handle for the same key before inserting the new one

**`run_discovery_cycle()` (new standalone async fn)**
- Extracted from the inline background task loop for testability
- Three passes per cycle:
  1. **Remove:** pairs in `status_map` not in credential store → abort handle, remove entry
  2. **Restart:** pairs in `status_map` with `last_error.is_some()` → abort old handle, fetch fresh credentials, start new scheduler, replace status Arc
  3. **Add:** pairs in credential store not in `status_map` → start new scheduler (previous behavior)
- Status snapshot taken without holding map lock during individual status reads (avoids nested lock)

**`shutdown()` / `Drop`**
- Now drains `connector_handles` instead of `bg_scheduler_handles`

## Tests

`cargo test` in `connector-manager/`: **24 passed** (was 22), 0 failed, 3 doc tests pass.

### Updated existing tests
- `test_start_connector_for_user` — checks `connector_handles.len()` instead of `scheduler_handles.len()`
- `test_shutdown` — checks `connector_handles.len()` before/after

### New tests
- `test_discovery_restarts_errored_scheduler` — pre-populates status_map with `last_error = Some(...)`, runs one cycle, asserts status Arc was replaced and new status has no error
- `test_discovery_removes_deleted_credentials` — pre-populates status_map with no matching credentials in store, runs one cycle, asserts entry removed from both `status_map` and `connector_handles`

## Files Modified

- `connector-manager/src/manager.rs`
