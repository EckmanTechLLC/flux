# Phase 3 Task 5: Namespace Registration API

**Date:** 2026-02-13
**Status:** Complete ✅
**Reference:** ADR-003 Task 5 (lines 292-308)

---

## Objective

Expose namespace registration via HTTP API so users can register namespaces and obtain tokens for write authorization (Phase 3 multitenancy).

---

## Scope

1. Create namespace API module with two endpoints
2. POST /api/namespaces - Register new namespace (returns token)
3. GET /api/namespaces/:name - Lookup namespace (NO token in response)
4. Only available when `auth_enabled=true` (404 if disabled)
5. Error handling: 400 validation, 409 conflicts, 404 not found
6. Integration tests (8 test cases)
7. Wire into main.rs router

---

## Implementation

### Files Created

**`/src/api/namespace.rs`** (398 lines)
- `RegisterRequest` - POST request body
- `RegisterResponse` - Registration response (includes token)
- `NamespaceInfo` - Lookup response (NO token)
- `create_namespace_router()` - Router factory
- `register_namespace()` - POST /api/namespaces handler
- `lookup_namespace()` - GET /api/namespaces/:name handler
- `NamespaceError` - Error types with IntoResponse
- Test suite (8 tests, all passing)

**Key security feature:** Token only returned on registration, never exposed in lookup.

### Files Modified

**`/src/api/mod.rs`**
- Added `pub mod namespace`
- Exported `create_namespace_router`

**`/src/main.rs`**
- Imported `create_namespace_router`
- Created namespace router (reuses `AppState` from ingestion)
- Merged namespace_router into app

**`/Cargo.toml`**
- Added `tower = "0.5"` to dev-dependencies (for test utilities)

---

## API Specification

### POST /api/namespaces

Register a new namespace.

**Request:**
```json
{
  "name": "matt"
}
```

**Response (200 OK):**
```json
{
  "namespaceId": "ns_7x9f2a",
  "name": "matt",
  "token": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Errors:**
- `400 Bad Request` - Invalid name format (too short/long, invalid characters)
- `404 Not Found` - Auth disabled (namespace API not available)
- `409 Conflict` - Name already exists

**Validation rules:**
- Name: 3-32 characters
- Pattern: `[a-z0-9-_]`
- Unique across all namespaces

---

### GET /api/namespaces/:name

Look up namespace info (token NOT included).

**Response (200 OK):**
```json
{
  "namespaceId": "ns_7x9f2a",
  "name": "matt",
  "createdAt": "2026-02-13T10:30:00Z",
  "entityCount": 42
}
```

**Errors:**
- `404 Not Found` - Namespace not found OR auth disabled

**Security:** Token never exposed in lookup responses.

---

## Test Results

```
running 8 tests
test api::namespace::tests::test_register_namespace_auth_disabled ... ok
test api::namespace::tests::test_lookup_namespace_success ... ok
test api::namespace::tests::test_register_namespace_success ... ok
test api::namespace::tests::test_lookup_namespace_auth_disabled ... ok
test api::namespace::tests::test_token_not_exposed_in_lookup ... ok
test api::namespace::tests::test_lookup_namespace_not_found ... ok
test api::namespace::tests::test_register_namespace_duplicate ... ok
test api::namespace::tests::test_register_namespace_validation_errors ... ok

test result: ok. 8 passed; 0 failed
```

Full suite: 121 tests passed, 0 failed.

**Test coverage:**
1. ✅ Register namespace success (returns token, namespace_id, name)
2. ✅ Register with auth disabled (404 response)
3. ✅ Validation errors (too short, invalid characters)
4. ✅ Duplicate name conflict (409 response)
5. ✅ Lookup namespace success
6. ✅ Lookup non-existent namespace (404)
7. ✅ Lookup with auth disabled (404)
8. ✅ Token not exposed in lookup response

---

## Behavior

### When auth_enabled=false (internal mode)
- POST /api/namespaces → 404 "Namespace registration not available"
- GET /api/namespaces/:name → 404 "Namespace registration not available"
- Namespace API effectively disabled
- Backward compatible with Phase 1/2 behavior

### When auth_enabled=true (public mode)
- POST /api/namespaces → Register and return token
- GET /api/namespaces/:name → Return namespace info (NO token)
- Namespace registration required for event ingestion (enforced by Task 4)

---

## Integration

**Router chain in main.rs:**
```rust
let app = ingestion_router
    .merge(namespace_router)
    .merge(ws_router)
    .merge(query_router);
```

**Shared state:**
- Uses `AppState` from ingestion.rs
- Includes `namespace_registry: Arc<NamespaceRegistry>`
- Includes `auth_enabled: bool`
- Same registry instance used by auth middleware (Task 4)

---

## Example Usage

**Register namespace:**
```bash
curl -X POST http://localhost:3000/api/namespaces \
  -H "Content-Type: application/json" \
  -d '{"name": "matt"}'

# Response:
# {"namespaceId":"ns_7x9f2a","name":"matt","token":"550e8400-e29b-41d4-a716-446655440000"}
```

**Lookup namespace:**
```bash
curl http://localhost:3000/api/namespaces/matt

# Response:
# {"namespaceId":"ns_7x9f2a","name":"matt","createdAt":"2026-02-13T10:30:00Z","entityCount":0}
```

**Use token for event ingestion:**
```bash
curl -X POST http://localhost:3000/api/events \
  -H "Authorization: Bearer 550e8400-e29b-41d4-a716-446655440000" \
  -H "Content-Type: application/json" \
  -d '{...event with entity_id "matt/arc-01"...}'
```

---

## Security Notes

1. **Token exposure:** Token returned ONLY on registration, never in lookup
2. **Auth disabled:** Namespace endpoints return 404 (not exposed)
3. **Validation:** Strict name format prevents injection/abuse
4. **Conflict handling:** 409 response for duplicate names (clear feedback)
5. **Open reading:** GET endpoint has no auth (consistent with Phase 3 design)

---

## Dependencies

**Completed (Tasks 1-4):**
- ✅ Task 1: Namespace model & registry
- ✅ Task 2: Token generation & extraction
- ✅ Task 3: Entity ID parsing
- ✅ Task 4: Authorization middleware integrated

**This task (Task 5):** ✅ Complete
- Namespace registration API endpoints
- HTTP error handling
- Integration tests

**Next (Task 6):** Entity query filtering & discovery
- Add query params to GET /api/state/entities
- Filter by namespace/prefix

---

## Verification

- [x] Build succeeds with no warnings
- [x] All 8 namespace API tests pass
- [x] Full test suite passes (121 tests)
- [x] Router mounted in main.rs
- [x] Token only in registration response
- [x] 404 when auth disabled
- [x] Validation errors return 400
- [x] Duplicate names return 409

---

## Next Steps

1. Task 6: Entity query filtering (namespace=, prefix=)
2. Task 7: Configuration & documentation
3. Integration test: Full auth flow (register → publish → query)
4. Deploy test instance with auth_enabled=true

---

## Notes

- Reused `AppState` from ingestion.rs (DRY principle)
- Added `tower` to dev-dependencies for test utilities
- Tests create shared registry to verify conflict detection
- All tests async (tokio::test) to match handler signatures
- Error messages clear and user-friendly

**Implementation time:** ~60 minutes
**Lines added:** ~400 (namespace.rs + tests)
**Test coverage:** Complete (all scenarios tested)
