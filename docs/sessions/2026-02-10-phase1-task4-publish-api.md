# Session: Phase 1 Task 4 - Publish API

**Date:** 2026-02-11
**Session:** 2026-02-10-phase1-task4-publish-api
**Status:** ✅ Complete

---

## Objective

Implement Flux publish API (control plane validation → NATS publish).

---

## Scope

1. Create Publisher struct that wraps NATS JetStream client
2. Implement Publish method:
   - Accept event from producer
   - Validate using event model (ValidateAndPrepare)
   - Verify stream exists (or auto-create)
   - Publish to NATS JetStream
   - Return confirmation (eventId, sequence number)
3. Add authorization placeholder (for Phase 2 - just log for now)
4. Write tests:
   - Publish valid event (returns eventId)
   - Publish to non-existent stream (auto-creates)
   - Publish invalid event (validation fails)
   - Verify event persisted in NATS
5. Update main.go with simple test publish on startup

---

## Implementation

### Publisher Architecture

**Key Design:** Publisher is a control plane component that validates and forwards to NATS data plane.

**Flow:**
1. Producer provides Event struct
2. Publisher calls ValidateAndPrepare() → generates eventId if missing
3. Publisher checks stream exists → auto-creates with defaults if not
4. Publisher serializes event to JSON
5. Publisher publishes to NATS JetStream
6. NATS returns PubAck with sequence number
7. Publisher returns PublishResult (eventId, stream, sequence)

### Publisher Struct

```go
type Publisher struct {
    js            nats.JetStreamContext
    streamManager *streams.Manager
}
```

**Dependencies:**
- NATS JetStream context (data plane)
- Stream manager (control plane - verify/create streams)

### PublishResult

```go
type PublishResult struct {
    EventID  string // UUIDv7 event identifier
    Stream   string // Stream name where event was published
    Sequence uint64 // NATS sequence number
}
```

Returns confirmation to producer with:
- Generated/provided eventId
- Stream name
- NATS sequence number (monotonic per stream)

### Publish Method Flow

**Validation:**
1. Check event is not nil
2. Log authorization placeholder (Phase 2 TODO)
3. Call event.ValidateAndPrepare():
   - Generates eventId if missing
   - Validates required fields (stream, source, timestamp, payload)
   - Validates formats (stream name, timestamp, payload JSON)

**Stream Management:**
1. Check if stream exists (streamManager.StreamExists)
2. If not exists → auto-create with DefaultStreamConfig
3. Log auto-creation for visibility

**Publishing:**
1. Serialize event to JSON (json.Marshal)
2. Publish to NATS subject (same as stream name)
3. NATS JetStream returns PubAck with sequence
4. Return PublishResult

**Error Handling:**
- Validation errors → return immediately
- Stream check errors → return with context
- Auto-create errors → return with context
- Publish errors → return with context

### Authorization Placeholder

Phase 1 implementation logs authorization intent:
```go
log.Printf("Authorization placeholder: allowing publish to stream '%s' from source '%s'",
    event.Stream, event.Source)
```

Phase 2 will implement:
- Stream-level permissions
- Source identity verification
- NATS ACLs integration

---

## Files Created

**`/flux-service/internal/publisher/publisher.go`** (92 lines)
- Publisher struct
- PublishResult struct
- NewPublisher constructor
- Publish method (validation + stream check + NATS publish)
- Authorization placeholder

**`/flux-service/internal/publisher/publisher_test.go`** (409 lines)
- 8 test functions covering:
  - NewPublisher creation
  - Valid event publish
  - EventId generation
  - Auto-create stream
  - Invalid events (6 subtests)
  - Event persisted in NATS
  - Preserve existing eventId
- Test helpers:
  - getTestJetStream (multi-URL fallback)
  - cleanupStream (proper NATS name conversion)
  - toNATSStreamName helper

---

## Files Modified

**`/flux-service/main.go`**
- Added imports: `internal/model`, `internal/publisher`
- Created Publisher instance after stream initialization
- Added test event publish on startup:
  - Publishes to `alarms.events` stream
  - Source: `flux-service-startup`
  - Payload: service startup message
  - Logs success or failure

**`/CLAUDE.md`**
- Marked "Example producer implementation" as complete (publish API covers this)

---

## Testing

### Test Results

**All 7 tests passing:**
```
=== RUN   TestNewPublisher
--- PASS: TestNewPublisher (0.00s)
=== RUN   TestPublish_ValidEvent
--- PASS: TestPublish_ValidEvent (0.02s)
=== RUN   TestPublish_GeneratesEventID
--- PASS: TestPublish_GeneratesEventID (0.01s)
=== RUN   TestPublish_AutoCreateStream
--- PASS: TestPublish_AutoCreateStream (0.02s)
=== RUN   TestPublish_InvalidEvent
    --- PASS: TestPublish_InvalidEvent/nil_event (0.00s)
    --- PASS: TestPublish_InvalidEvent/missing_stream (0.00s)
    --- PASS: TestPublish_InvalidEvent/missing_source (0.00s)
    --- PASS: TestPublish_InvalidEvent/missing_timestamp (0.00s)
    --- PASS: TestPublish_InvalidEvent/missing_payload (0.00s)
    --- PASS: TestPublish_InvalidEvent/invalid_stream_name (0.00s)
--- PASS: TestPublish_InvalidEvent (0.01s)
=== RUN   TestPublish_EventPersistedInNATS
--- PASS: TestPublish_EventPersistedInNATS (0.05s)
=== RUN   TestPublish_PreservesExistingEventID
--- PASS: TestPublish_PreservesExistingEventID (0.01s)
PASS
ok      github.com/flux/flux-service/internal/publisher    0.132s
```

### Test Command

**Run tests in Docker:**
```bash
cd /home/etl/projects/flux/flux-service
docker run --rm -v "$(pwd):/workspace" -w /workspace \
  --network flux_flux-network \
  golang:1.22-alpine sh -c "go test -v ./internal/publisher/..."
```

---

## Startup Verification

**Service logs showing successful test publish:**
```
2026/02/11 12:23:30 Initializing streams...
2026/02/11 12:23:30 Initializing stream: alarms.events
2026/02/11 12:23:30 Stream alarms.events ready (retention: 168h0m0s, max size: 10GB, max msgs: 10M)
2026/02/11 12:23:30 Initializing stream: sensor.readings
2026/02/11 12:23:30 Stream sensor.readings ready (retention: 168h0m0s, max size: 10GB, max msgs: 10M)
2026/02/11 12:23:30 Testing publish API with startup event...
2026/02/11 12:23:30 Authorization placeholder: allowing publish to stream 'alarms.events' from source 'flux-service-startup'
2026/02/11 12:23:30 Published event 019c4ca8-1714-7983-b531-2e487ac5b1bb to stream alarms.events (sequence: 1)
2026/02/11 12:23:30 Test publish successful: eventId=019c4ca8-1714-7983-b531-2e487ac5b1bb, sequence=1
2026/02/11 12:23:30 Flux Service ready on port 8090
```

**Verification:**
- ✅ Streams initialized
- ✅ Publisher created successfully
- ✅ Test event validated and published
- ✅ EventId generated (UUIDv7)
- ✅ Sequence number returned (1)
- ✅ Service ready

---

## Technical Details

### Auto-Create Stream Behavior

**Design Decision:** Auto-create streams on first publish rather than requiring pre-creation.

**Rationale:**
- Reduces operational friction (producers don't need pre-setup)
- Uses sensible defaults (7 days, 10GB, 10M messages)
- Aligns with Phase 1 simplicity goals
- Can be disabled in Phase 2 if needed

**Implementation:**
```go
exists, err := p.streamManager.StreamExists(event.Stream)
if !exists {
    log.Printf("Stream '%s' does not exist, auto-creating with default config", event.Stream)
    config := streams.DefaultStreamConfig(event.Stream)
    if err := p.streamManager.CreateStream(config); err != nil {
        return nil, fmt.Errorf("failed to auto-create stream: %w", err)
    }
}
```

**Trade-offs:**
- ✅ Convenience: Producers just publish
- ✅ Flexibility: Streams created on-demand
- ⚠️ Control: Can create streams with unexpected names
- ⚠️ Resource: Could create many streams (mitigated by stream naming validation)

### Event Serialization

Events are serialized to JSON before publishing:
```go
eventJSON, err := json.Marshal(event)
```

**Format:**
- Compact JSON (no extra whitespace)
- All envelope fields included
- Payload preserved as-is (already json.RawMessage)

**Example serialized event:**
```json
{
  "eventId": "019c4ca8-1714-7983-b531-2e487ac5b1bb",
  "stream": "alarms.events",
  "source": "flux-service-startup",
  "timestamp": 1738413810000,
  "schema": "service.startup.v1",
  "payload": {"message": "Flux service started", "status": "ready"}
}
```

### NATS Publish Details

**Subject Mapping:**
- Flux stream name = NATS subject (e.g., "alarms.events")
- NATS stream name is different (e.g., "ALARMS_EVENTS")
- Subject enables hierarchical wildcards in subscriptions

**JetStream Publish:**
```go
pubAck, err := p.js.Publish(event.Stream, eventJSON)
```

Returns `PubAck` with:
- Stream name (NATS internal name)
- Sequence number (monotonic per stream, starts at 1)
- Domain (if using multi-cluster)

**Acknowledgement:**
- Synchronous (blocks until NATS confirms persistence)
- At-least-once delivery (NATS persists before returning)
- Idempotent on retry (same eventId, different sequence)

---

## Verification Checklist

- [x] Publisher struct with JetStream and StreamManager
- [x] PublishResult struct with eventId, stream, sequence
- [x] Publish method validates events
- [x] Publish method auto-creates streams
- [x] Publish method returns confirmation
- [x] Authorization placeholder logged
- [x] Test: NewPublisher
- [x] Test: Valid event publish
- [x] Test: EventId generation
- [x] Test: Auto-create stream
- [x] Test: Invalid events (6 variants)
- [x] Test: Event persisted in NATS
- [x] Test: Preserve existing eventId
- [x] All tests passing
- [x] main.go integration (startup test publish)
- [x] Service starts successfully
- [x] Logs show test publish
- [x] CLAUDE.md updated

---

## Issues Encountered

### Issue 1: NATS API for Subscribe Options

**Problem:** Test used `nats.StartWithLastReceived()` which doesn't exist in nats.go API.

**Root Cause:** Incorrect function name - documentation uses `DeliverLast()` instead.

**Solution:**
```go
// Before (incorrect):
sub, err := js.SubscribeSync(streamName, nats.StartWithLastReceived())

// After (correct):
sub, err := js.SubscribeSync(streamName, nats.DeliverLast())
```

### Issue 2: JSON Payload Comparison in Tests

**Problem:** Test compared JSON string representations, which failed due to whitespace differences.

**Input:** `{"sensor": "temp-01", "value": 23.5}` (with spaces)
**Output:** `{"sensor":"temp-01","value":23.5}` (without spaces)

**Root Cause:** `json.Marshal()` removes extra whitespace (compact format).

**Solution:** Compare deserialized JSON objects instead of strings:
```go
var originalData, retrievedData map[string]interface{}
json.Unmarshal(originalPayload, &originalData)
json.Unmarshal(retrievedEvent.Payload, &retrievedData)
// Compare field by field
```

### Issue 3: Docker Compose Container Recreation

**Problem:** `docker-compose up -d --build` failed with `KeyError: 'ContainerConfig'`.

**Root Cause:** Legacy docker-compose issue when recreating containers with volume bindings.

**Solution:** Stop, remove, and start fresh:
```bash
docker-compose stop flux-service
docker-compose rm -f flux-service
docker-compose up -d flux-service
```

---

## Next Steps

**Phase 1 Task 5:** Subscribe API / Consumer Example (suggested)
- Implement consumer authorization (Flux control plane)
- Return NATS connection details
- Example consumer: read directly from NATS
- Demonstrate replay (from beginning, timestamp, sequence)

**Alternative:** Python client examples demonstrating publish/subscribe

**Dependencies:**
- Event model (Task 2) ✅
- Stream management (Task 3) ✅
- Publish API (Task 4) ✅

---

## Notes

- Publisher is control plane only (validates, authorizes, facilitates)
- Actual event data flows through NATS data plane (high throughput)
- Auto-create streams uses sensible defaults (7d, 10GB, 10M msgs)
- Authorization is placeholder (Phase 2 will implement ACLs)
- Publish is synchronous (blocks until NATS confirms)
- Sequence numbers are monotonic per stream (consumers use for ordering)
- EventId generation uses UUIDv7 (time-ordered for efficiency)
- Tests verify both happy path and error cases
- Startup test publish demonstrates end-to-end flow

---

## Files Modified Summary

**Created:**
- `/flux-service/internal/publisher/publisher.go` (92 lines)
- `/flux-service/internal/publisher/publisher_test.go` (409 lines)
- `/docs/sessions/2026-02-10-phase1-task4-publish-api.md` (this file)

**Modified:**
- `/flux-service/main.go` (added publisher initialization and test publish)
- `/CLAUDE.md` (status update)

**Total:** 501 new lines of code + tests, 2 files modified

---

**Session completed successfully.**
