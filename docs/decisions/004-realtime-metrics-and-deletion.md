# ADR-004: Real-Time Metrics & Entity Management

**Status:** Accepted
**Date:** 2026-02-14
**Context:** Phase 4A - Real-time metrics and entity deletion

---

## Decision

Add real-time metrics broadcasting and event-sourced entity deletion to Flux.

---

## Rationale

**Problem:**
- UI had fake client-side metrics (guessed EPS, hardcoded values)
- No way to delete entities (manual NATS cleanup required)
- No visibility into Flux performance

**Solution:**
1. Real-time metrics via separate WebSocket channel
2. DELETE API with tombstone events
3. Event-sourced deletion (survives restart)

---

## Implementation

### Metrics Architecture

**MetricsTracker** (lock-free):
- AtomicU64 for counters (event count, WS connections)
- Sliding window for event rate (VecDeque with 5s window)
- HashMap for active publisher tracking
- Lazy cleanup (triggered by new events)

**Metrics Broadcaster**:
- Separate broadcast channel (capacity: 10)
- Background task broadcasts every 2 seconds (configurable)
- Doesn't compete with state update channel

**Metrics Included:**
- Total entities
- Total events
- Event rate (events/second)
- Active publishers (within time window)
- WebSocket connections

### Deletion Architecture

**Tombstone Events:**
- Special payload marker: `__deleted__: true`
- Published to NATS like normal events
- Stream: `flux.deletions`

**Processing:**
- State engine detects tombstone in `process_event()`
- Removes entity from DashMap
- Broadcasts `entity_deleted` message via WebSocket
- Snapshots naturally exclude deleted entities

**API Endpoints:**
- `DELETE /api/state/entities/:id` - Single entity
- `POST /api/state/entities/delete` - Batch with filters (namespace, prefix, entity_ids)

**Authorization:**
- When `auth_enabled=true`: Bearer token must own namespace
- When `auth_enabled=false`: Open access (internal mode)
- Batch delete limited to 10,000 entities (configurable)

---

## Consequences

**Benefits:**
- ✅ UI shows real server metrics (no guessing)
- ✅ Entities can be deleted via API
- ✅ Deletions persist across restarts (event-sourced)
- ✅ Metrics don't impact state update performance
- ✅ Authorization follows existing namespace model

**Trade-offs:**
- Minimal overhead: ~0.1% at 10K eps
- Three broadcast channels (state, metrics, deletions)
- Tombstone events stored in NATS (subject to retention policy)

---

## Configuration

```toml
[metrics]
broadcast_interval_seconds = 2
active_publisher_window_seconds = 10

[api]
max_batch_delete = 10000
```

---

## Alternatives Considered

**HTTP polling for metrics:** Rejected - adds latency, increases load
**Direct entity deletion:** Rejected - not event-sourced, state inconsistent after restart
**Single broadcast channel:** Rejected - metrics would compete with state updates
