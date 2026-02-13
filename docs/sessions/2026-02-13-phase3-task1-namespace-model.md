# Phase 3 Task 1: Namespace Model & Registry

**Date:** 2026-02-13
**Status:** Complete ✅
**Phase:** Phase 3 - Multi-tenancy and Authentication

---

## Objective

Implement the namespace model and in-memory registry - the foundation for multi-tenancy and authentication in public Flux instances.

---

## Reference

- ADR-003: Multi-tenancy and Authentication (Task 1, lines 228-240)
- ADR-000: Design discussion (namespace ownership model)
- src/state/engine.rs (DashMap pattern reference)

---

## Implementation

### Files Created

1. **src/namespace/mod.rs** - Core namespace module
   - `Namespace` struct (id, name, token, created_at, entity_count)
   - `NamespaceRegistry` with DashMap storage (lock-free concurrent access)
   - Registration logic with name validation and uniqueness
   - Token generation (UUID v4) and validation
   - Lookup methods (by name, by token)
   - Error types (RegistrationError, ValidationError, AuthError)

2. **src/namespace/tests.rs** - Comprehensive unit tests
   - 16 tests covering all functionality
   - Name validation (length, character rules)
   - Registration (success, duplicate detection)
   - Lookups (by name, by token)
   - Token validation (success, unauthorized, not found)
   - ID/token uniqueness verification

### Files Modified

1. **src/lib.rs** - Added `pub mod namespace;`

2. **Cargo.toml** - Added dependencies:
   - `uuid` v4 feature (token generation)
   - `rand` 0.8 (namespace ID generation)

---

## Design Decisions

### Namespace Struct

```rust
pub struct Namespace {
    pub id: String,              // ns_{random_8chars}
    pub name: String,            // user-chosen (matt, arc, etc.)
    pub token: String,           // UUID v4 bearer token
    pub created_at: DateTime<Utc>,
    pub entity_count: u64,       // Stats
}
```

### Registry Storage Pattern

Following StateEngine's DashMap pattern for lock-free concurrent access:

```rust
pub struct NamespaceRegistry {
    namespaces: Arc<DashMap<String, Namespace>>,  // Primary storage
    names: Arc<DashMap<String, String>>,          // Index: name -> id
    tokens: Arc<DashMap<String, String>>,         // Index: token -> id
}
```

**Why three indices:**
- `namespaces`: Primary storage (lookup by system ID)
- `names`: Fast uniqueness check + lookup by user-chosen name
- `tokens`: Fast auth validation (O(1) token -> namespace)

### Validation Rules

**Name validation** (from ADR-003):
- Length: 3-32 characters
- Characters: lowercase alphanumeric + dash/underscore `[a-z0-9-_]`
- Uniqueness: enforced at registration

**ID generation:**
- Format: `ns_{random_8chars}`
- Random alphanumeric suffix
- Example: `ns_7x9f2a`

**Token generation:**
- UUID v4 (random)
- Bearer token format
- No expiration (Phase 3 scope)

---

## API

### NamespaceRegistry Methods

```rust
// Create new registry
pub fn new() -> Self

// Register namespace (validates name, generates ID/token)
pub fn register(&self, name: &str) -> Result<Namespace, RegistrationError>

// Validate name format
pub fn validate_name(name: &str) -> Result<(), ValidationError>

// Look up by name
pub fn lookup_by_name(&self, name: &str) -> Option<Namespace>

// Look up by token
pub fn lookup_by_token(&self, token: &str) -> Option<Namespace>

// Validate token owns namespace (for write auth)
pub fn validate_token(&self, token: &str, namespace: &str) -> Result<(), AuthError>

// Get by ID (internal)
pub fn get(&self, namespace_id: &str) -> Option<Namespace>

// Get count
pub fn count(&self) -> usize
```

---

## Test Results

All tests pass (16 namespace tests + existing 51 tests):

```
test namespace::tests::test_count ... ok
test namespace::tests::test_lookup_by_name ... ok
test namespace::tests::test_lookup_by_token ... ok
test namespace::tests::test_multiple_namespaces_unique_ids ... ok
test namespace::tests::test_namespace_id_format ... ok
test namespace::tests::test_register_duplicate_name ... ok
test namespace::tests::test_register_invalid_name ... ok
test namespace::tests::test_register_success ... ok
test namespace::tests::test_validate_name_invalid_chars ... ok
test namespace::tests::test_validate_name_too_long ... ok
test namespace::tests::test_validate_name_too_short ... ok
test namespace::tests::test_validate_name_valid ... ok
test namespace::tests::test_validate_token_cross_namespace ... ok
test namespace::tests::test_validate_token_namespace_not_found ... ok
test namespace::tests::test_validate_token_success ... ok
test namespace::tests::test_validate_token_wrong_token ... ok

test result: ok. 67 passed; 0 failed; 0 ignored; 0 measured
```

---

## Test Coverage

### Name Validation
✅ Valid names (lowercase, alphanumeric, dash, underscore)
✅ Too short (< 3 chars)
✅ Too long (> 32 chars)
✅ Invalid characters (uppercase, special chars, spaces)

### Registration
✅ Success (generates ID, token, timestamps)
✅ Duplicate name rejection
✅ Invalid name rejection
✅ ID format verification (ns_{8chars})
✅ Token format verification (UUID v4)
✅ Unique IDs across registrations
✅ Unique tokens across registrations

### Lookups
✅ By name (found/not found)
✅ By token (found/not found)

### Token Validation
✅ Valid token for namespace
✅ Wrong token for namespace
✅ Namespace not found
✅ Cross-namespace token rejection

---

## Next Steps

Task 1 is complete. Foundation ready for:

**Task 2:** Token generation & extraction utilities (auth module)
**Task 3:** Entity ID parsing (namespace/entity_id format)
**Task 4:** Authorization middleware (write protection)
**Task 5:** Namespace registration API endpoints
**Task 6:** Entity query filtering by namespace
**Task 7:** Configuration & documentation

---

## Notes

- NO API integration yet (Task 5)
- In-memory only (persistence is Phase 4)
- No token expiration (Phase 4)
- Registry is thread-safe (DashMap)
- Pattern matches StateEngine for consistency

---

## Verification Commands

```bash
# Build
cargo build

# Run namespace tests
cargo test namespace

# Run all tests
cargo test
```
