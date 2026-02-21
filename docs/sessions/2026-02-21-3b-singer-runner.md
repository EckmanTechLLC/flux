# Session: ADR-007 Phase 3B Task 2+3 — Singer Runner and State Management

**Date:** 2026-02-21
**Status:** Complete ✅

---

## Task

Implement `NamedSourceConfig` + `NamedConfigStore` (SQLite), `NamedRunner`
(Singer tap subprocess management), and state file persistence for incremental sync.
API endpoints and UI are Phase 3B Session 3.

---

## What Was Built

### `connector-manager/src/named_config.rs` (new)
SQLite-backed config store for Singer tap sources:
- `NamedSourceConfig` — `{ id, tap_name, namespace, entity_key_field, config_json, poll_interval_secs, created_at }`
- `NamedConfigStore` — `new`, `insert`, `get`, `list`, `delete` — table `named_sources`
- 5 unit tests: insert/get, list, delete, nonexistent get, nonexistent delete no-op

### `connector-manager/src/runners/named.rs` (extended)
Added alongside existing `TapCatalogStore`:

**`NamedStatus`** — `{ source_id, tap_name, last_run, last_error, restart_count }`

**`NamedRunner`** — manages Singer tap subprocesses:
- `new(store, flux_api_url)`
- `start_source(config)` — spawns background `run_tap_loop` task
- `stop_source(source_id)` — aborts task, removes temp files
- `status()` → `Vec<NamedStatus>`

**`run_tap_loop`** (private) — long-running tokio task:
1. Updates `last_run` timestamp in status map
2. Calls `run_tap_once`; updates `last_error` / `restart_count` on result
3. Sleeps `poll_interval_secs`, then repeats

**`run_tap_once`** (private) — single tap invocation:
1. Writes `config_json` to `/tmp/flux-tap-{id}-config.json` with mode 0600
2. If `/tmp/flux-tap-{id}-state.json` exists, passes `--state <path>` to tap
3. Spawns `{tap_name} --config <path>` with stdout piped, stderr null
4. Reads stdout line by line; parses Singer JSON:
   - `SCHEMA` → ignored
   - `RECORD` → maps to Flux event, POSTs to `{flux_api_url}/api/events`
   - `STATE` → writes `value` to state file (incremental sync bookmark)
5. Waits for tap exit; logs non-zero exit codes
6. Removes config file; state file kept for next run

**`value_to_string`** — converts JSON value to entity key string (string → clone, number/bool/null → to_string)

2 new unit tests: `test_value_to_string`, `test_named_runner_status_empty`

### Singer → Flux Event Mapping
```
stream  = "taps.{tap_name}.{singer_stream}"
source  = "tap.{tap_name}"
timestamp = Utc::now().timestamp_millis()
key     = record[entity_key_field] (fallback: first field value)
payload = { "entity_id": "{namespace}/{key}", "properties": record }
```

### State Persistence
- State file: `/tmp/flux-tap-{id}-state.json`
- Written on each `STATE` Singer message (overwrites previous)
- Read on next run via `--state` flag
- Removed on `stop_source`; kept across poll intervals

### `connector-manager/src/lib.rs` (modified)
Added `pub mod named_config;`

### `connector-manager/src/api.rs` (modified)
- Added `NamedRunner` import
- Added `named_runner: Arc<NamedRunner>` to `ApiState`
- Updated `list_connectors` (`GET /api/connectors`) to include named sources
  from `named_runner.store.list()` + `named_runner.status()`
- Updated `make_state()` test helper

### `connector-manager/src/main.rs` (modified)
- Added `NAMED_CONFIG_DB` env var (default: `named_config.db`)
- Creates `NamedConfigStore` + `NamedRunner` at startup
- Restarts persisted named sources from DB on startup
- Passes `named_runner` to `ApiState`

---

## Decisions

- **Temp file permissions**: config written 0600 (credentials); state file has no permissions restriction (it's just a bookmark, not sensitive)
- **stderr discarded**: tap stderr goes to null. Non-zero exit codes logged. Complex stderr capture deferred to polish phase.
- **State file path**: `/tmp/` — survives across container restarts in the same session but not across container recreation. This is acceptable for MVP; persistent state path via env var is a future improvement.
- **tap_name is the command**: stored as `"tap-github"`, invoked as `"tap-github"`. No transformation.
- **No install logic**: if tap is not on PATH, `run_tap_once` returns `Err(...)` which becomes `last_error`. User must `pip install` the tap. Auto-install is Phase 3C polish.

---

## Test Results

```
running 54 tests
... all named_config and runners::named tests pass ...
test named_config::tests::test_delete_config ... ok
test named_config::tests::test_delete_nonexistent_is_noop ... ok
test named_config::tests::test_get_nonexistent_returns_none ... ok
test named_config::tests::test_insert_and_get ... ok
test named_config::tests::test_list_configs ... ok
test runners::named::tests::test_catalog_store_empty_on_missing_cache ... ok
test runners::named::tests::test_derive_label ... ok
test runners::named::tests::test_named_runner_status_empty ... ok
test runners::named::tests::test_save_and_load_catalog ... ok
test runners::named::tests::test_value_to_string ... ok
test runners::named::tests::test_needs_refresh_false_for_fresh_cache ... ok

test result: ok. 54 passed; 0 failed; 0 ignored
```

---

## Files Modified

- `connector-manager/src/named_config.rs` — new file (193 lines)
- `connector-manager/src/runners/named.rs` — extended (added NamedRunner + Singer runner)
- `connector-manager/src/lib.rs` — added `pub mod named_config;`
- `connector-manager/src/api.rs` — ApiState + list_connectors + test helper
- `connector-manager/src/main.rs` — named store/runner init + startup restart

---

## Next Session (3B Session 3)

API endpoints + UI:
- `POST /api/connectors/named/:tap` — create a named tap source
- `DELETE /api/connectors/named/:source_id` — remove named source
- UI catalog form: pick tap, fill credentials, set namespace/key field/poll interval
- `GET /api/connectors` already includes named sources (done this session)
