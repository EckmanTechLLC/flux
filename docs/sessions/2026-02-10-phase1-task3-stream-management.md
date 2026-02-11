# Session: Phase 1 Task 3 - Stream Management

**Date:** 2026-02-10
**Session:** 2026-02-10-phase1-task3-stream-management
**Status:** ✅ Complete

---

## Objective

Implement NATS JetStream stream creation and management for Flux.

---

## Scope

1. Define stream configuration struct (retention, limits, storage from ADR-001)
2. Implement CreateStream function:
   - Connect to NATS JetStream
   - Create stream with Flux conventions (subjects matching stream names)
   - Default retention: 7 days, 10GB, 10M messages, file storage
   - Handle stream already exists (idempotent)
3. Implement stream initialization on startup:
   - Create/verify streams exist with correct config
   - Log stream creation/verification
4. Write tests:
   - Stream creation with default config
   - Stream creation with custom retention
   - Idempotent (calling twice doesn't error)
5. Update main.go to initialize streams on startup

---

## Implementation

### Stream Manager Architecture

**Key Design Decision:** NATS stream names cannot contain dots, but Flux stream names use dot notation for hierarchy. Solution:
- Flux stream name: `alarms.events` (user-facing, hierarchical)
- NATS stream name: `ALARMS_EVENTS` (uppercase with underscores)
- NATS subjects: `alarms.events` (hierarchical, used for routing)

**Conversion Function:**
```go
func toNATSStreamName(fluxName string) string {
    return strings.ToUpper(strings.ReplaceAll(fluxName, ".", "_"))
}
```

### StreamConfig Structure

Default retention policies per ADR-001:
- **MaxAge:** 7 days (168 hours)
- **MaxBytes:** 10GB
- **MaxMsgs:** 10 million messages
- **Storage:** File-based (persistent)
- **Retention:** Limits-based (time + size + count)
- **Replicas:** 1 (single-node initially)
- **Discard:** DiscardOld (remove oldest when limits reached)

### Manager Functions

**CreateStream:**
- Validates configuration (required fields)
- Converts Flux stream name to NATS stream name
- Creates stream with NATS JetStream API
- Idempotent: Returns success if stream already exists
- Error handling for actual failures

**StreamExists:**
- Checks if a stream exists by Flux name
- Converts to NATS name before querying
- Returns boolean + error

**InitializeStreams:**
- Creates multiple streams on startup
- Best-effort initialization (continues on errors)
- Logs progress for each stream
- Returns first error encountered (if any)

### Startup Integration

Modified `main.go` to initialize default streams on startup:
- Creates `alarms.events` stream
- Creates `sensor.readings` stream
- Logs initialization progress
- Continues even if initialization fails (graceful degradation)

---

## Files Created

**`/flux-service/internal/streams/manager.go`** (132 lines)
- StreamConfig struct
- DefaultStreamConfig function
- Manager struct
- NewManager function
- CreateStream function (idempotent)
- StreamExists function
- InitializeStreams function
- toNATSStreamName helper (stream name conversion)

**`/flux-service/internal/streams/manager_test.go`** (384 lines)
- 9 test functions covering:
  - DefaultStreamConfig validation
  - NewManager creation
  - CreateStream with default config
  - CreateStream with custom config
  - Idempotent stream creation
  - Invalid configuration handling
  - StreamExists checks
  - InitializeStreams (multiple streams)
  - InitializeStreams with nil logger
- Test helpers:
  - getTestJetStream (with fallback URLs)
  - cleanupStream (with NATS name conversion)
  - String contains/indexOf helpers

---

## Files Modified

**`/flux-service/main.go`**
- Added import: `github.com/flux/flux-service/internal/streams`
- Added stream initialization after JetStream verification
- Initializes two default streams: `alarms.events`, `sensor.readings`
- Logs stream initialization progress

**`/CLAUDE.md`**
- Marked stream management task as complete

---

## Testing

### Test Results

**All 9 tests passing:**
```
=== RUN   TestDefaultStreamConfig
--- PASS: TestDefaultStreamConfig (0.00s)
=== RUN   TestNewManager
--- PASS: TestNewManager (0.01s)
=== RUN   TestCreateStream_Success
--- PASS: TestCreateStream_Success (0.01s)
=== RUN   TestCreateStream_CustomConfig
--- PASS: TestCreateStream_CustomConfig (0.01s)
=== RUN   TestCreateStream_Idempotent
--- PASS: TestCreateStream_Idempotent (0.01s)
=== RUN   TestCreateStream_InvalidConfig
--- PASS: TestCreateStream_InvalidConfig (0.00s)
=== RUN   TestStreamExists
--- PASS: TestStreamExists (0.01s)
=== RUN   TestInitializeStreams
--- PASS: TestInitializeStreams (0.05s)
=== RUN   TestInitializeStreams_NilLogger
--- PASS: TestInitializeStreams_NilLogger (0.01s)
PASS
ok  	github.com/flux/flux-service/internal/streams	0.133s
```

### Test Command

**Run tests in Docker:**
```bash
cd /home/etl/projects/flux/flux-service
docker run --rm -v "$(pwd):/workspace" -w /workspace \
  --network flux_flux-network \
  golang:1.22-alpine sh -c "go test -v ./internal/streams/..."
```

**Note:** Tests require NATS with JetStream running:
```bash
docker-compose up -d
```

---

## Startup Verification

**Service logs showing successful stream initialization:**
```
2026/02/10 23:58:50 Flux Service starting...
2026/02/10 23:58:50 NATS URL: nats://nats:4222
2026/02/10 23:58:50 Service port: 8090
2026/02/10 23:58:50 Connecting to NATS at nats://nats:4222...
2026/02/10 23:58:50 Connected to NATS successfully
2026/02/10 23:58:50 JetStream enabled: true
2026/02/10 23:58:50 JetStream account info:
2026/02/10 23:58:50   Memory: 0 bytes
2026/02/10 23:58:50   Storage: 0 bytes
2026/02/10 23:58:50   Streams: 1
2026/02/10 23:58:50   Consumers: 3
2026/02/10 23:58:50 Initializing streams...
2026/02/10 23:58:50 Initializing stream: alarms.events
2026/02/10 23:58:50 Stream alarms.events ready (retention: 168h0m0s, max size: 10GB, max msgs: 10M)
2026/02/10 23:58:50 Initializing stream: sensor.readings
2026/02/10 23:58:50 Stream sensor.readings ready (retention: 168h0m0s, max size: 10GB, max msgs: 10M)
2026/02/10 23:58:50 Flux Service ready on port 8090
```

**Verification:**
- ✅ Service starts successfully
- ✅ Connects to NATS
- ✅ Initializes two streams
- ✅ Logs retention policies correctly (168h = 7 days)
- ✅ Shows correct max size (10GB) and max messages (10M)

---

## Technical Details

### NATS Stream Naming Convention

**Challenge:** NATS stream names have restrictions (no dots), but Flux uses hierarchical dot notation.

**Solution:**
- User-facing (Flux): `alarms.events` (readable, hierarchical)
- Internal (NATS): `ALARMS_EVENTS` (uppercase, underscores)
- Subjects: `alarms.events` (hierarchical routing)

**Benefits:**
- Users work with intuitive dot notation
- NATS constraints are satisfied
- Subjects enable hierarchical wildcards (`alarms.*`, `alarms.>`)

### Idempotent Stream Creation

**Behavior:**
- First call: Creates stream, returns success
- Second call: Detects existing stream, returns success (no error)
- Error only on actual failures (network, permissions, etc.)

**Implementation:**
```go
_, err := m.js.AddStream(streamConfig)
if err != nil {
    if err == nats.ErrStreamNameAlreadyInUse {
        // Verify stream exists and is accessible
        info, streamErr := m.js.StreamInfo(natsStreamName)
        if streamErr != nil {
            return fmt.Errorf("stream exists but cannot get info: %w", streamErr)
        }
        return nil // Success (idempotent)
    }
    return fmt.Errorf("failed to create stream: %w", err)
}
```

### Graceful Initialization

**Design:** Stream initialization continues even if some streams fail.
- Logs each stream initialization
- Records first error encountered
- Returns error at end but doesn't abort
- Allows service to start even with partial initialization

**Rationale:** Better availability - service can start and handle requests even if some streams fail to initialize.

---

## Verification Checklist

- [x] StreamConfig struct with retention policies
- [x] DefaultStreamConfig with ADR-001 defaults
- [x] Manager struct with JetStream context
- [x] CreateStream function (idempotent)
- [x] StreamExists function
- [x] InitializeStreams function
- [x] NATS stream name conversion (toNATSStreamName)
- [x] Stream creation tests (default and custom config)
- [x] Idempotent behavior tests
- [x] Invalid configuration tests
- [x] StreamExists tests
- [x] InitializeStreams tests
- [x] main.go integration (startup initialization)
- [x] Service starts successfully
- [x] Logs show stream creation
- [x] All tests passing
- [x] CLAUDE.md updated

---

## Issues Encountered

### Issue 1: NATS Stream Name Validation

**Problem:** Test failures with "nats: invalid stream name" error.

**Root Cause:** NATS stream names cannot contain dots (`.`). Flux uses dot notation for hierarchy (`alarms.events`).

**Solution:**
- Introduced `toNATSStreamName()` conversion function
- Converts `alarms.events` → `ALARMS_EVENTS`
- Applied conversion in CreateStream and StreamExists
- Subjects still use dot notation for hierarchical routing

### Issue 2: Test Cleanup

**Problem:** StreamExists test failing due to streams persisting between tests.

**Root Cause:** `cleanupStream()` was using Flux stream name instead of NATS stream name when deleting.

**Solution:**
- Updated `cleanupStream()` to convert Flux name to NATS name before deletion
- Ensures proper cleanup between test runs

### Issue 3: Docker Network Connectivity

**Problem:** Tests skipping because they couldn't connect to NATS.

**Root Cause:** Test container trying to connect to `localhost:4223`, but NATS is on a different Docker network.

**Solution:**
- Updated test connection logic to try multiple URLs
- First tries `nats://flux-nats:4222` (Docker network)
- Falls back to `nats://localhost:4223` (local testing)
- Tests now work in both environments

---

## Next Steps

**Phase 1 Task 4:** Publish API Implementation (suggested)
- gRPC or HTTP endpoint for event publishing
- Integrate event validation (ValidateAndPrepare from task 2)
- Integrate stream manager (verify stream exists from task 3)
- Publish validated events to NATS JetStream
- Return confirmation (eventId, sequence)

**Alternative:** Subscribe API or Example Producer/Consumer

**Dependencies:**
- Event model (Task 2) ✅
- Stream management (Task 3) ✅

---

## Notes

- Stream manager is foundation for publish/subscribe APIs
- NATS handles all data plane operations (high throughput)
- Flux manages control plane (validation, authorization, stream setup)
- File storage provides persistence across restarts
- Limits-based retention automatically manages storage
- Default streams (`alarms.events`, `sensor.readings`) demonstrate conventions
- Additional streams can be added via configuration or API (future)

---

## Files Modified Summary

**Created:**
- `/flux-service/internal/streams/manager.go` (132 lines)
- `/flux-service/internal/streams/manager_test.go` (384 lines)
- `/docs/sessions/2026-02-10-phase1-task3-stream-management.md` (this file)

**Modified:**
- `/flux-service/main.go` (added stream initialization)
- `/CLAUDE.md` (status update)

**Total:** 516 new lines of code + tests, 2 files modified

---

**Session completed successfully.**
