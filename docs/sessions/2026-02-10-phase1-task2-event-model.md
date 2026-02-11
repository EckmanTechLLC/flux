# Session: Phase 1 Task 2 - Event Model Implementation

**Date:** 2026-02-10
**Session:** 2026-02-10-phase1-task2-event-model
**Status:** ✅ Complete

---

## Objective

Implement Flux event envelope model and validation in Go.

---

## Scope

1. Define Go structs for Flux event envelope (eventId, stream, source, timestamp, key, schema, payload)
2. Implement UUIDv7 generation (using google/uuid library)
3. Implement event validation:
   - Required fields: stream, source, timestamp, payload
   - Optional fields: eventId (generate if missing), key, schema
   - Validate timestamp is Unix epoch milliseconds
   - Validate stream naming convention (lowercase, dots)
4. Write unit tests for validation (valid events, missing required fields, invalid formats)
5. No API endpoints yet - just core event model

---

## Implementation

### Event Structure

Implemented standardized Flux event envelope per ADR-001:

**Required Fields:**
- `eventId` (string, UUIDv7) - Generated if not provided
- `stream` (string) - Lowercase with dots (e.g., "alarms.events")
- `source` (string) - Producer identity
- `timestamp` (int64) - Unix epoch milliseconds
- `payload` (json.RawMessage) - Opaque JSON object

**Optional Fields:**
- `key` (string) - Ordering/partition key for consumer-side grouping
- `schema` (string) - Schema name + version (metadata only)

### Validation Rules

**Stream naming convention:**
- Lowercase alphanumeric with dots
- Hierarchical structure (e.g., "sensor.hvac.temperature")
- Regex: `^[a-z0-9]+(\.[a-z0-9]+)*$`

**Timestamp:**
- Must be positive (> 0)
- Unix epoch milliseconds

**Payload:**
- Must be valid JSON object
- Opaque to Flux (no schema validation)

**EventID:**
- UUIDv7 format if provided
- Auto-generated using UUIDv7 if missing

### Files Created

**`/flux-service/internal/model/event.go`**
- Event struct with JSON tags
- UUIDv7 generation function
- Timestamp helper methods (SetTimestampNow, GetTimestamp)

**`/flux-service/internal/model/validator.go`**
- Validate() method - checks event conformance
- ValidateAndPrepare() method - validates and generates missing fields
- ValidationError type with structured error messages
- Stream name pattern validation (regex)

**`/flux-service/internal/model/event_test.go`**
- 12 test functions covering:
  - UUIDv7 generation
  - Timestamp methods
  - Valid event validation
  - Missing required fields detection
  - Invalid format detection
  - ValidateAndPrepare behavior
  - JSON serialization/deserialization
  - Optional field omission (omitempty)

### Files Modified

**`/flux-service/go.mod`**
- Added dependency: `github.com/google/uuid v1.6.0`

**`/CLAUDE.md`**
- Marked "Event model implementation" as complete

---

## Technical Details

### UUIDv7

Using `github.com/google/uuid` library with NewV7() for time-ordered event IDs:
```go
func GenerateEventID() string {
	return uuid.Must(uuid.NewV7()).String()
}
```

**Benefits:**
- Time-ordered for efficient indexing
- Globally unique across all streams
- Sortable by creation time

### Validation Flow

**Two validation modes:**

1. **Validate()** - Check conformance only
   - Verifies all required fields present
   - Validates formats
   - Returns ValidationError on failure

2. **ValidateAndPrepare()** - Generate + validate
   - Generates eventId if missing
   - Calls Validate()
   - Primary function for publish flow

### Error Handling

Structured validation errors:
```go
type ValidationError struct {
	Field   string
	Message string
}
```

Error messages include field name and specific issue.

---

## Testing

### Test Coverage

**12 test functions:**
1. TestGenerateEventID - UUIDv7 generation
2. TestEvent_SetTimestampNow - Timestamp generation
3. TestEvent_GetTimestamp - Timestamp parsing
4. TestValidate_ValidEvent - Valid event variants (3 subtests)
5. TestValidate_MissingRequiredFields - Missing fields (4 subtests)
6. TestValidate_InvalidFormats - Format violations (7 subtests)
7. TestValidateAndPrepare_GeneratesEventID - Auto-generation
8. TestValidateAndPrepare_PreservesExistingEventID - ID preservation
9. TestValidateAndPrepare_FailsOnInvalidEvent - Validation enforcement
10. TestValidationError_Error - Error message formatting
11. TestEvent_JSONSerialization - Marshal/unmarshal round-trip
12. TestEvent_OmitsEmptyOptionalFields - JSON omitempty behavior

**Test Results:**
```
=== RUN   TestGenerateEventID
--- PASS: TestGenerateEventID (0.00s)
=== RUN   TestEvent_SetTimestampNow
--- PASS: TestEvent_SetTimestampNow (0.00s)
=== RUN   TestEvent_GetTimestamp
--- PASS: TestEvent_GetTimestamp (0.00s)
=== RUN   TestValidate_ValidEvent
--- PASS: TestValidate_ValidEvent (0.00s)
=== RUN   TestValidate_MissingRequiredFields
--- PASS: TestValidate_MissingRequiredFields (0.00s)
=== RUN   TestValidate_InvalidFormats
--- PASS: TestValidate_InvalidFormats (0.00s)
=== RUN   TestValidateAndPrepare_GeneratesEventID
--- PASS: TestValidateAndPrepare_GeneratesEventID (0.00s)
=== RUN   TestValidateAndPrepare_PreservesExistingEventID
--- PASS: TestValidateAndPrepare_PreservesExistingEventID (0.00s)
=== RUN   TestValidateAndPrepare_FailsOnInvalidEvent
--- PASS: TestValidateAndPrepare_FailsOnInvalidEvent (0.00s)
=== RUN   TestValidationError_Error
--- PASS: TestValidationError_Error (0.00s)
=== RUN   TestEvent_JSONSerialization
--- PASS: TestEvent_JSONSerialization (0.00s)
=== RUN   TestEvent_OmitsEmptyOptionalFields
--- PASS: TestEvent_OmitsEmptyOptionalFields (0.00s)
PASS
ok  	github.com/flux/flux-service/internal/model	0.016s
```

**All tests passed ✅**

### Test Commands

**Run tests in Docker (since Go not installed on host):**
```bash
cd /home/etl/projects/flux/flux-service
docker run --rm -v "$(pwd):/workspace" -w /workspace golang:1.22-alpine sh -c "go mod tidy && go test -v ./internal/model/..."
```

**Alternative (if Go installed locally):**
```bash
cd /home/etl/projects/flux/flux-service
go test -v ./internal/model/...
```

---

## Verification Checklist

- [x] Event struct defined with all envelope fields
- [x] UUIDv7 generation implemented
- [x] Stream naming validation (lowercase, dots)
- [x] Required field validation (stream, source, timestamp, payload)
- [x] Optional field handling (eventId, key, schema)
- [x] Timestamp validation (positive Unix epoch milliseconds)
- [x] Payload validation (valid JSON object)
- [x] EventID format validation (UUID)
- [x] Unit tests for valid events
- [x] Unit tests for missing required fields
- [x] Unit tests for invalid formats
- [x] JSON serialization tests
- [x] go.mod updated with google/uuid
- [x] All tests passing
- [x] CLAUDE.md updated

---

## Next Steps

**Phase 1 Task 3:** Publish API Implementation
- gRPC or HTTP endpoint for event publishing
- Integrate event validation (ValidateAndPrepare)
- Publish validated events to NATS JetStream
- Return confirmation (eventId, sequence)

**Dependencies:** Event model (completed)

**Alternative Path:** Stream management API before publish API
- Create/configure NATS streams with Flux conventions
- Stream existence checks
- Retention policy setup

---

## Issues Encountered

None. Implementation completed without issues.

---

## Notes

- Used `json.RawMessage` for payload to keep it opaque
- Regex pattern enforces stream naming convention strictly
- UUIDv7 provides time-ordering for efficient replay
- Validation errors include field name for clear debugging
- Tests cover both happy path and error cases
- Optional fields use `omitempty` JSON tag to exclude when empty
- Timestamp helpers (SetTimestampNow, GetTimestamp) for convenience

---

## Files Modified Summary

**Created:**
- `/flux-service/internal/model/event.go` (57 lines)
- `/flux-service/internal/model/validator.go` (86 lines)
- `/flux-service/internal/model/event_test.go` (393 lines)
- `/docs/sessions/2026-02-10-phase1-task2-event-model.md` (this file)

**Modified:**
- `/flux-service/go.mod` (added google/uuid dependency)
- `/flux-service/go.sum` (updated with dependency checksums)
- `/CLAUDE.md` (status update)

---

**Session completed successfully.**
