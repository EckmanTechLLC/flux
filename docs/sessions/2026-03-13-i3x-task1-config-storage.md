# Session: i3X Connector — Task 1: Config Storage

**Date:** 2026-03-13
**ADR:** ADR-010 (i3X Connector)
**Status:** Complete

---

## What Was Done

- Created `connector-manager/src/i3x_config.rs`
  - `I3xSourceConfig` struct: `id`, `name`, `base_url`, `namespace`, `flux_namespace_token`, `created_at`
  - `I3xConfigStore`: SQLite-backed store with `new`, `create_table`, `insert`, `get`, `list`, `delete`
  - `row_to_config` helper (private)
  - No `migrate()` needed — new table, no legacy DB to migrate
  - API key NOT stored here; goes in CredentialStore under `"i3x/{source_id}"`
- Registered module in `connector-manager/src/lib.rs` as `pub mod i3x_config`
- 5 unit tests (in-memory SQLite `:memory:`): insert/get, list, delete, get-nonexistent, delete-nonexistent

## Files Modified

- `connector-manager/src/i3x_config.rs` — new
- `connector-manager/src/lib.rs` — added `pub mod i3x_config`

## Verify

```
cd /home/etl/projects/flux && cargo test -p connector-manager i3x_config
```

Expected: 5 tests pass.

## Next

Task 2: `runners/i3x.rs` — `I3xRunner`, SSE streaming, Flux event publishing
