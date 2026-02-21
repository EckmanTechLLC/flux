# Session: ADR-007 Phase 3A Task 2 — Bento Generic Runner

**Date:** 2026-02-20
**Status:** Complete ✅

---

## What Was Done

Implemented the Bento subprocess runner for generic HTTP polling sources
(`connector-manager/src/runners/generic.rs`).

### Changes

**`connector-manager/src/runners/generic.rs`** — full rewrite:

- `GenericStatus` struct: `source_id`, `last_started`, `last_error`, `restart_count`
- `GenericRunner` extended with:
  - `flux_api_url: String`
  - `process_handles: Mutex<HashMap<String, tokio::process::Child>>`
  - `status_map: Arc<Mutex<HashMap<String, GenericStatus>>>`
- `GenericRunner::new(store, flux_api_url)` — updated signature
- `start_source(config, token)` — writes `/tmp/flux-bento-{id}.yaml`, spawns
  `bento -c <path>`, passes token as `FLUX_GENERIC_TOKEN` env var; gracefully
  skips if bento not found on PATH
- `stop_source(source_id)` — kills child process, removes temp config file
- `status()` — returns all `GenericStatus` entries
- `render_bento_config(config, flux_api_url)` — renders Bento YAML; auth is
  `${FLUX_GENERIC_TOKEN}` env var reference (never a literal token in the file)

**`CLAUDE.md`** — test count 38 → 41, Task 2 marked complete

### Key Design Decisions

- Auth tokens injected via `FLUX_GENERIC_TOKEN` env var; config file is safe to log
- `std::sync::Mutex` (not tokio) — no lock held across `.await` points
- `restart_count` starts at 0 on first start, increments on each subsequent call
- Bento not found on PATH → warning + `Ok(())` (non-fatal, allows testing without bento)

### Tests Added

3 new unit tests (no subprocess spawning):
- `test_render_bento_config_no_auth`
- `test_render_bento_config_bearer_token`
- `test_render_bento_config_api_key_header`

Each verifies: URL present, entity key present, namespace present, no literal token.

### Test Results

```
running 41 tests
...all passed...
Doc-tests: 3 passed
```

All 38 existing tests still pass. 3 new tests added.

---

## Next Steps

- Phase 3A Task 3: API endpoints (`POST /api/connectors/generic`, `DELETE /api/connectors/:id`)
- Phase 3A Task 4: UI form (Add Generic Source)
