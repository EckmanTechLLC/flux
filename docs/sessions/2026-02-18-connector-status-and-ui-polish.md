# Session: Connector Status Fix + UI Polish

**Date:** 2026-02-18
**Status:** Complete

## Changes

### 1. `src/api/connectors.rs` — auth-disabled connector status fix

`list_connectors()` was returning all connectors as `not_configured` when auth is disabled.

**Fix:** Use "default" namespace for credential lookup (same pattern as auth-enabled branch).

```rust
// Before: always not_configured
// After: check credential_store with "default" namespace
let configured = credential_store.list_by_user("default")...
```

### 2. `ui/public/index.html` — entity sort order

`renderEntities()` sorted entities alphabetically. Changed to sort by `lastUpdated` descending so most recently active entities appear first.

```js
// Before
groups[type].sort((a, b) => a.id.localeCompare(b.id))
// After
groups[type].sort((a, b) => b.lastUpdated - a.lastUpdated)
```

### 3. `ui/public/index.html` — font size reduction

Reduced entity font sizes by 1px each to fit more entities on screen:

| Selector | Before | After |
|---|---|---|
| `.entity-name` | 13px | 12px |
| `.entity-status` | 10px | 9px |
| `.entity-time` | 10px | 9px |
| `.prop-row` | 11px | 10px |
| `.prop-val` | 11px | 10px |

Colors and layout unchanged.

## Tests

`cargo test --lib`: 164 passed, 8 pre-existing failures (NATS connection refused — not running locally).
Connector tests: 5/5 pass.

## Files Modified

- `src/api/connectors.rs`
- `ui/public/index.html`

## Deployment

`docker compose up -d --build flux flux-ui` — both containers restarted.
