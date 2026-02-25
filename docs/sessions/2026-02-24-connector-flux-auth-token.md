# Session: Connector Flux Auth Token

**Date:** 2026-02-24
**Status:** Complete ✅

## Task

Add Flux namespace token support to Generic (Bento) and Named (Singer) connector runners so they can publish events to auth-enabled Flux instances (`FLUX_AUTH_ENABLED=true`).

## Changes

### connector-manager/src/generic_config.rs
- Added `flux_namespace_token: Option<String>` field to `GenericSourceConfig`
- Added `flux_namespace_token TEXT` column to `CREATE TABLE` schema
- Added `migrate()` method (ALTER TABLE + ignore duplicate column error for existing DBs)
- Called `store.migrate()?` in `new()` after `create_table()`
- Updated `insert()` to persist `flux_namespace_token` (param `?9`)
- Updated SELECT queries to include `flux_namespace_token`
- Updated `row_to_config()` to read field at index 8
- Updated test `sample_config()` to set `flux_namespace_token: None`

### connector-manager/src/named_config.rs
- Same changes as above for `NamedSourceConfig` and `named_sources` table
- New field is at SELECT index 7 (named table has one fewer existing column)

### connector-manager/src/runners/generic.rs
- `render_bento_config` signature: added `flux_namespace_token: Option<&str>` param
- When `Some`, renders `Authorization: "Bearer ${FLUX_OUTPUT_TOKEN}"` in output http_client headers
- `run_bento_loop`: passes `config.flux_namespace_token` as `FLUX_OUTPUT_TOKEN` env var to bento
- Updated all test calls to pass `None`; added 2 new tests:
  - `test_render_bento_config_with_flux_token` — verifies output auth header added
  - `test_render_bento_config_bearer_with_flux_token` — verifies both headers present

### connector-manager/src/runners/named.rs
- `run_tap_once`: adds `Authorization: Bearer <token>` header to reqwest publish call when `config.flux_namespace_token` is `Some`

### connector-manager/src/api.rs
- Added `flux_namespace_token: Option<String>` to `CreateGenericSourceRequest`
- Added `flux_namespace_token: Option<String>` to `CreateNamedSourceRequest`
- `handle_create_generic_source`: sets `config.flux_namespace_token` from request
- `handle_create_named_source`: sets `config.flux_namespace_token` from request
- Updated test helpers `make_request()` and `make_named_request()` to include `flux_namespace_token: None`

### ui/public/index.html
- Added `gsFluxToken` password input to Generic source form (after gsHeaderNameField)
- Added `nsFluxToken` password input to Named source form (after nsConfigJson)
- `addGenericSource()`: reads `gsFluxToken`, sends `flux_namespace_token: fluxToken || null`
- `addNamedSource()`: reads `nsFluxToken`, sends `flux_namespace_token: fluxToken || null`
- Both functions reset their flux token fields on success

## Tests

- Flux core: 196 passed (no regressions)
- connector-manager: 58 passed (includes 2 new render tests)
- Doc tests: 3 passed

## How to deploy

```
docker compose build --no-cache connector-manager flux-ui && docker compose up -d connector-manager flux-ui
```

## Usage

When creating a Generic or Named source via the UI or API, supply the Flux namespace token in the `flux_namespace_token` field. It is:
- Persisted in SQLite (plain text, same treatment as tap `config_json`)
- Passed as `FLUX_OUTPUT_TOKEN` env var to bento (never written to YAML config)
- Added as `Authorization: Bearer` header on every reqwest publish call in named runner
