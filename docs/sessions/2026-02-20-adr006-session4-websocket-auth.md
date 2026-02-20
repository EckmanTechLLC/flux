# Session: ADR-006 Session 4 — WebSocket Auth Enforcement

**Date:** 2026-02-20
**ADR:** docs/decisions/006-security-hardening.md § Session 4

---

## Objective

Validate bearer token on WebSocket upgrade when `auth_enabled=true`. Token
passed via `?token=` query param. Invalid/missing token: return 401 before
upgrade. Auth disabled: no change.

---

## Files Created

- `tests/websocket_auth_test.rs` — 4 integration tests

## Files Modified

- `src/api/websocket.rs` — Added `WsAppState.namespace_registry`,
  `WsAppState.auth_enabled`; added `ws_auth` tower middleware; added
  `create_ws_router()` builder function
- `src/api/mod.rs` — exported `create_ws_router`
- `src/main.rs` — switched from inline router to `create_ws_router(ws_state)`;
  pass `namespace_registry`/`auth_enabled` to `WsAppState`; removed unused
  `axum::routing::get` import

---

## Implementation Detail

Auth is implemented as an Axum `route_layer` middleware (`ws_auth`) rather
than in the handler body. This is required because `WebSocketUpgrade` is an
extractor that fails before handler code runs if the request doesn't have
Hyper's `OnUpgrade` extension. Middleware runs before extraction, so 401 is
returned cleanly even for non-upgrade test requests.

Token validation uses `registry.lookup_by_token(token)` — not
`validate_token()` — because WebSocket is a subscribe-only endpoint with no
specific namespace to check. Any valid token (belonging to any registered
namespace) grants access.

---

## Behavior

- `auth_enabled=false`: middleware is a no-op, all connections proceed.
- `auth_enabled=true`, no token: 401 Unauthorized.
- `auth_enabled=true`, unknown token: 401 Unauthorized.
- `auth_enabled=true`, valid token: connection proceeds to WebSocket upgrade.

---

## Tests

```
cargo test --test websocket_auth_test
```

4 tests, all passing:

- `test_auth_disabled_no_token_allowed`
- `test_auth_enabled_no_token_returns_401`
- `test_auth_enabled_invalid_token_returns_401`
- `test_auth_enabled_valid_token_not_rejected`

Note: tests use `tower::ServiceExt::oneshot`. When auth passes, requests reach
`WebSocketUpgrade` extraction which returns 426 (no Hyper `OnUpgrade` in test
requests). Tests assert `!= 401` for those cases. In production, the server
returns 101.

No regressions: 171 lib tests pass, 8 pre-existing namespace tests fail with
NATS ECONNREFUSED (unchanged from previous sessions).

---

## ADR-006 Status

All 4 sessions complete:
- [x] Session 1: Runtime Config Store + Admin API
- [x] Session 2: Body Size Limits
- [x] Session 3: Rate Limiting
- [x] Session 4: WebSocket Auth Enforcement
