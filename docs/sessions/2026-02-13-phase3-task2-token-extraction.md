# Phase 3 Task 2: Token Generation & Extraction

**Date:** 2026-02-13
**Task:** Implement token extraction utilities for HTTP and WebSocket authentication
**Status:** ✅ Complete

---

## Overview

Implemented token extraction utilities for Phase 3 multitenancy authentication. Provides functions to extract bearer tokens from HTTP Authorization headers and WebSocket JSON messages.

**Reference:** ADR-003 lines 241-250

---

## Implementation Summary

### Files Created

**`/src/auth/mod.rs`** (90 lines)
- `extract_bearer_token(headers: &HeaderMap) -> Result<String, TokenError>`
  - Parses "Authorization: Bearer <token>" format
  - Case-insensitive "Bearer" prefix
  - Trims whitespace from token
  - Returns token string or TokenError

- `extract_token_from_message(message: &Value) -> Result<String, TokenError>`
  - Extracts "token" field from JSON message
  - Validates field is string type
  - Checks for empty tokens
  - Returns token string or TokenError

- `TokenError` enum:
  - `Missing` - No Authorization header or token field
  - `InvalidFormat` - Wrong format or non-string token
  - `Empty` - Token is empty string

- Internal helper:
  - `parse_bearer_token(header_value: &str)` - Parses "Bearer <token>"

**`/src/auth/tests.rs`** (248 lines)
- 20 comprehensive unit tests covering:
  - Valid bearer tokens (standard, whitespace, case variations)
  - Missing/malformed Authorization headers
  - Invalid auth schemes (Basic, etc.)
  - Empty tokens
  - Valid WebSocket message tokens
  - Missing token fields
  - Non-string/null tokens
  - Empty messages
  - Error message formatting

### Files Modified

**`/src/lib.rs`** (3 lines)
- Added `pub mod auth;` after namespace module

---

## Test Results

**Build:** ✅ Success
```
cargo build
Finished `dev` profile in 9.42s
```

**Auth Tests:** ✅ 20/20 passed
```
cargo test auth::
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured
```

**Full Suite:** ✅ 87/87 passed (20 new + 67 existing)
```
cargo test --quiet
test result: ok. 87 passed; 0 failed; 0 ignored; 0 measured
```

---

## Token Formats

### HTTP Authorization Header
```
Authorization: Bearer 550e8400-e29b-41d4-a716-446655440000
```

**Extraction:**
```rust
use axum::http::HeaderMap;
use flux::auth::extract_bearer_token;

let token = extract_bearer_token(&headers)?;
// token = "550e8400-e29b-41d4-a716-446655440000"
```

### WebSocket JSON Message
```json
{
  "type": "subscribe",
  "token": "550e8400-e29b-41d4-a716-446655440000",
  "entity_id": "matt/sensor_42"
}
```

**Extraction:**
```rust
use serde_json::Value;
use flux::auth::extract_token_from_message;

let message: Value = serde_json::from_str(json_str)?;
let token = extract_token_from_message(&message)?;
// token = "550e8400-e29b-41d4-a716-446655440000"
```

---

## Error Handling

All extraction functions return `Result<String, TokenError>`:

| Error | Cause | HTTP Status (future) |
|-------|-------|---------------------|
| `TokenError::Missing` | No Authorization header or token field | 401 Unauthorized |
| `TokenError::InvalidFormat` | Wrong format or non-string token | 400 Bad Request |
| `TokenError::Empty` | Token is empty string | 400 Bad Request |

**Error Display:**
- `Missing`: "Authorization token not provided"
- `InvalidFormat`: "Invalid authorization token format"
- `Empty`: "Authorization token is empty"

---

## Design Decisions

**Case-insensitive Bearer:**
- Accepts "Bearer", "bearer", "BEARER" etc.
- Follows HTTP header case-insensitivity convention

**Whitespace handling:**
- HTTP: Trims whitespace from extracted token
- WebSocket: Preserves token as-is (no trimming)
- Rationale: HTTP headers may have formatting variations, WebSocket JSON is controlled by client

**No token validation:**
- Extraction only - no UUID format validation
- Token validation (namespace ownership) is separate concern
- Allows flexibility for future token formats (JWT, etc.)

**Generic JSON value:**
- Uses `serde_json::Value` for WebSocket messages
- Not coupled to specific message types (subscribe, unsubscribe)
- Works with any JSON containing "token" field

---

## Integration Notes

**NOT IMPLEMENTED (future tasks):**
- ❌ API integration (Task 4)
- ❌ Token validation against namespace registry (Task 4)
- ❌ Conditional auth based on config (Task 7)
- ❌ Error response mapping (Task 4)

**Next Steps (Task 3):**
- Implement entity ID parsing (`namespace/entity_id`)
- Extract namespace from entity IDs
- Validation helpers for namespaced IDs

**Next Steps (Task 4):**
- Authorization middleware for ingestion endpoints
- Integrate `extract_bearer_token()` into POST /api/events
- Call `namespace_registry.validate_token()`
- Return 401/403 error responses

---

## Code Quality

**No issues:**
- ✅ No TODOs in code
- ✅ No commented-out code
- ✅ Clear function names
- ✅ Comprehensive tests (edge cases covered)
- ✅ Error types implement Display and Error traits
- ✅ Follows existing Flux patterns (Result types, module structure)

**Test coverage:**
- HTTP extraction: 9 test cases
- WebSocket extraction: 9 test cases
- Error display: 3 test cases
- Total: 21 assertions across 20 tests

---

## Dependencies

**No new dependencies added** - uses existing:
- `axum::http::HeaderMap` (already in Cargo.toml)
- `serde_json::Value` (already in Cargo.toml)

---

## Session Checklist

- [x] Read CLAUDE.md, ADR-003, existing code
- [x] Verified assumptions (API patterns, message formats)
- [x] Created auth module with extraction functions
- [x] Created comprehensive test suite
- [x] Updated lib.rs to expose auth module
- [x] Built successfully (no warnings)
- [x] All tests pass (20/20 auth, 87/87 total)
- [x] Documented implementation (this file)

**Time:** ~20 minutes (read + implement + test + document)

---

## Summary

Phase 3 Task 2 complete. Token extraction utilities implemented with 20 passing tests. Ready for Task 3 (entity ID parsing) and Task 4 (authorization middleware integration).

**No blockers. No scope expansion.**
