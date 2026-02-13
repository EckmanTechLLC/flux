# Phase 2 Planning Session

**Date:** 2026-02-12
**Session:** Phase 2 Planning
**Status:** Complete

---

## Goal

Plan Phase 2: Persistence & Recovery for Flux state engine.

---

## Context Review

**Phase 1 Status (Complete):**
- In-memory state engine using DashMap
- Events persisted in NATS JetStream (FLUX_EVENTS stream)
- WebSocket + HTTP APIs for state access
- State lost on restart (no snapshots)

**Phase 2 Requirements:**
- State survives restarts (snapshots + replay)
- Fast recovery (<10 seconds for 100k entities)
- Event replay from arbitrary points (time-travel, debugging)
- Production-ready durability

---

## Files Reviewed

1. `/FLUX-DESIGN.md` - State ownership model, architecture
2. `/docs/decisions/001-flux-state-engine-architecture.md` - Phase 1 architecture
3. `/docs/architecture.md` - Current implementation details
4. `/src/state/engine.rs` - Current in-memory state implementation

**Key findings:**
- DashMap already in use (lock-free concurrent reads)
- NATS JetStream persists events with sequence numbers
- State engine subscribes to flux.events.> via durable consumer
- No current snapshot or persistence mechanism

---

## Decisions Made

### ADR-002: Persistence and Recovery

**Created:** `/docs/decisions/002-persistence-and-recovery.md`

**Key decisions:**
1. **Snapshot format:** JSON (human-readable, good enough performance)
2. **Storage:** Filesystem at `/var/lib/flux/snapshots/` (simple, no external deps)
3. **Frequency:** Time-based, default 5 minutes (predictable recovery time)
4. **Atomicity:** Write to temp file, rename (POSIX atomic rename)
5. **Recovery:** Load snapshot + replay events since snapshot sequence

**Why these choices:**
- JSON: Human-readable, schema evolution, ops-friendly (vs binary)
- Filesystem: Simple, fast, easy backup (vs S3/database for Phase 2)
- Time-based: Predictable recovery time (vs event-count based)
- Atomic rename: Safe, no partial snapshots visible

**Out of scope (Phase 3+):**
- Online replay API (time-travel queries)
- Cloud storage backends (S3, GCS)
- Distributed snapshots (multi-node)
- Binary snapshot formats

---

## Phase 2 Tasks (Breakdown)

### Task 1: Snapshot Core (Small, 1-2 hours)
**Files:**
- `src/snapshot/mod.rs` - Snapshot struct, save/load
- `src/snapshot/tests.rs` - Unit tests

**Scope:**
- Snapshot struct (version, created_at, sequence, entities)
- Serialize to JSON (from DashMap entities)
- Deserialize from JSON (to HashMap)
- Save to file, load from file
- Manual snapshot only (no background loop yet)

**Tests:**
- Serialize/deserialize round-trip
- Save/load from filesystem
- Handle missing files gracefully

**Dependencies:** None

---

### Task 2: Sequence Number Tracking (Small, 1-2 hours)
**Files:**
- `src/state/engine.rs` - Add sequence tracking

**Scope:**
- Add `last_processed_sequence: AtomicU64` field to StateEngine
- Update `run_subscriber()` to extract and store NATS sequence
- Add `get_last_processed_sequence()` method
- Update `process_event()` signature to accept sequence

**Tests:**
- Verify sequence advances on event processing
- Verify get_last_processed_sequence() returns correct value

**Dependencies:** None

---

### Task 3: Snapshot Manager (Medium, 3-4 hours)
**Files:**
- `src/snapshot/manager.rs` - SnapshotManager struct
- `src/snapshot/config.rs` - SnapshotConfig struct

**Scope:**
- SnapshotManager with background snapshot loop
- Tokio interval timer (configurable minutes)
- Create snapshot, save to filesystem
- Cleanup old snapshots (keep last N)
- Configuration: interval, directory, keep_count
- Logging: snapshot created, cleanup

**Tests:**
- Verify snapshot created on interval
- Verify old snapshots cleaned up
- Verify snapshot naming (timestamp + sequence)

**Dependencies:** Task 1 (needs Snapshot), Task 2 (needs sequence)

---

### Task 4: Recovery on Startup (Medium, 4-5 hours)
**Files:**
- `src/main.rs` - Add recovery logic before subscriber
- `src/snapshot/recovery.rs` - Recovery logic

**Scope:**
- Find latest snapshot in directory
- Load snapshot into StateEngine (populate DashMap)
- Start NATS consumer from snapshot sequence + 1
- Handle no snapshot case (start from beginning)
- Logging: recovery progress, entities loaded
- Error handling: corrupt snapshot fallback

**Tests:**
- Integration test: Create snapshot, restart, verify state recovered
- Test no snapshot case (cold start)
- Test corrupt snapshot (fallback to earlier snapshot)

**Dependencies:** Task 1 (needs Snapshot), Task 2 (needs sequence), Task 3 (needs manager)

---

### Task 5: Compression & Atomicity (Small, 2-3 hours)
**Files:**
- `src/snapshot/mod.rs` - Add compression

**Scope:**
- Add gzip compression to save_to_file()
- Add gzip decompression to load_from_file()
- Atomic write: temp file (.tmp) + rename
- Fsync before rename
- Update snapshot filename: .json.gz extension

**Tests:**
- Verify compressed snapshots are smaller
- Verify atomic write (no partial files)
- Verify decompression works

**Dependencies:** Task 3 (compression integrated into manager)

---

### Task 6: Documentation & Config (Small, 1-2 hours)
**Files:**
- `docs/architecture.md` - Add persistence section
- `README.md` - Add snapshot configuration
- `config.toml` - Add snapshot settings

**Scope:**
- Document snapshot strategy in architecture.md
- Document recovery process in architecture.md
- Add snapshot config section to README
- Add default snapshot config to config.toml
- Update deployment section with disk requirements

**Tests:**
- N/A (documentation only)

**Dependencies:** Task 4 (docs after implementation)

---

## Task Dependencies (Visual)

```
Task 1: Snapshot Core (Small)
  ↓
Task 2: Sequence Tracking (Small)
  ↓
Task 3: Snapshot Manager (Medium)
  ↓
Task 4: Recovery on Startup (Medium)
  ↓
Task 5: Compression & Atomicity (Small)
  ↓
Task 6: Documentation & Config (Small)
```

**Total estimated time:** 12-18 hours

---

## Implementation Order

1. **Task 1** - Snapshot core (foundation)
2. **Task 2** - Sequence tracking (needed for snapshots)
3. **Task 3** - Snapshot manager (background loop)
4. **Task 4** - Recovery (load snapshots on startup)
5. **Task 5** - Compression (production-ready)
6. **Task 6** - Documentation (finalize)

**Rationale:**
- Bottom-up: Build foundation first (snapshot struct)
- Dependencies: Each task builds on previous
- Testing: Can test each task independently
- Incremental: Can stop after any task if needed

---

## Success Criteria

**Phase 2 Complete when:**
- [ ] Snapshots created automatically every 5 minutes
- [ ] State recovers from snapshot on restart (<10 sec)
- [ ] Integration test: Publish events → restart → state intact
- [ ] Snapshots are gzipped and atomic (no partial files)
- [ ] Documentation updated (architecture.md, README.md)
- [ ] Configuration added (config.toml)

**Testing:**
1. Run Flux, publish 10k events
2. Wait for snapshot (check /var/lib/flux/snapshots/)
3. Restart Flux
4. Verify state recovered (query entities, should have 10k)
5. Verify fast recovery (<10 sec)

---

## Out of Scope (Future Phases)

**Not in Phase 2:**
- ❌ Online replay API (POST /api/replay)
- ❌ Cloud storage backends (S3, GCS)
- ❌ Binary snapshot formats (MessagePack, etc)
- ❌ Incremental snapshots (delta encoding)
- ❌ Multi-node snapshot coordination
- ❌ Snapshot compression algorithms (zstd, etc)

**Why deferred:**
- Phase 2 focus: Basic persistence working
- Cloud storage: Adds external dependencies
- Online replay: Complex, requires concurrent state
- Incremental snapshots: Premature optimization

**Future phase candidates:**
- Phase 3: Online replay API, cloud storage
- Phase 4: Advanced compression, multi-node

---

## Files Created

1. `/docs/decisions/002-persistence-and-recovery.md` (ADR-002)
2. `/docs/sessions/2026-02-12-phase2-planning.md` (this file)

---

## Next Steps

1. **User approval:** Review ADR-002 and task breakdown
2. **Start Task 1:** Implement snapshot core (src/snapshot/mod.rs)
3. **Sequential implementation:** Follow task order 1-6
4. **Session notes:** Document each task implementation

---

## Notes

- ADR kept concise (~300 lines vs 2000)
- Task breakdown clear and focused (6 tasks)
- Dependencies explicit and minimal
- Estimated complexity provided (small/medium)
- Out of scope clearly documented
