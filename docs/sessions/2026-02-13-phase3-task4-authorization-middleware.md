# Phase 3 Task 4: Authorization Middleware

**Date:** 2026-02-13
**Status:** Complete ✅
**ADR:** `/docs/decisions/003-multitenancy-and-authentication.md` (Task 4, lines 262-273)

---

## Objective

Implement authorization middleware for event ingestion endpoints. Protect write operations with token-based authorization while maintaining backward compatibility (auth disabled by default).

---

## Implementation

### 1. Created Authorization Middleware

**File:** `/src/api/auth_middleware.rs`

**Core function:** `authorize_event(headers, event, registry, auth_enabled)`

**Flow:**
1. If `auth_enabled=false`: Return Ok (backward compatible)
2. If `auth_enabled=true`:
   - Extract bearer token from Authorization header
   - Extract `entity_id` from `event.payload`
   - Parse namespace from entity_id (expects `namespace/entity` format)
   - Validate token owns namespace via NamespaceRegistry
3. Return appropriate errors:
   - `AuthError::InvalidToken` → 401 Unauthorized
   - `AuthError::InvalidEntityId` → 401 Unauthorized
   - `AuthError::NamespaceNotFound` → 401 Unauthorized
   - `AuthError::Forbidden` → 403 Forbidden

**Key decisions:**
- Auth is opt-in via config flag (default: false)
- In auth mode, entity_id MUST have namespace prefix
- Single token validates ownership, not just authentication

### 2. Updated AppState

**File:** `/src/api/ingestion.rs`

**Added fields to AppState:**
```rust
pub struct AppState {
    pub event_publisher: EventPublisher,
    pub namespace_registry: Arc<NamespaceRegistry>,  // New
    pub auth_enabled: bool,                           // New
}
```

**Updated handlers:**
- `publish_event`: Added HeaderMap parameter, calls authorize_event
- `publish_batch`: Added HeaderMap parameter, validates each event in batch

**Error handling:**
- Added `AppError::Unauthorized` (401)
- Added `AppError::Forbidden` (403)
- Implemented `From<AuthError>` for AppError conversion

### 3. Updated Main Initialization

**File:** `/src/main.rs`

**Added:**
- Environment variable: `FLUX_AUTH_ENABLED` (default: false)
- NamespaceRegistry initialization
- Pass registry and auth_enabled to AppState

```rust
let auth_enabled = std::env::var("FLUX_AUTH_ENABLED")
    .unwrap_or_else(|_| "false".to_string())
    .parse::<bool>()
    .unwrap_or(false);

let namespace_registry = Arc::new(NamespaceRegistry::new());
```

### 4. Updated Module Exports

**File:** `/src/api/mod.rs`

- Exported `auth_middleware` module

---

## Testing

### Unit Tests (11 tests, all passing)

**File:** `/src/api/auth_middleware/tests.rs`

**Coverage:**
- ✅ Auth disabled allows any entity (with/without namespace)
- ✅ Auth enabled rejects missing token
- ✅ Auth enabled rejects missing entity_id
- ✅ Auth enabled rejects entity_id without namespace prefix
- ✅ Auth enabled rejects unregistered namespace
- ✅ Auth enabled rejects wrong token
- ✅ Auth enabled accepts valid token
- ✅ Auth enabled rejects cross-namespace access
- ✅ Auth enabled rejects invalid namespace format
- ✅ Auth enabled rejects multiple slashes in entity_id

**Test results:**
```
test api::auth_middleware::tests::test_auth_disabled_allows_all ... ok
test api::auth_middleware::tests::test_auth_disabled_allows_namespaced_entity ... ok
test api::auth_middleware::tests::test_auth_enabled_missing_entity_id ... ok
test api::auth_middleware::tests::test_auth_enabled_invalid_namespace_format ... ok
test api::auth_middleware::tests::test_auth_enabled_different_namespace ... ok
test api::auth_middleware::tests::test_auth_enabled_missing_token ... ok
test api::auth_middleware::tests::test_auth_enabled_multiple_slashes ... ok
test api::auth_middleware::tests::test_auth_enabled_missing_namespace_prefix ... ok
test api::auth_middleware::tests::test_auth_enabled_namespace_not_found ... ok
test api::auth_middleware::tests::test_auth_enabled_wrong_token ... ok
test api::auth_middleware::tests::test_auth_enabled_valid_token ... ok

test result: ok. 11 passed; 0 failed
```

### Full Test Suite

**Result:** All 113 tests passing
- No regressions introduced
- Backward compatibility maintained

### Build Verification

**Command:** `cargo build`
**Result:** Success, no warnings

---

## Files Modified

1. `/src/api/auth_middleware.rs` - Created (authorization logic)
2. `/src/api/auth_middleware/tests.rs` - Created (11 unit tests)
3. `/src/api/ingestion.rs` - Updated AppState, handlers, error types
4. `/src/api/mod.rs` - Export auth_middleware module
5. `/src/main.rs` - Initialize auth config and namespace registry

---

## Verification

### Auth Disabled (Default)

**Expected:** All events accepted, no Authorization header required

```bash
# Start Flux (auth disabled by default)
cargo run

# Publish event without token (should succeed)
curl -X POST http://localhost:3000/api/events \
  -H "Content-Type: application/json" \
  -d '{
    "stream": "test",
    "source": "test",
    "timestamp": 1234567890,
    "payload": {
      "entity_id": "sensor-01",
      "properties": {"temp": 20.5}
    }
  }'
```

### Auth Enabled

**Expected:** Events require valid token for namespace

```bash
# Start Flux with auth enabled
FLUX_AUTH_ENABLED=true cargo run

# Register namespace (Task 5 - not yet implemented)
# For now, namespaces must be registered programmatically

# Publish event without token (should fail: 401)
curl -X POST http://localhost:3000/api/events \
  -H "Content-Type: application/json" \
  -d '{
    "stream": "test",
    "source": "test",
    "timestamp": 1234567890,
    "payload": {
      "entity_id": "matt/sensor-01",
      "properties": {"temp": 20.5}
    }
  }'

# Publish event with valid token (should succeed: 200)
curl -X POST http://localhost:3000/api/events \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <valid-token>" \
  -d '{
    "stream": "test",
    "source": "test",
    "timestamp": 1234567890,
    "payload": {
      "entity_id": "matt/sensor-01",
      "properties": {"temp": 20.5}
    }
  }'
```

---

## Key Behaviors

### Backward Compatibility ✅

- Default: `auth_enabled=false`
- No Authorization header required
- Entity IDs can be any format (no namespace prefix required)
- Existing deployments continue working unchanged

### Auth Mode Requirements

**When `FLUX_AUTH_ENABLED=true`:**

1. **Authorization header required:**
   - Format: `Authorization: Bearer <token>`
   - Missing/invalid → 401 Unauthorized

2. **Entity ID must have namespace prefix:**
   - Format: `namespace/entity` (e.g., `matt/sensor-01`)
   - Simple entity IDs (no namespace) → 401 Unauthorized

3. **Namespace must be registered:**
   - Namespace not found → 401 Unauthorized

4. **Token must own namespace:**
   - Wrong token → 403 Forbidden

### Error Responses

**401 Unauthorized:**
- Missing Authorization header
- Invalid token format
- Missing entity_id in payload
- Entity_id without namespace prefix (in auth mode)
- Namespace not registered

**403 Forbidden:**
- Token doesn't own the namespace (cross-namespace access)

---

## Integration with Existing Components

### Dependencies (Tasks 1-3)

✅ **Task 1:** NamespaceRegistry with `validate_token(token, namespace)` method
✅ **Task 2:** `extract_bearer_token(headers)` from auth module
✅ **Task 3:** `parse_entity_id(id)` from entity module

### Used By

**Event Ingestion:**
- POST /api/events (single event)
- POST /api/events/batch (multiple events)

**Not Yet Protected:**
- WebSocket subscriptions (Task 5)
- State query endpoints (intentionally open for reading)

---

## Next Steps

### Task 5: Namespace Registration API

**Goal:** Add HTTP endpoints for namespace registration

**Endpoints:**
- POST /api/namespaces - Register namespace
- GET /api/namespaces/:name - Get namespace info

**Integration:** Once Task 5 is complete, users can register namespaces via API and obtain tokens for event publishing.

### Task 6: Entity Query Filtering

**Goal:** Add query parameters to filter entities by namespace

**Enhancements:**
- `GET /api/state/entities?namespace=matt`
- `GET /api/state/entities?prefix=matt/sensor`

---

## Notes

### Design Decisions

1. **Authorization at ingestion layer only:**
   - Write operations protected
   - Read operations remain open (by design)
   - "World is open for observation" principle

2. **Batch handling:**
   - Each event in batch validated independently
   - One failed auth doesn't block entire batch
   - Individual errors reported in batch response

3. **Error mapping:**
   - Most auth errors → 401 (authentication problem)
   - Cross-namespace access → 403 (authorization problem)
   - Distinction helps debugging token vs. permission issues

4. **No changes to WebSocket yet:**
   - Task 4 scope: HTTP event ingestion only
   - WebSocket subscriptions remain unauthenticated
   - Task 5 may add optional auth for WebSocket

### Backward Compatibility Guarantee

**Internal deployments (auth_enabled=false):**
- No configuration changes required
- No Authorization headers needed
- Entity IDs remain simple strings
- Zero impact on existing code

**Migration path:**
- Opt-in via environment variable
- Can toggle per deployment
- No breaking changes to API

---

## Summary

Task 4 complete: Authorization middleware implemented and tested.

**Deliverables:**
- ✅ Authorization middleware with conditional auth
- ✅ Updated AppState with auth_enabled and namespace_registry
- ✅ Protected event ingestion endpoints
- ✅ Comprehensive unit tests (11 tests)
- ✅ Backward compatibility maintained (auth disabled by default)
- ✅ All existing tests passing (113 total)

**Ready for:** Task 5 (Namespace Registration API)
