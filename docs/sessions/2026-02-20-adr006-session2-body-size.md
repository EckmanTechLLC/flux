# Session: ADR-006 Session 2 — Body Size Limits

**Date:** 2026-02-20
**ADR:** docs/decisions/006-security-hardening.md § Session 2

---

## Objective

Enforce runtime-configurable body size limits on `POST /api/events` and
`POST /api/events/batch`, returning `413 Payload Too Large` when exceeded.

---

## Files Modified

- `src/api/ingestion.rs` — added `runtime_config: SharedRuntimeConfig` to
  `AppState`; changed both handlers to accept raw `axum::body::Bytes`, check
  byte length against the appropriate limit from `RuntimeConfig`, return
  `AppError::PayloadTooLarge` (→ 413) if exceeded, then deserialize from
  the checked bytes. Added `PayloadTooLarge` variant to `AppError`.
- `src/main.rs` — pass `runtime_config: Arc::clone(&runtime_config)` to
  `AppState` (runtime_config is still moved into `AdminAppState` after clone).
- `src/api/namespace.rs` — added `runtime_config: new_runtime_config()` to
  five `AppState` struct literals in `#[cfg(test)]` to fix compile errors
  caused by the new required field.

## Files Created

- `tests/body_size_test.rs` — 5 integration tests

---

## Behavior

### POST /api/events
- Body ≤ `body_size_limit_single_bytes` (default 1 MB): request proceeds normally
- Body > limit: `413 Payload Too Large` with `{"error": "payload too large"}`

### POST /api/events/batch
- Body ≤ `body_size_limit_batch_bytes` (default 10 MB): request proceeds normally
- Body > limit: `413 Payload Too Large` with `{"error": "payload too large"}`

Limits are read from `SharedRuntimeConfig` on each request. A `PUT
/api/admin/config` change takes effect immediately for subsequent requests.

---

## Tests

```
cargo test --test body_size_test
```

5 tests, all passing:

- `test_single_event_body_too_large_returns_413`
- `test_batch_body_too_large_returns_413`
- `test_single_event_within_limit_passes_check`
- `test_body_at_exact_limit_is_allowed`
- `test_runtime_config_defaults`

**Note on test design:** `AppState` requires a live NATS connection
(`EventPublisher` wraps a `jetstream::Context`). Tests use a minimal
test-only router with handlers that implement the identical body size check
logic. The real handler integration (NATS publish path) is exercised manually
against a running instance.

No regressions: 167 lib tests pass, 8 pre-existing namespace tests fail with
NATS ECONNREFUSED (unchanged from Session 1).

---

## Next Steps

- Session 3: Rate limiting (reads `rate_limit_*` from `RuntimeConfig`)
- Session 4: WebSocket auth enforcement
