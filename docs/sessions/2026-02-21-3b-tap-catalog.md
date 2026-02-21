# Session: ADR-007 Phase 3B Task 1 — Singer Tap Catalog

**Date:** 2026-02-21
**Status:** Complete ✅

---

## Task

Implement the Meltano Hub tap catalog fetch, cache, and API endpoint.
Singer subprocess runner is Phase 3B Session 2 — out of scope here.

---

## What Was Built

### `connector-manager/src/runners/named.rs`
Replaced stub with:
- `TapCatalogEntry` — public struct: `{ name, label, description, pip_url, logo_url }`
- `TapCatalogStore` — fetch, cache (disk), and serve the catalog
  - `new(cache_path)` — loads from disk if cache exists, else empty
  - `list()` — returns in-memory catalog (may be empty before first refresh)
  - `needs_refresh()` — true if cache absent or > 24 hours old
  - `refresh()` — fetches Meltano Hub index, updates memory + disk cache
- `NamedRunner` stub retained for Phase 3B Session 2
- 4 unit tests: label derivation, empty store, save/load round-trip, fresh cache TTL

### `connector-manager/src/api.rs`
- Added `tap_catalog: Arc<TapCatalogStore>` to `ApiState`
- Added `GET /api/connectors/taps` → returns `Vec<TapCatalogEntry>` as JSON
- Updated test `make_state()` helper

### `connector-manager/src/main.rs`
- Adds `TAP_CATALOG_CACHE` env var (default: `/tmp/flux-tap-catalog.json`)
- Creates `TapCatalogStore` at startup
- Spawns background tokio task: calls `refresh()` if `needs_refresh()` — non-blocking
- Passes `tap_catalog` into `ApiState`

---

## Meltano Hub API Notes

- Index URL: `https://hub.meltano.com/meltano/api/v1/plugins/extractors/index`
  (redirects to AWS API Gateway — reqwest follows automatically)
- Index response: flat object `{ "tap-name": { default_variant, variants, logo_url } }`
- **No label/description/pip_url in the index** — only name (key) and logo_url
- Strategy: `label` derived from name (strip "tap-", title-case), `pip_url = name`,
  `description = ""`. Full per-tap detail can be fetched in Session 2 when user selects a tap.

---

## Decisions

- **Index-only fetch (single HTTP request)**: avoids 300+ per-tap requests at startup.
  The index gives us name + logo. Label is derived. pip_url = name (works for most taps).
- **Non-blocking background refresh**: startup is not gated on Meltano Hub availability.
  If the hub is down, catalog is empty; error is logged.
- **Daily TTL (86400s)**: tap catalog changes rarely; daily refresh is sufficient.
- **`/tmp/flux-tap-catalog.json`**: default cache path, overridable via `TAP_CATALOG_CACHE`.

---

## Test Results

```
test runners::named::tests::test_derive_label ... ok
test runners::named::tests::test_catalog_store_empty_on_missing_cache ... ok
test runners::named::tests::test_needs_refresh_false_for_fresh_cache ... ok
test runners::named::tests::test_save_and_load_catalog ... ok

47 passed; 0 failed
```

---

## Files Modified

- `connector-manager/src/runners/named.rs` — full replacement of stub
- `connector-manager/src/api.rs` — ApiState + new endpoint + route
- `connector-manager/src/main.rs` — catalog init + background task

---

## Next Session (3B Session 2)

Singer runner (`NamedRunner`):
- `POST /api/connectors/named/:tap` — create a named tap source
- Install tap via `pip install` if not present
- Write temp config JSON, spawn tap subprocess
- Parse Singer RECORD/STATE/SCHEMA stdout → Flux events
- Reschedule after tap exits
