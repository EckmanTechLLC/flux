# Session: Remove WebSocket Auth Gate

**Date:** 2026-02-23
**Status:** Complete

## Problem

WebSocket subscriptions are read-only (subscribers receive state, never write). The `ws_auth`
middleware from ADR-006 incorrectly required a namespace token for WS connections when
`FLUX_AUTH_ENABLED=true`, breaking the operator dashboard.

Auth should only gate publishing (POST /api/events).

## Changes

### `src/api/websocket.rs`
- Removed `WsQuery` struct
- Removed `ws_auth` middleware fn
- Removed unused imports: `NamespaceRegistry`, `Query`, `Request`, `StatusCode`, `middleware`, `Next`, `Deserialize`
- Simplified `WsAppState`: removed `namespace_registry` and `auth_enabled` fields
- Simplified `create_ws_router`: removed `.route_layer(...)` call

### `src/main.rs`
- Updated `WsAppState` construction: only passes `state_engine`

### `tests/websocket_auth_test.rs`
- Deleted — all tests were for the removed `ws_auth` behavior

## Test Count Change

Was: 142 Flux core + 43 connector-manager + 3 doc tests
Now: 138 Flux core (4 WS auth tests removed) + 43 connector-manager + 3 doc tests

## Verification

```bash
cargo test
docker compose build --no-cache flux && docker compose up -d flux
```

Confirm UI shows "Live" with `FLUX_AUTH_ENABLED=true` without a token.
