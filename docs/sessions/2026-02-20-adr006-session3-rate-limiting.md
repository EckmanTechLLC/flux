# Session: ADR-006 Session 3 — Rate Limiting

**Date:** 2026-02-20
**ADR:** docs/decisions/006-security-hardening.md § Session 3

---

## Objective

Per-namespace token bucket rate limiting on `POST /api/events` and
`POST /api/events/batch`. Active only when `auth_enabled=true`. Limit
is runtime-configurable via admin API.

---

## Files Created

- `src/rate_limit/mod.rs` — `RateLimiter` with `DashMap<String, TokenBucket>`
- `tests/rate_limit_test.rs` — 5 integration tests

## Files Modified

- `src/lib.rs` — added `pub mod rate_limit`
- `src/api/ingestion.rs` — added `rate_limiter: Arc<RateLimiter>` to `AppState`;
  rate limit check in `publish_event` (returns `AppError::RateLimited` → 429)
  and `publish_batch` (per-event check, failed results include error message);
  added `Retry-After: 60` header on 429; added `extract_namespace_from_event()` helper
- `src/api/namespace.rs` — added `rate_limiter` field + import to 5 `AppState`
  literals in `#[cfg(test)]` (compile fix)
- `src/main.rs` — initialize `Arc<RateLimiter>`, pass to `AppState`

---

## Behavior

### Rate limiting

- Active only when `auth_enabled=true`. No-op in internal mode.
- Per-namespace token bucket. Separate namespaces cannot starve each other.
- Bucket capacity = `rate_limit_per_namespace_per_minute` (default: 10,000).
- Refill rate = capacity / 60 tokens/second.
- Bucket created lazily on first event. Resets on restart (in-memory only).
- Limit change via `PUT /api/admin/config` takes effect immediately for new
  refill calculations (token count is continuous, so old bucket gets new rate).

### On exceed

- `publish_event`: `429 Too Many Requests` with `Retry-After: 60` header and
  `{"error": "rate limit exceeded"}`.
- `publish_batch`: that event marked failed with `"rate limit exceeded"` error;
  other events in the batch proceed normally.

### Namespace extraction

`extract_namespace_from_event()` parses `entity_id` from event payload
(same source as auth_middleware). Falls back to `event.stream` if missing.

---

## Tests

```
cargo test --test rate_limit_test
```

5 tests, all passing:

- `test_auth_disabled_bypasses_rate_limit`
- `test_within_limit_allowed`
- `test_exceeding_limit_returns_429_with_retry_after`
- `test_separate_namespaces_are_isolated`
- `test_runtime_config_rate_limit_defaults`

Unit tests for `TokenBucket` logic (allows, blocks when empty, separate buckets,
refill) live in `src/rate_limit/mod.rs`.

No regressions: 171 lib tests pass, 8 pre-existing namespace tests fail with
NATS ECONNREFUSED (unchanged from previous sessions).

---

## Next Steps

- Session 4: WebSocket auth enforcement (`src/api/websocket.rs`)
