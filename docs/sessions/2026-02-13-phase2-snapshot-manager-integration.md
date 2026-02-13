# Phase 2 Follow-up: Snapshot Manager Integration

**Date:** 2026-02-13
**Status:** ✅ Complete
**Task:** Integrate SnapshotManager into main.rs for periodic snapshot creation

---

## Objective

Complete the persistence/recovery cycle by spawning the SnapshotManager background task in main.rs. Previously, recovery worked (loaded snapshots on startup) but snapshots were never created during runtime.

---

## Context

Phase 2 Task 3 created `SnapshotManager` with `run_snapshot_loop()` for periodic snapshots, but it was never integrated into main.rs. The snapshot manager runs indefinitely in the background, creating snapshots every N minutes and cleaning up old ones.

**References:**
- ADR-002: Persistence and Recovery (Task 3)
- `/docs/sessions/2026-02-12-phase2-task3-snapshot-manager.md`
- `/src/snapshot/manager.rs` (existing implementation)

---

## Implementation

### Files Modified

**src/main.rs** (2 changes):

1. **Imports** (line 5):
   - Added `config::SnapshotConfig` and `manager::SnapshotManager` to snapshot imports
   - Changed from: `use flux::snapshot::recovery;`
   - Changed to: `use flux::snapshot::{config::SnapshotConfig, manager::SnapshotManager, recovery};`

2. **Background Task Spawn** (after line 63):
   - Created `SnapshotConfig::default()` (uses default values matching config.toml)
   - Created `SnapshotManager` instance with cloned StateEngine
   - Spawned tokio task to run `snapshot_manager.run_snapshot_loop()`
   - Added error logging if snapshot manager fails
   - Added info log "Snapshot manager started"

### Code Changes

```rust
// Start snapshot manager (background task)
let snapshot_config = SnapshotConfig::default();
let snapshot_manager = SnapshotManager::new(Arc::clone(&state_engine), snapshot_config);
tokio::spawn(async move {
    if let Err(e) = snapshot_manager.run_snapshot_loop().await {
        tracing::error!(error = %e, "Snapshot manager failed");
    }
});
info!("Snapshot manager started");
```

**Placement:** Immediately after state engine subscriber starts (line 63), before HTTP server initialization.

---

## Key Decisions

### Configuration Source

**Decision:** Use `SnapshotConfig::default()` directly in main.rs

**Rationale:**
- Default values match config.toml exactly (enabled: true, interval: 5 min, dir: /var/lib/flux/snapshots, keep: 10)
- config.toml exists but isn't currently loaded anywhere in main.rs
- Loading config.toml is out of scope for this focused integration task
- Future: Add proper config file loading when needed

### Task Spawning Pattern

**Pattern:** Same as state engine subscriber
```rust
tokio::spawn(async move {
    if let Err(e) = snapshot_manager.run_snapshot_loop().await {
        tracing::error!(error = %e, "Snapshot manager failed");
    }
});
```

**Rationale:**
- Consistent error handling pattern with subscriber
- Background task doesn't block main execution
- Errors logged but don't crash entire application
- Task runs indefinitely until Flux shuts down

### Directory Creation

**Handled by:** SnapshotManager itself (line 48 in manager.rs)
- Manager creates `/var/lib/flux/snapshots` on first loop iteration
- No need to create directory in main.rs
- Error logged if directory creation fails

---

## Startup Sequence (Complete)

With this change, Flux now has complete persistence/recovery:

```
1. Initialize NATS client
2. Create StateEngine (empty)
3. Load latest snapshot (if exists) → populate StateEngine
4. Start state engine subscriber (replay events since snapshot)
5. Start snapshot manager (periodic snapshots) ← NEW
6. Start HTTP/WebSocket APIs
```

**Runtime behavior:**
- Every 5 minutes: Snapshot manager creates new snapshot
- Cleanup: Keep last 10 snapshots, delete older ones
- Recovery: On next restart, latest snapshot is loaded

---

## Testing

### Build Verification

```bash
$ cargo build
   Compiling flux v0.1.0 (/home/etl/projects/flux)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.84s
```

✅ Compilation successful

### Expected Runtime Logs

```
INFO flux: Flux starting...
INFO flux: NATS client connected
INFO flux: State engine initialized
INFO flux: No snapshot found, starting from beginning
INFO flux: State engine subscriber started
INFO flux::snapshot::manager: Starting snapshot manager interval_minutes=5 directory=/var/lib/flux/snapshots keep_count=10
INFO flux: Snapshot manager started
INFO flux: Starting HTTP server on 0.0.0.0:3000
```

After 5 minutes:
```
INFO flux::snapshot::manager: Snapshot saved sequence=12345 entities=100 path=/var/lib/flux/snapshots/snapshot-20260213T120000.000Z-seq12345.json.gz
```

---

## Verification Checklist

- [x] Imports added correctly
- [x] SnapshotManager spawned after state engine subscriber
- [x] Uses SnapshotConfig::default() (matches config.toml)
- [x] Follows same error handling pattern as subscriber
- [x] Added appropriate logging
- [x] Build succeeds with no errors
- [x] Session notes created

---

## Files Changed

**Modified:**
- `/src/main.rs` (+9 lines)

**Total changes:** 9 lines added (imports + spawn logic)

---

## Impact

### Before This Change
- ✅ Recovery worked (loaded snapshots on startup)
- ❌ Snapshots never created during runtime
- ❌ State lost if no snapshot existed

### After This Change
- ✅ Recovery works (loaded snapshots on startup)
- ✅ Snapshots created every 5 minutes automatically
- ✅ Old snapshots cleaned up (keep last 10)
- ✅ Full persistence/recovery cycle complete

---

## Next Steps

**Phase 2 is now complete** with all tasks finished:
- ✅ Task 1: Snapshot core (serialize, save/load)
- ✅ Task 2: Sequence number tracking
- ✅ Task 3: Snapshot manager (background loop, cleanup)
- ✅ Task 4: Recovery on startup
- ✅ Task 5: Compression & atomicity
- ✅ Task 6: Documentation & configuration
- ✅ **Follow-up: Integrate snapshot manager into main.rs**

**Recommended next:**
- Test end-to-end persistence/recovery with real deployment
- Monitor snapshot creation in production logs
- Verify snapshot cleanup after 10 snapshots

---

## Notes

- SnapshotManager is non-blocking (runs in background)
- First snapshot happens after first interval (5 minutes after startup)
- Manager automatically creates snapshot directory if missing
- Error in snapshot manager doesn't crash Flux (logged only)
- Arc::clone(&state_engine) shares reference, no data duplication
