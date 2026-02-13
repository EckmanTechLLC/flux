# Phase 3 Task 3: Entity ID Parsing

**Date:** 2026-02-13
**Status:** ✅ Complete
**Session:** phase3-task3-entity-id-parsing

## Objective

Implement entity ID parsing utilities to extract namespace prefixes from entity IDs, supporting both public ("namespace/entity") and internal ("entity") formats.

## Implementation

### Files Created

**`/src/entity/mod.rs`** (107 lines)
- `ParsedEntityId` struct with optional namespace and entity fields
- `ParseError` enum for validation errors
- `parse_entity_id()` - splits entity ID into namespace and entity parts
- `extract_namespace()` - convenience function to get just the namespace
- Validates namespace part using rules from `NamespaceRegistry::validate_name()`

**`/src/entity/tests.rs`** (149 lines)
- 15 comprehensive unit tests covering:
  - Valid formats with/without namespace
  - Empty input handling
  - Invalid formats (multiple slashes, empty parts)
  - Namespace validation (length, character rules)
  - Edge cases (min/max length, special chars in entity)

### Files Modified

**`/src/lib.rs`**
- Added `pub mod entity;` declaration

## API Design

### Entity ID Formats

**Public mode:** `"namespace/entity"`
- Example: `"matt/sensor-01"` → namespace="matt", entity="sensor-01"
- Namespace must follow validation rules: [a-z0-9-_], 3-32 chars

**Internal mode:** `"entity"`
- Example: `"sensor-01"` → namespace=None, entity="sensor-01"
- No namespace prefix

### Functions

```rust
pub fn parse_entity_id(entity_id: &str) -> Result<ParsedEntityId, ParseError>
```
- Parses entity ID into structured format
- Validates namespace if present
- Returns error for invalid formats

```rust
pub fn extract_namespace(entity_id: &str) -> Option<String>
```
- Convenience function to extract just namespace
- Returns None if no namespace or parse fails

### Validation Rules

Namespace part (when present) must satisfy:
- Length: 3-32 characters
- Characters: lowercase alphanumeric + dash/underscore `[a-z0-9-_]`
- No uppercase, spaces, or special characters

Entity part:
- No restrictions (any non-empty string)
- Cannot contain "/" (used as delimiter)

## Testing

```bash
# Entity module tests
$ cargo test entity:: --lib
running 15 tests
test entity::tests::test_parse_entity_id_with_namespace ... ok
test entity::tests::test_parse_entity_id_without_namespace ... ok
test entity::tests::test_parse_entity_id_empty ... ok
test entity::tests::test_parse_entity_id_empty_namespace ... ok
test entity::tests::test_parse_entity_id_empty_entity ... ok
test entity::tests::test_parse_entity_id_multiple_slashes ... ok
test entity::tests::test_parse_entity_id_invalid_namespace_too_short ... ok
test entity::tests::test_parse_entity_id_invalid_namespace_too_long ... ok
test entity::tests::test_parse_entity_id_invalid_namespace_uppercase ... ok
test entity::tests::test_parse_entity_id_invalid_namespace_special_chars ... ok
test entity::tests::test_parse_entity_id_valid_namespace_chars ... ok
test entity::tests::test_extract_namespace_with_namespace ... ok
test entity::tests::test_extract_namespace_without_namespace ... ok
test entity::tests::test_extract_namespace_invalid ... ok
test entity::tests::test_parse_entity_id_edge_cases ... ok

test result: ok. 15 passed; 0 failed; 0 ignored

# All library tests still pass
$ cargo test --lib
test result: ok. 102 passed; 0 failed; 0 ignored
```

## Design Decisions

### Namespace Validation Reuse
Reused `NamespaceRegistry::validate_name()` for consistency with namespace registration rules.

### Error Types
- `ParseError::Empty` - Empty entity ID
- `ParseError::InvalidFormat` - Structural issues (multiple slashes, empty parts)
- `ParseError::InvalidNamespace` - Namespace doesn't match validation rules

### Entity Part Flexibility
No restrictions on entity part format (except non-empty). This preserves flexibility for various ID schemes.

### Delimiter Choice
Used "/" as delimiter:
- Clear visual separation
- Common in REST paths
- Similar to namespacing conventions

## Test Coverage

**Valid inputs:**
- With namespace: "matt/sensor-01"
- Without namespace: "sensor-01"
- Valid chars: a-z, 0-9, dash, underscore
- Edge cases: min/max namespace length

**Invalid inputs:**
- Empty string
- Empty namespace: "/sensor"
- Empty entity: "matt/"
- Multiple slashes: "a/b/c"
- Invalid namespace: too short, too long, uppercase, special chars

**Extract namespace:**
- Returns Some(namespace) when present
- Returns None when absent or invalid

## Integration Notes

**Not yet integrated:**
- API middleware (Task 4)
- Event ingestion validation (Task 4)
- Authorization checks (Task 4)

These utilities provide the foundation for namespace-aware entity ID handling in the API layer.

## Files Summary

Created:
- `/src/entity/mod.rs` - Entity ID parsing utilities
- `/src/entity/tests.rs` - Unit tests

Modified:
- `/src/lib.rs` - Added entity module declaration

Lines of code:
- Implementation: 107 lines
- Tests: 149 lines
- Total: 256 lines

Build: ✅ Success
Tests: ✅ 102 passed (15 new entity tests)
