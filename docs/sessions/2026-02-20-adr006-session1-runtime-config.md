# Session: ADR-006 Session 1 — Runtime Config Store + Admin API

**Date:** 2026-02-20
**ADR:** docs/decisions/006-security-hardening.md § Session 1

---

## Objective

Implement the foundation for ADR-006 security hardening: a runtime-configurable
`RuntimeConfig` struct and an admin HTTP API to read/update it without restart.

---

## Files Created

- `src/config/runtime.rs` — `RuntimeConfig` struct, `SharedRuntimeConfig` type alias,
  `new_runtime_config()` factory, env-var loader
- `src/api/admin.rs` — `AdminAppState`, `GET /api/admin/config`, `PUT /api/admin/config`
- `tests/admin_api_test.rs` — 5 integration tests

## Files Modified

- `src/config.rs` → `src/config/mod.rs` (converted to module directory; no logic changes)
- `src/config/mod.rs` — added `pub mod runtime` + re-exports
- `src/api/mod.rs` — registered `pub mod admin`, re-exported `create_admin_router` / `AdminAppState`
- `src/main.rs` — initialize `RuntimeConfig`, read `FLUX_ADMIN_TOKEN`, create and merge admin router

---

## Behavior

### GET /api/admin/config
- Open (no auth required in this session)
- Returns current `RuntimeConfig` as JSON

### PUT /api/admin/config
- Requires `Authorization: Bearer <FLUX_ADMIN_TOKEN>` header
- Returns 401 if token missing or wrong
- Accepts partial body — only present fields are updated
- Returns updated config as JSON

### RuntimeConfig defaults

| Field | Default |
|-------|---------|
| `rate_limit_enabled` | `true` |
| `rate_limit_per_namespace_per_minute` | `10000` |
| `body_size_limit_single_bytes` | `1048576` (1 MB) |
| `body_size_limit_batch_bytes` | `10485760` (10 MB) |

Initial values can be overridden via env vars:
- `FLUX_RATE_LIMIT_ENABLED`
- `FLUX_RATE_LIMIT_PER_NAMESPACE_PER_MINUTE`
- `FLUX_BODY_SIZE_LIMIT_SINGLE_BYTES`
- `FLUX_BODY_SIZE_LIMIT_BATCH_BYTES`

### Admin token

Set via `FLUX_ADMIN_TOKEN` env var. If unset, PUT is unrestricted (dev mode warning logged).

---

## Tests

```
cargo test --test admin_api_test
```

5 tests, all passing:
- `test_get_config_returns_defaults`
- `test_put_config_updates_fields`
- `test_put_config_wrong_token_returns_401`
- `test_put_config_missing_token_returns_401`
- `test_put_config_partial_update`

No regressions in existing 167 lib tests. 8 pre-existing namespace test failures
are NATS connection errors (ECONNREFUSED) unrelated to this session.

---

## Next Steps

- Session 2: Body size limits (reads `body_size_limit_*` from `RuntimeConfig`)
- Session 3: Rate limiting (reads `rate_limit_*` from `RuntimeConfig`)
- Session 4: WebSocket auth enforcement
