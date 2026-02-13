# Phase 2 Task 4: Recovery on Startup

**Date:** 2026-02-12
**Status:** ✅ Complete
**Task:** Implement recovery logic to load snapshot on startup and replay events

---

## Overview

Added recovery logic to restore state from snapshots on startup. The system now:
- Finds and loads the most recent valid snapshot
- Populates StateEngine with snapshot data
- Resumes NATS event replay from snapshot sequence + 1
- Falls back to cold start if no snapshots exist

---

## Files Created

### `/src/snapshot/recovery.rs` (200 lines)
Recovery logic for loading snapshots on startup.

**Key functions:**
- `load_latest_snapshot(snapshot_dir)` - Find and load most recent valid snapshot
  - Returns `Option<(Snapshot, u64)>` - snapshot + sequence number
  - Lists all snapshots in directory
  - Sorts by timestamp (newest first)
  - Tries loading snapshots until one succeeds
  - Falls back to next oldest on corrupt snapshot
  - Returns None for cold start (no valid snapshots)

**Error handling:**
- Missing directory → cold start (log info)
- Empty directory → cold start (log info)
- Corrupt snapshot → try next oldest (log warning)
- All snapshots corrupt → cold start (log error)

**Tests:**
- `test_load_latest_snapshot_no_directory` - Non-existent directory
- `test_load_latest_snapshot_empty_directory` - Empty directory
- `test_load_latest_snapshot_success` - Load valid snapshot
- `test_load_latest_snapshot_picks_newest` - Select newest among multiple
- `test_load_latest_snapshot_fallback_on_corrupt` - Fall back to older on corrupt
- `test_load_latest_snapshot_all_corrupt` - Cold start when all corrupt

---

## Files Modified

### `/src/snapshot/mod.rs`
- Added `pub mod recovery;` to export recovery module

### `/src/state/engine.rs`
Added `load_from_snapshot()` method:
```rust
pub fn load_from_snapshot(&self, entities: HashMap<String, Entity>, sequence: u64)
```
- Clears existing state (DashMap.clear())
- Loads entities from snapshot
- Sets `last_processed_sequence` to snapshot sequence
- Logs: entities loaded, sequence number

Modified `run_subscriber()` to accept optional start sequence:
```rust
pub async fn run_subscriber(
    self: Arc<Self>,
    jetstream: jetstream::Context,
    start_sequence: Option<u64>
) -> Result<()>
```
- Uses `DeliverPolicy::ByStartSequence { start_sequence: seq + 1 }` if Some(seq)
- Uses `DeliverPolicy::All` if None (cold start)
- Logs recovery mode and start sequence

### `/src/main.rs`
Added recovery logic before starting subscriber:
```rust
let snapshot_dir = PathBuf::from("/var/lib/flux/snapshots");
let start_sequence = match recovery::load_latest_snapshot(&snapshot_dir)? {
    Some((snapshot, seq)) => {
        info!("Loaded snapshot: seq={}, entities={}", seq, snapshot.entity_count());
        state_engine.load_from_snapshot(snapshot.to_hashmap(), seq);
        Some(seq)
    }
    None => {
        info!("No snapshot found, starting from beginning");
        None
    }
};

// Pass start_sequence to run_subscriber
engine_clone.run_subscriber(jetstream_clone, start_sequence).await
```

**Logging:**
- Snapshot loaded: `"Loaded snapshot: seq={}, entities={}"`
- No snapshot: `"No snapshot found, starting from beginning"`
- Recovery mode: `"Recovering from snapshot, replaying events from sequence {}"`
- Cold start: `"No snapshot, processing all events from beginning"`

### `/src/state/tests.rs`
Added tests for `load_from_snapshot()`:
- `test_load_from_snapshot` - Basic loading
- `test_load_from_snapshot_clears_existing_state` - Verify old state cleared
- `test_load_from_empty_snapshot` - Empty snapshot handling

---

## Implementation Details

### Recovery Flow

**On Startup:**
1. Create StateEngine (empty)
2. Call `recovery::load_latest_snapshot(&snapshot_dir)`
3. If snapshot found:
   - Load entities into StateEngine via `load_from_snapshot()`
   - Set last_processed_sequence to snapshot sequence
   - Start NATS consumer from `seq + 1` (DeliverPolicy::ByStartSequence)
4. If no snapshot:
   - Start NATS consumer from beginning (DeliverPolicy::All)

**Snapshot Selection:**
- Lists all `snapshot-*.json` files in directory
- Sorts by filename (timestamp is lexicographically sortable)
- Tries newest first, falls back to older on error
- Returns None if all fail (triggers cold start)

**NATS Consumer Configuration:**
```rust
// With snapshot (recovery)
deliver_policy: DeliverPolicy::ByStartSequence {
    start_sequence: snapshot_seq + 1  // Resume after snapshot
}

// Without snapshot (cold start)
deliver_policy: DeliverPolicy::All  // From beginning
```

### Error Handling

**Corrupt Snapshots:**
- Log warning: `"Corrupt snapshot {path}, trying next oldest"`
- Try next oldest snapshot
- If all fail: cold start (log error)

**No Snapshots:**
- Log info: `"No snapshot found, starting from beginning"`
- Start NATS from seq 0

**Missing Directory:**
- Log info: `"Snapshot directory does not exist, starting without snapshot"`
- Continue with cold start

---

## Test Results

All tests pass:
```
test snapshot::recovery::tests::test_load_latest_snapshot_all_corrupt ... ok
test snapshot::recovery::tests::test_load_latest_snapshot_empty_directory ... ok
test snapshot::recovery::tests::test_load_latest_snapshot_fallback_on_corrupt ... ok
test snapshot::recovery::tests::test_load_latest_snapshot_no_directory ... ok
test snapshot::recovery::tests::test_load_latest_snapshot_picks_newest ... ok
test snapshot::recovery::tests::test_load_latest_snapshot_success ... ok
test state::tests::test_load_from_empty_snapshot ... ok
test state::tests::test_load_from_snapshot ... ok
test state::tests::test_load_from_snapshot_clears_existing_state ... ok

test result: ok. 48 passed; 0 failed; 0 ignored
```

Build successful:
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.45s
```

---

## Design Decisions

**Snapshot Directory:**
- Hardcoded to `/var/lib/flux/snapshots` for Phase 2
- Task 6 (Documentation) will add config.toml support

**Corrupt Snapshot Handling:**
- Log and try next oldest (simple, effective)
- No `.corrupt` marker files (avoids cleanup complexity)
- Can add markers later if needed

**Sequence Tracking:**
- Use NATS JetStream sequence numbers (built-in, monotonic)
- Store in snapshot, restore on load
- Pass to NATS consumer via DeliverPolicy

**State Clearing:**
- `load_from_snapshot()` clears existing state first
- Ensures clean slate on recovery
- Important for restart scenarios

---

## Verification Steps

**Manual Testing (Future):**
1. Start Flux → publishes events → creates snapshots
2. Stop Flux
3. Restart Flux → should load latest snapshot
4. Check logs: `"Loaded snapshot: seq=X, entities=Y"`
5. Check logs: `"Recovering from snapshot, replaying events from sequence X+1"`
6. Verify state matches pre-restart state

**Integration Test Scenario:**
1. Publish 100 events
2. Wait for snapshot (seq 100)
3. Publish 50 more events (total 150)
4. Restart service
5. Verify: snapshot loaded (seq 100), replayed 50 events, state complete

---

## Next Steps

**Phase 2 Remaining:**
- ✅ Task 1: Snapshot core (complete)
- ✅ Task 2: Sequence tracking (complete)
- ✅ Task 3: Snapshot manager (complete)
- ✅ Task 4: Recovery on startup (complete)
- ⏳ Task 5: Production improvements (atomicity, compression)
- ⏳ Task 6: Documentation & testing

---

## Notes

- Recovery logic is simple and robust
- Falls back gracefully on any error
- Logs clearly indicate recovery mode vs. cold start
- All existing tests continue to pass
- No breaking changes to existing APIs
