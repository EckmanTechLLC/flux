# Phase 2 Task 3: Snapshot Manager

**Date:** 2026-02-12
**Status:** ✅ Complete
**Task:** Implement background snapshot manager for periodic state persistence

---

## Objective

Create a background task that periodically saves state snapshots to the filesystem, implementing automatic cleanup of old snapshots.

---

## Implementation

### Files Created

1. **src/snapshot/config.rs**
   - `SnapshotConfig` struct with fields:
     - `enabled`: bool (default: true)
     - `interval_minutes`: u64 (default: 5)
     - `directory`: PathBuf (default: "/var/lib/flux/snapshots")
     - `keep_count`: usize (default: 10)
   - Default implementation for convenient instantiation

2. **src/snapshot/manager.rs**
   - `SnapshotManager` struct holding StateEngine reference and config
   - `run_snapshot_loop()`: Background task using `tokio::time::interval`
   - `create_and_save_snapshot()`: Creates snapshot and saves to filesystem
   - `snapshot_path()`: Generates filename with timestamp and sequence
   - `cleanup_old_snapshots()`: Deletes old snapshots, keeps most recent N
   - `list_snapshots()`: Lists valid snapshot files in directory

3. **src/snapshot/manager/tests.rs**
   - 6 comprehensive tests covering:
     - Snapshot path format validation
     - Snapshot creation and saving
     - Old snapshot cleanup logic
     - File filtering (only valid snapshots)
     - Disabled manager behavior
     - Sequence number preservation

### Files Modified

1. **src/snapshot/mod.rs**
   - Added `pub mod config;`
   - Added `pub mod manager;`

2. **Cargo.toml**
   - Added `tempfile = "3.14"` to `[dev-dependencies]` for tests

---

## Key Decisions

### Snapshot Filename Format

**Format:** `snapshot-{timestamp}-seq{sequence}.json`
- Timestamp: `%Y%m%dT%H%M%S%.3fZ` (includes milliseconds)
- Sequence: NATS JetStream sequence number
- Example: `snapshot-20260212T153045.123Z-seq12345.json`

**Rationale:**
- Millisecond precision prevents filename collisions
- Lexicographic sorting matches chronological order
- Sequence number embedded for debugging/verification

### Cleanup Strategy

**Approach:** Keep N most recent snapshots
- Sort snapshots by filename (timestamp-based)
- Delete oldest snapshots exceeding `keep_count`
- Runs after each snapshot creation

**Alternative considered:** Time-based cleanup (delete snapshots older than X days)
- Rejected: Count-based is simpler and more predictable
- Time-based can be added later if needed

### Error Handling

**Philosophy:** Log errors but don't crash
- Snapshot failures are logged but don't stop the loop
- Cleanup failures are logged per-file
- Directory creation happens on startup

**Rationale:**
- Snapshot manager is non-critical background task
- Should not bring down entire Flux instance
- Operators can monitor logs for failures

---

## Testing

### Test Coverage

All tests passing (6/6):
```
test snapshot::manager::tests::test_cleanup_old_snapshots ... ok
test snapshot::manager::tests::test_create_and_save_snapshot ... ok
test snapshot::manager::tests::test_disabled_manager_exits_immediately ... ok
test snapshot::manager::tests::test_list_snapshots_filters_correctly ... ok
test snapshot::manager::tests::test_snapshot_path_format ... ok
test snapshot::manager::tests::test_snapshot_preserves_sequence_number ... ok
```

### Test Strategy

1. **Isolated unit tests**: Each function tested independently
2. **Temporary directories**: Use `tempfile::TempDir` for clean test isolation
3. **Time delays**: 10ms sleep between snapshots for timestamp uniqueness
4. **Arc cloning**: Tests clone `Arc<StateEngine>` before passing to manager

---

## Integration Points

### Dependencies from Previous Tasks

- **Task 1 (Snapshot Core):**
  - `Snapshot::from_state_engine()` - Creates snapshot from engine state
  - `Snapshot::save_to_file()` - Persists snapshot to JSON file
  - `Snapshot::entity_count()` - Used for logging

- **Task 2 (Sequence Tracking):**
  - `StateEngine::get_last_processed_sequence()` - Gets current NATS sequence
  - Sequence embedded in snapshot filename and metadata

### Used By (Future Tasks)

- **Task 4 (Recovery on Startup):**
  - Will use `SnapshotManager::list_snapshots()` to find latest snapshot
  - Will load snapshot before starting state engine subscriber

- **Main Application:**
  - Will spawn `run_snapshot_loop()` as background tokio task
  - Will pass `SnapshotConfig` from config file or defaults

---

## Design Notes

### Background Loop Pattern

```rust
pub async fn run_snapshot_loop(&self) -> Result<()> {
    let mut timer = interval(Duration::from_secs(self.config.interval_minutes * 60));

    loop {
        timer.tick().await;

        if let Err(e) = self.create_and_save_snapshot().await {
            error!(error = %e, "Failed to create snapshot");
        }
    }
}
```

**Key aspects:**
- Uses `tokio::time::interval` for precise timing
- First tick happens immediately (after first interval)
- Errors logged but don't stop loop
- Easy to cancel via task cancellation

### Directory Management

**Strategy:** Create on startup, assume persistence
- Directory created once in `run_snapshot_loop()`
- Not re-created on every snapshot
- Assumes directory persists (not tmpfs)

**Future enhancement:** Validate directory on each snapshot
- Add health check for directory accessibility
- Detect permission changes or filesystem issues

---

## Performance Considerations

### Lock-Free State Access

- StateEngine uses `Arc<DashMap>` for lock-free reads
- Snapshot creation doesn't block state updates
- `get_all_entities()` creates owned copies (safe but allocates)

### Filesystem I/O

- Snapshots written synchronously (blocking I/O)
- Currently not a concern (5-minute intervals)
- Future: Consider `tokio::fs` for async writes

### Memory Usage

- Snapshot holds full copy of all entities
- Cleanup deletes files immediately (no deferred GC)
- No compression in this task (added in Task 5)

---

## Future Enhancements (Out of Scope)

1. **Compression** (Task 5)
   - gzip snapshots to reduce disk usage
   - Transparent decompression on load

2. **Atomic Writes** (Task 5)
   - Write to temp file, rename to final
   - Prevents partial snapshots on crash

3. **Metrics**
   - Snapshot duration
   - Snapshot size
   - Success/failure counts

4. **Configurable Cleanup**
   - Time-based cleanup (keep last N days)
   - Size-based cleanup (keep last N GB)

---

## Verification Steps

1. ✅ All unit tests pass (13 snapshot tests total)
2. ✅ Manager tests cover core functionality
3. ✅ Snapshot naming format verified
4. ✅ Cleanup logic tested with multiple snapshots
5. ✅ Config defaults documented

---

## Files Changed Summary

**Created:**
- `src/snapshot/config.rs` (32 lines)
- `src/snapshot/manager.rs` (141 lines)
- `src/snapshot/manager/tests.rs` (175 lines)

**Modified:**
- `src/snapshot/mod.rs` (+2 lines)
- `Cargo.toml` (+3 lines for dev-dependencies)

**Total:** ~353 lines added

---

## Next Steps

**Blocked tasks now unblocked:**
- Task 4: Recovery on Startup (needs snapshot manager to list/load snapshots)

**Recommended next task:**
- Task 4: Implement startup recovery using latest snapshot
- Will complete the persistence/recovery cycle

---

## Notes

- Millisecond timestamps prevent filename collisions in tests
- Manager designed to run continuously, not one-shot
- Config can disable snapshots via `enabled: false`
- All error paths have proper logging for operations visibility
