# Phase 2 Testing: Persistence & Recovery

**Date:** 2026-02-13
**Session:** phase2-testing
**Goal:** Comprehensive testing of Phase 2 snapshot and recovery features

---

## Test Configuration

**Snapshot settings** (from config.toml):
- Interval: 5 minutes
- Directory: `/var/lib/flux/snapshots`
- Keep count: 10 snapshots
- Auto-recovery: enabled

**Expected behavior:**
- Snapshots created automatically every 5 minutes
- Compressed format: `.json.gz`
- Atomic writes (no `.tmp` files after completion)
- Recovery loads latest snapshot + replays events since snapshot

---

## Test Steps

### 1. Clean Environment Setup

**Objective:** Start with clean state to verify all features.

**Commands:**
```bash
# Stop Flux if running
docker compose down

# Clean snapshot directory
sudo rm -rf /var/lib/flux/snapshots/*
ls -lh /var/lib/flux/snapshots/  # Should be empty

# Clean NATS data (fresh event stream)
sudo rm -rf data/nats/*

# Verify clean state
docker compose ps  # Should show nothing running
```

**Expected result:** Clean environment, no running containers, empty snapshot dir.

---

### 2. Cold Start (No Snapshot)

**Objective:** Verify Flux starts correctly without existing snapshots.

**Commands:**
```bash
# Start Flux
docker compose up -d

# Watch logs for startup
docker compose logs -f flux

# Look for:
# - "No snapshot found, starting from beginning"
# - "Connected to NATS"
# - "State engine started"
# - "HTTP server listening on 0.0.0.0:3000"
```

**Expected result:** Clean startup, no errors, listens on port 3000.

Press Ctrl+C to stop following logs when startup complete.

---

### 3. Publish Initial Events

**Objective:** Build initial state with known entities.

**Commands:**
```bash
cd examples/python

# Publish 10 events across 3 entities
python publish.py --count 10

# Query current state
python query.py

# Expected: 3 entities (sensor-001, sensor-002, sensor-003)
# with temperature and status properties
```

**Expected result:** 10 events published, 3 entities in state.

**Save output for later comparison.**

---

### 4. Wait for First Snapshot

**Objective:** Verify automatic snapshot creation.

**Commands:**
```bash
# Check current time
date

# Wait 5+ minutes (snapshot interval)
# Monitor logs while waiting:
docker compose logs -f flux

# Look for:
# - "Creating snapshot at sequence=X"
# - "Snapshot saved: /var/lib/flux/snapshots/snapshot-TIMESTAMP-seqN.json.gz"
# - "Snapshot complete: entities=3, size=X bytes"
```

**While waiting (in separate terminal):**
```bash
# Watch snapshot directory
watch -n 10 'ls -lh /var/lib/flux/snapshots/'

# When snapshot appears:
# - Verify .json.gz extension
# - Verify NO .tmp files (atomic write completed)
# - Note timestamp and sequence number
```

**Expected result:** Snapshot file created after 5 minutes, no temp files.

---

### 5. Inspect Snapshot Content

**Objective:** Verify snapshot structure and compression.

**Commands:**
```bash
# Find latest snapshot
SNAPSHOT=$(ls -t /var/lib/flux/snapshots/*.json.gz | head -1)
echo "Latest snapshot: $SNAPSHOT"

# Check compressed size
ls -lh $SNAPSHOT

# Uncompress and view structure
zcat $SNAPSHOT | jq '.' | head -50

# Verify fields:
# - snapshot_version: "1"
# - created_at: ISO 8601 timestamp
# - sequence_number: matches NATS sequence
# - entity_count: 3
# - entities: object with entity IDs as keys

# Check full entity structure
zcat $SNAPSHOT | jq '.entities."sensor-001"'

# Verify entity fields:
# - id: "sensor-001"
# - properties: {"temperature": 22.5, "status": "active"}
# - last_updated: ISO 8601 timestamp

# Check compression ratio
UNCOMPRESSED=$(zcat $SNAPSHOT | wc -c)
COMPRESSED=$(stat -c%s $SNAPSHOT)
echo "Uncompressed: $UNCOMPRESSED bytes"
echo "Compressed: $COMPRESSED bytes"
echo "Ratio: $(echo "scale=2; $COMPRESSED / $UNCOMPRESSED * 100" | bc)%"
```

**Expected result:**
- Valid JSON structure
- All required fields present
- Compression ratio ~30-50% (JSON compresses well)
- 3 entities with correct properties

---

### 6. Test Recovery (Cold Restart)

**Objective:** Verify snapshot loading and state restoration.

**Commands:**
```bash
# Stop Flux (state lost from memory)
docker compose down

# Verify snapshot still exists
ls -lh /var/lib/flux/snapshots/

# Start Flux again
docker compose up -d

# Watch recovery logs
docker compose logs -f flux

# Look for:
# - "Found snapshot: /var/lib/flux/snapshots/snapshot-TIMESTAMP-seqN.json.gz"
# - "Loading snapshot from sequence=N"
# - "Snapshot loaded: entities=3"
# - "Replaying events from sequence=N+1"
# - "Recovery complete"
# - "State engine started"
```

**Expected result:**
- Snapshot loaded successfully
- State restored (3 entities)
- No errors during recovery

Press Ctrl+C to stop following logs.

---

### 7. Verify State After Recovery

**Objective:** Confirm state matches pre-restart state.

**Commands:**
```bash
cd examples/python

# Query state after recovery
python query.py

# Compare with saved output from Step 3
# Should be IDENTICAL (same entities, same properties)
```

**Expected result:** State identical to pre-restart state.

---

### 8. Publish Events After Recovery

**Objective:** Verify event processing continues correctly post-recovery.

**Commands:**
```bash
# Publish 5 more events
python publish.py --count 5

# Query updated state
python query.py

# Expected: Still 3 entities, but properties updated
# (temperature values changed, timestamps newer)

# Subscribe to real-time updates
python subscribe.py

# (Leave running, open new terminal for next step)
```

**Expected result:**
- New events processed correctly
- State updated (newer timestamps)
- Real-time subscriptions working

---

### 9. Wait for Second Snapshot

**Objective:** Verify snapshot includes post-recovery events.

**Commands:**
```bash
# In separate terminal, watch snapshots
watch -n 10 'ls -lt /var/lib/flux/snapshots/ | head -5'

# Wait 5+ minutes for next snapshot interval

# When second snapshot appears:
SNAPSHOT2=$(ls -t /var/lib/flux/snapshots/*.json.gz | head -1)

# Compare sequence numbers
SNAP1_SEQ=$(ls -t /var/lib/flux/snapshots/*.json.gz | tail -1 | grep -oP 'seq\K[0-9]+')
SNAP2_SEQ=$(ls -t /var/lib/flux/snapshots/*.json.gz | head -1 | grep -oP 'seq\K[0-9]+')

echo "First snapshot sequence: $SNAP1_SEQ"
echo "Second snapshot sequence: $SNAP2_SEQ"
echo "Events between snapshots: $((SNAP2_SEQ - SNAP1_SEQ))"

# Verify second snapshot includes post-recovery state
zcat $SNAPSHOT2 | jq '.entities."sensor-001".last_updated'

# Should show newer timestamp than first snapshot
```

**Expected result:**
- Second snapshot created
- Sequence number advanced (SNAP2_SEQ > SNAP1_SEQ)
- State includes post-recovery events

---

### 10. Test Event Replay from Snapshot

**Objective:** Verify recovery skips events before snapshot.

**Commands:**
```bash
# Check NATS stream info
docker compose exec nats nats stream info FLUX_EVENTS

# Note:
# - First sequence: 1
# - Last sequence: X (total events published)
# - Messages: X (all events persisted)

# Check latest snapshot sequence
LATEST_SNAP=$(ls -t /var/lib/flux/snapshots/*.json.gz | head -1)
SNAP_SEQ=$(echo $LATEST_SNAP | grep -oP 'seq\K[0-9]+')
echo "Latest snapshot at sequence: $SNAP_SEQ"

# Restart Flux (should replay from SNAP_SEQ + 1)
docker compose restart flux

# Watch recovery logs
docker compose logs -f flux | grep -E "snapshot|replay|sequence"

# Look for:
# - "Loading snapshot from sequence=$SNAP_SEQ"
# - "Replaying events from sequence=$((SNAP_SEQ + 1))"
# - Should NOT replay sequences 1 through $SNAP_SEQ

# Verify state correct after replay
cd examples/python && python query.py
```

**Expected result:**
- Recovery starts from snapshot sequence + 1
- Only events AFTER snapshot replayed
- Final state correct (matches pre-restart)

---

### 11. Test Snapshot Cleanup (Keep Count)

**Objective:** Verify old snapshots deleted (keep_count = 10).

**Commands:**
```bash
# Check current snapshot count
ls /var/lib/flux/snapshots/ | wc -l

# If < 10 snapshots exist, need to trigger more
# (Either wait multiple intervals OR reduce interval temporarily)

# For faster testing, you can:
# 1. Edit config.toml: interval_minutes = 1
# 2. Restart Flux: docker compose restart flux
# 3. Wait for 11+ snapshots to be created

# Once 11+ snapshots created, verify cleanup
watch -n 10 'ls -lt /var/lib/flux/snapshots/ | wc -l'

# Should stabilize at keep_count (10)
# Oldest snapshots deleted automatically
```

**Expected result:**
- Snapshot count never exceeds keep_count (10)
- Oldest snapshots deleted when limit reached

---

### 12. Test Atomic Writes (No Corruption)

**Objective:** Verify no partial snapshots (.tmp files) remain.

**Commands:**
```bash
# Check for any .tmp files (should be none after writes complete)
find /var/lib/flux/snapshots/ -name "*.tmp*"

# Expected: No output (no temp files)

# During snapshot creation (tight timing window):
# Open two terminals:

# Terminal 1: Watch for .tmp files during snapshot
watch -n 0.5 'ls -lh /var/lib/flux/snapshots/*.tmp* 2>/dev/null || echo "No temp files"'

# Terminal 2: Trigger snapshot by waiting for interval
docker compose logs -f flux | grep snapshot

# Observe:
# - .tmp file appears briefly (while writing)
# - .tmp file disappears (renamed to .json.gz atomically)
# - Final .json.gz file appears
```

**Expected result:**
- Temp files exist only during write
- Atomic rename completes successfully
- No orphaned .tmp files

---

### 13. Test Compression Efficiency

**Objective:** Measure compression ratio for different state sizes.

**Commands:**
```bash
# Current state (3 entities, small)
SNAP_SMALL=$(ls -t /var/lib/flux/snapshots/*.json.gz | head -1)
SIZE_SMALL=$(stat -c%s $SNAP_SMALL)
ENTITIES_SMALL=$(zcat $SNAP_SMALL | jq '.entity_count')

echo "Small state: $ENTITIES_SMALL entities, $SIZE_SMALL bytes"

# Publish many more events (100+ entities)
cd examples/python
for i in {1..100}; do
  python publish.py --count 10
  sleep 1
done

# Wait for next snapshot
sleep 300  # 5 minutes

# Check new snapshot size
SNAP_LARGE=$(ls -t /var/lib/flux/snapshots/*.json.gz | head -1)
SIZE_LARGE=$(stat -c%s $SNAP_LARGE)
ENTITIES_LARGE=$(zcat $SNAP_LARGE | jq '.entity_count')

echo "Large state: $ENTITIES_LARGE entities, $SIZE_LARGE bytes"

# Calculate compression ratio
zcat $SNAP_LARGE | wc -c  # Uncompressed
echo "Compressed: $SIZE_LARGE bytes"
```

**Expected result:**
- Compression ratio ~30-50%
- Scales linearly with entity count
- Large snapshots (~100k entities) still < 100MB compressed

---

### 14. Final State Verification

**Objective:** Confirm end-to-end persistence across multiple restart cycles.

**Commands:**
```bash
# Record final state
cd examples/python
python query.py > /tmp/state_final.txt

# Restart Flux 3 times
for i in {1..3}; do
  echo "=== Restart cycle $i ==="
  docker compose restart flux
  sleep 10  # Wait for recovery

  # Verify state identical after each restart
  python query.py > /tmp/state_restart_$i.txt
  diff /tmp/state_final.txt /tmp/state_restart_$i.txt

  if [ $? -eq 0 ]; then
    echo "✓ State identical after restart $i"
  else
    echo "✗ State DIFFERS after restart $i"
  fi
done
```

**Expected result:**
- State identical across all restart cycles
- No data loss
- Consistent recovery behavior

---

## Test Summary Template

After completing all steps, report results:

```
### Test Results: Phase 2 Persistence & Recovery

**Date:** 2026-02-13

**Environment:**
- Flux version: [from Cargo.toml]
- NATS version: [from docker-compose.yml]
- Platform: Linux x86_64

**Tests Passed:** [X/14]

**Detailed Results:**
1. Clean Environment Setup: [PASS/FAIL]
2. Cold Start (No Snapshot): [PASS/FAIL]
3. Publish Initial Events: [PASS/FAIL]
4. Wait for First Snapshot: [PASS/FAIL]
5. Inspect Snapshot Content: [PASS/FAIL]
6. Test Recovery (Cold Restart): [PASS/FAIL]
7. Verify State After Recovery: [PASS/FAIL]
8. Publish Events After Recovery: [PASS/FAIL]
9. Wait for Second Snapshot: [PASS/FAIL]
10. Test Event Replay from Snapshot: [PASS/FAIL]
11. Test Snapshot Cleanup: [PASS/FAIL]
12. Test Atomic Writes: [PASS/FAIL]
13. Test Compression Efficiency: [PASS/FAIL]
14. Final State Verification: [PASS/FAIL]

**Issues Found:**
- [List any failures, unexpected behavior, or errors]

**Performance Metrics:**
- Snapshot creation time: X seconds
- Snapshot size (3 entities): X KB
- Snapshot size (100+ entities): X MB
- Compression ratio: X%
- Recovery time: X seconds
- Events replayed during recovery: X

**Notes:**
- [Any additional observations]
```

---

## Next Steps After Testing

If all tests pass:
1. Commit Phase 2 changes to git
2. Update CLAUDE.md (mark Phase 2 complete, add test date)
3. Deploy to test instance (etl-bot)
4. Update ADR-002 status to "Accepted"

If tests fail:
1. Document failure details in this session note
2. Create bug fix tasks
3. Re-test after fixes

---

## Files Modified During Phase 2

**Core Implementation:**
- `src/snapshot/mod.rs` - Snapshot struct, serialize/deserialize
- `src/snapshot/manager.rs` - SnapshotManager, background loop, cleanup
- `src/state/engine.rs` - Sequence tracking, snapshot creation
- `src/main.rs` - Recovery integration on startup

**Configuration:**
- `config.toml` - Snapshot settings

**Documentation:**
- `docs/decisions/002-persistence-and-recovery.md`
- `docs/sessions/2026-02-12-phase2-*.md` (Tasks 1-6)
- `docs/sessions/2026-02-13-phase2-testing.md` (this file)

**Tests:**
- `src/snapshot/tests.rs` - Unit tests
- `src/state/tests.rs` - Updated with sequence tracking tests

---

**End of testing guide. Execute steps 1-14 and report results.**
