# Session: Namespace Admin Gate

**Date:** 2026-02-23
**Status:** Complete

## Goal

Gate `POST /api/namespaces` behind `FLUX_ADMIN_TOKEN` to prevent open namespace creation on the public instance.

## Changes

### `src/api/ingestion.rs`
- Added `admin_token: Option<String>` field to `AppState`

### `src/api/namespace.rs`
- Added `HeaderMap` import
- Added `Unauthorized` variant to `NamespaceError` (returns 401)
- `register_namespace` handler: after `auth_enabled` check, validates `Authorization: Bearer <token>` if `admin_token` is `Some`
- If `admin_token` is `None` → no check (open, for self-hosted)
- Fixed pre-existing bug: `create_test_publisher()` used wrong NATS port (`4222` → `4223`)
- Updated all existing `AppState` struct literals in tests to include `admin_token: None`
- Added `create_test_app_with_token(auth_enabled, admin_token)` helper
- Added two new tests:
  - `test_register_namespace_requires_admin_token` — returns 401 without token
  - `test_register_namespace_accepts_admin_token` — returns 200 with correct token

### `src/main.rs`
- Pass `admin_token: admin_token.clone()` into `AppState`

### `connector-manager/src/runners/generic.rs`
- Fixed pre-existing compile error: `MutexGuard` held across `.await` — wrapped lock block in explicit scope to drop guard before sleep

## Test Results

187 passed, 0 failed (all tests green)

## Deploy

```
docker compose build --no-cache flux && docker compose up -d flux
```

## Behavior

| Condition | Result |
|-----------|--------|
| `FLUX_ADMIN_TOKEN` not set | Endpoint open (dev/self-hosted) |
| `FLUX_ADMIN_TOKEN` set, no header | 401 Unauthorized |
| `FLUX_ADMIN_TOKEN` set, wrong token | 401 Unauthorized |
| `FLUX_ADMIN_TOKEN` set, correct token | 200 OK |
| `FLUX_AUTH_ENABLED=false` | 404 (auth disabled, unchanged) |
