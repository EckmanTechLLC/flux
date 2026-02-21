# Session: Generic Connector API Endpoints

**Date:** 2026-02-20
**Task:** ADR-007 Phase 3A Task 3 — Generic Connector API Endpoints
**Status:** Complete ✅

---

## What Was Done

Added HTTP API endpoints to the connector-manager binary for managing generic (Bento) sources.

### New Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/connectors/generic` | Create generic source |
| `DELETE` | `/api/connectors/generic/:source_id` | Remove generic source |
| `GET` | `/api/connectors` | List all connectors (builtin + generic) |

### Files Created/Modified

| File | Change |
|------|--------|
| `connector-manager/src/api.rs` | **New** — handlers, business logic, types, tests |
| `connector-manager/src/lib.rs` | Added `pub mod api;` |
| `connector-manager/src/main.rs` | Added GenericConfigStore/Runner init, HTTP server on port 3001 |
| `connector-manager/Cargo.toml` | Added `axum = "0.7"`, added `v4` to uuid features |

---

## Key Design Decisions

**Connector-manager has its own HTTP server (port 3001).**
Main Flux API runs on 3000. Connector-manager was previously CLI-only; now it also serves API requests.

**Business logic extracted from HTTP handlers.**
`handle_create_generic_source` and `handle_delete_generic_source` are public async functions that can be called from unit tests without an HTTP stack.

**Auth type input shape matches ADR-007 spec:**
- `"none"` or `"bearer"` as a plain string
- `{ "api_key_header": "X-API-Key" }` as an object
Uses `serde(untagged)` enum to deserialize both forms.

**Token never logged.** Stored in `CredentialStore` under `user_id="generic", connector_name=<source_id>`. Passed directly to `GenericRunner::start_source` (not re-fetched).

**Persisted sources restarted on startup.** `main.rs` iterates `GenericConfigStore.list()` at boot and calls `start_source` for each, recovering tokens from CredentialStore.

**Bento not found = ok.** If `bento` binary is absent from PATH, `start_source` logs a warning and returns `Ok(())`. Tests pass without bento installed.

---

## Test Results

```
cargo test -p connector-manager

running 43 tests
...
test api::tests::test_delete_generic_source_removes_config ... ok
test api::tests::test_post_generic_source_stores_config ... ok
...
test result: ok. 43 passed; 0 failed; 0 ignored
Doc-tests: 3 passed
```

41 original tests + 2 new = 43 total, all passing.

---

## Environment Variables Added

| Variable | Default | Description |
|----------|---------|-------------|
| `GENERIC_CONFIG_DB` | `generic_config.db` | SQLite path for generic source configs |
| `CONNECTOR_API_PORT` | `3001` | Port for connector-manager HTTP API |

---

## Next Steps

- ADR-007 Phase 3A Task 4: UI form (Add Generic Source)
- ADR-007 Phase 3B: Named connector (Singer taps)
