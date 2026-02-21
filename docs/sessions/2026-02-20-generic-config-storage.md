# Session: ADR-007 Phase 3A Task 1 — Generic Connector Config Storage

**Date:** 2026-02-20
**Status:** Complete

---

## Task

Implement config storage for generic HTTP polling sources (ADR-007 Phase 3A Task 1).

## Files Created / Modified

| File | Change |
|------|--------|
| `connector-manager/src/generic_config.rs` | New — `AuthType`, `GenericSourceConfig`, `GenericConfigStore`, 7 unit tests |
| `connector-manager/src/runners/generic.rs` | Updated — `GenericRunner` now holds `Arc<GenericConfigStore>` |
| `connector-manager/src/lib.rs` | Added `pub mod generic_config;` |

## What Was Implemented

### `generic_config.rs`

- **`AuthType`** enum: `None`, `BearerToken`, `ApiKeyHeader { header_name }` — serialized as JSON in SQLite.
- **`GenericSourceConfig`** struct: `id, name, url, poll_interval_secs, entity_key, namespace, auth_type, created_at`.
- **`GenericConfigStore`**: wraps `Mutex<rusqlite::Connection>`.
  - `new(db_path)` — opens DB, calls `create_table`.
  - `create_table()` — creates `generic_sources` table (idempotent).
  - `insert(config)` — inserts new row.
  - `get(id)` — returns `Option<GenericSourceConfig>`.
  - `list()` — returns all configs ordered by `created_at`.
  - `delete(id)` — removes row (no-op if not found).
- **Credential storage pattern** documented in module comment: tokens stored in existing `CredentialStore` under `user_id="generic"`, `connector_name=<source-id>`. No new infrastructure.

### `runners/generic.rs`

`GenericRunner` now holds `Arc<GenericConfigStore>`. No subprocess spawning yet (Task 2).

## Tests

7 new unit tests using `:memory:` SQLite:
- `test_insert_and_get` — roundtrip with `AuthType::None`
- `test_insert_and_get_bearer_token`
- `test_insert_and_get_api_key_header`
- `test_list_configs` — 3 rows
- `test_delete_config`
- `test_get_nonexistent_returns_none`
- `test_delete_nonexistent_is_noop`

## Test Results

```
running 38 tests
... (38 passed, 0 failed)
Doc-tests: 3 passed
```

Previous count: 31. New count: 38 (+7).

## Not Implemented (Out of Scope)

- Bento subprocess spawning (Task 2)
- API endpoints (Task 3)
- UI form (Task 4)

## Next Task

Phase 3A Task 2: Bento runner — render config template, spawn bento process, monitor.
