# Session: Phase 1 Task 2 - Event Model & Validation

**Date:** 2026-02-11
**Task:** Implement FluxEvent model with validation and UUIDv7 generation
**Status:** ✅ Complete

---

## Objective

Create the core event model for Flux with envelope validation, UUIDv7 generation, and comprehensive unit tests.

## Implementation

### 1. Module Structure

Converted `src/event.rs` → `src/event/` module:
- `src/event/mod.rs` - FluxEvent struct and public API
- `src/event/validation.rs` - Validation logic and error types
- `src/event/tests.rs` - Comprehensive unit tests

### 2. FluxEvent Struct

Implemented as specified in ADR-001:

```rust
pub struct FluxEvent {
    pub event_id: String,      // UUIDv7 (auto-generated)
    pub stream: String,         // Required, validated format
    pub source: String,         // Required
    pub timestamp: i64,         // Required, Unix epoch ms
    pub key: Option<String>,    // Optional
    pub schema: Option<String>, // Optional
    pub payload: Value,         // Required, must be JSON object
}
```

**Serde attributes:**
- `event_id` serialized as `"eventId"` (camelCase for external API)
- Optional fields skip serialization if `None`

### 3. Validation Rules

Implemented in `validation.rs`:

**Required Fields:**
- `stream` - must not be empty
- `source` - must not be empty
- `timestamp` - must be positive (> 0)
- `payload` - must be valid JSON object (not array, string, null, etc.)

**Stream Format Validation:**
- Lowercase letters (a-z)
- Numbers (0-9)
- Dots (.) for hierarchy
- No leading/trailing dots
- No consecutive dots
- Examples: `"sensors"`, `"sensors.temperature"`, `"data.zone1.temp"`

**UUIDv7 Generation:**
- Auto-generate if `event_id` is empty
- Preserve existing `event_id` if provided
- Uses `uuid::Uuid::now_v7()` for time-ordered IDs

**Error Types:**
- `ValidationError::MissingStream`
- `ValidationError::MissingSource`
- `ValidationError::MissingPayload`
- `ValidationError::InvalidStreamFormat(String)`
- `ValidationError::InvalidTimestamp(i64)`
- `ValidationError::PayloadNotObject`

### 4. Public API

```rust
impl FluxEvent {
    pub fn validate_and_prepare(&mut self) -> Result<(), ValidationError>;
}

pub use validation::{validate_and_prepare, ValidationError};
```

Method mutates event to generate `event_id` if missing.

### 5. Tests

Implemented 16 unit tests covering:

**Validation Tests:**
- ✅ Valid event passes validation
- ✅ Missing stream fails
- ✅ Missing source fails
- ✅ Invalid stream format fails (uppercase, underscores, etc.)
- ✅ Invalid timestamp fails (negative, zero)
- ✅ Payload not object fails (string, array, null)

**UUIDv7 Tests:**
- ✅ UUIDv7 generation works
- ✅ Generated IDs are unique
- ✅ Generated IDs are valid UUID format
- ✅ Existing event_id preserved

**Serde Tests:**
- ✅ Serialization/deserialization works
- ✅ Optional None fields skipped in JSON
- ✅ eventId camelCase mapping works

**Stream Format Tests:**
- ✅ Valid stream names pass
- ✅ Invalid stream names fail

### 6. Test Results

```
cargo test --lib event
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored
```

All tests pass with no warnings.

---

## Files Created/Modified

**Created:**
- `src/event/mod.rs` - FluxEvent struct and module exports
- `src/event/validation.rs` - Validation logic and error types
- `src/event/tests.rs` - 16 unit tests

**Deleted:**
- `src/event.rs` - Replaced with module directory

**Modified:**
- None (lib.rs already had `pub mod event;`)

---

## Key Decisions

1. **Module structure:** Used directory module (`src/event/mod.rs`) instead of single file to separate concerns (struct, validation, tests)

2. **Validation approach:** Mutable `validate_and_prepare()` method that both validates and generates UUIDv7, avoiding need for separate validation/preparation steps

3. **Stream validation:** Strict format (lowercase, dots only) to enforce consistency and prevent naming chaos

4. **Payload validation:** Enforce JSON object (not array/string/etc.) to ensure consistent event structure for state derivation

5. **Error handling:** Custom `ValidationError` enum with `Display` and `Error` traits for clear error messages

---

## Next Steps

Task 3: State Engine Core
- In-memory state storage (DashMap)
- Generic entity/property model
- Event application logic
- State change detection

---

## Notes

- All dependencies (uuid v7, serde, serde_json) were already in Cargo.toml from Task 1
- No external crates added
- Validation is domain-agnostic (no schema validation)
- UUIDv7 provides time-ordered IDs for natural event ordering
- Tests cover both success and failure paths
