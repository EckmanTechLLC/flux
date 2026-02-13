# Phase 2 Task 2: Sequence Number Tracking

**Date:** 2026-02-12
**Task:** Add NATS sequence number tracking to StateEngine
**Status:** Complete ✅

---

## Goal

Add sequence number tracking to StateEngine to enable snapshot recovery from specific NATS stream positions.

---

## Context

Task 2 of Phase 2 (Persistence & Recovery).

**Requirements:**
- Track last processed NATS sequence number in StateEngine
- Extract sequence from NATS message metadata
- Store atomically after successful event processing
- Expose via `get_last_processed_sequence()` method

**Reference:**
- ADR-002 lines 276-291 (sequence tracking design)
- Task breakdown in `/docs/sessions/2026-02-12-phase2-planning.md` lines 96-110

---

## Implementation

### 1. Files Modified

**`src/state/engine.rs`:**
- Added import: `std::sync::atomic::{AtomicU64, Ordering}`
- Added field: `last_processed_sequence: AtomicU64` to StateEngine struct
- Initialized to 0 in `new()` constructor
- Updated `run_subscriber()` to extract sequence from `msg.info()?.stream_sequence`
- Store sequence after successful event processing: `self.last_processed_sequence.store(sequence, Ordering::SeqCst)`
- Added method: `pub fn get_last_processed_sequence(&self) -> u64`

**`src/state/tests.rs`:**
- Added test: `test_initial_sequence_is_zero()` - Verifies new engine starts at 0
- Added test: `test_sequence_tracking_thread_safe()` - Verifies concurrent reads work

### 2. Key Changes

**StateEngine struct (lines 14-23):**
```rust
pub struct StateEngine {
    entities: Arc<DashMap<String, Entity>>,
    state_tx: broadcast::Sender<StateUpdate>,
    last_processed_sequence: AtomicU64,  // NEW
}
```

**Initialization (lines 26-34):**
```rust
pub fn new() -> Self {
    let (state_tx, _) = broadcast::channel(1000);
    Self {
        entities: Arc::new(DashMap::new()),
        state_tx,
        last_processed_sequence: AtomicU64::new(0),  // NEW
    }
}
```

**Getter method (lines 92-95):**
```rust
pub fn get_last_processed_sequence(&self) -> u64 {
    self.last_processed_sequence.load(Ordering::SeqCst)
}
```

**Sequence extraction and storage in run_subscriber() (lines 171-186):**
```rust
// Extract NATS sequence number
let sequence = match msg.info() {
    Ok(info) => info.stream_sequence,
    Err(e) => {
        error!(error = %e, "Failed to get message info");
        let _ = msg.ack().await;
        continue;
    }
};

// Deserialize and process event
match serde_json::from_slice::<FluxEvent>(&msg.payload) {
    Ok(event) => {
        self.process_event(&event);
        // Store sequence after successful processing
        self.last_processed_sequence.store(sequence, Ordering::SeqCst);
        msg.ack().await?;
    }
    ...
}
```

---

## Testing

### Unit Tests

**Test results:**
```
cargo test --lib
running 33 tests
...
test state::tests::test_initial_sequence_is_zero ... ok
test state::tests::test_sequence_tracking_thread_safe ... ok
...
test result: ok. 33 passed; 0 failed
```

**New tests:**
1. `test_initial_sequence_is_zero` - Confirms new StateEngine starts at sequence 0
2. `test_sequence_tracking_thread_safe` - Verifies AtomicU64 allows concurrent reads

**Test coverage:**
- ✅ Initial sequence is 0
- ✅ Thread-safe reads via AtomicU64
- ⚠️ Integration test for sequence updates requires NATS (future Task 4)

---

## Design Decisions

### Why AtomicU64?
- Thread-safe updates from NATS subscriber (single writer)
- Lock-free reads for snapshot creation (multiple readers)
- SeqCst ordering ensures visibility across threads

### Why store after process_event()?
- Guarantees sequence only advances for successfully processed events
- Failed deserialization doesn't update sequence
- Consistent with event acknowledgment (ack after processing)

### Why extract from msg.info()?
- NATS JetStream provides stream_sequence per message
- Monotonically increasing, guaranteed by NATS
- No custom sequence management needed

---

## Scope Verification

**In scope (implemented):**
- ✅ AtomicU64 field for last_processed_sequence
- ✅ Extract sequence from msg.info()?.stream_sequence
- ✅ Store after successful event processing
- ✅ get_last_processed_sequence() method
- ✅ Unit tests for initial value and thread safety

**Out of scope:**
- ❌ Snapshot integration (Task 3)
- ❌ Recovery from sequence (Task 4)
- ❌ Persistence of sequence to disk (Task 4)

---

## Next Steps

**Task 3: Snapshot Manager**
- Use `get_last_processed_sequence()` when creating snapshots
- Store sequence in snapshot JSON (`"sequence_number": ...`)
- Background loop to create snapshots periodically

**Task 4: Recovery on Startup**
- Load snapshot sequence from disk
- Use sequence to start NATS consumer from `snapshot_seq + 1`

---

## Files Changed

1. `/home/etl/projects/flux/src/state/engine.rs` (modified)
2. `/home/etl/projects/flux/src/state/tests.rs` (modified)
3. `/home/etl/projects/flux/docs/sessions/2026-02-12-phase2-task2-sequence-tracking.md` (created)

---

## Summary

Task 2 complete. StateEngine now tracks NATS sequence numbers atomically. Sequence advances only on successful event processing. Ready for Task 3 (Snapshot Manager) integration.
