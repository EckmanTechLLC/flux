# Phase 2 Task 1: Snapshot Core

**Date:** 2026-02-12
**Task:** Implement snapshot serialization/deserialization (no background loop)
**Status:** Complete ✅

---

## Goal

Create core snapshot functionality for manually saving and loading StateEngine state to/from filesystem.

---

## Implementation

### Files Created

1. **`/src/snapshot/mod.rs`** - Snapshot core
   - `Snapshot` struct (version, created_at, sequence_number, entities)
   - `from_state_engine()` - Create snapshot from StateEngine DashMap
   - `to_hashmap()` - Convert snapshot entities to HashMap for loading
   - `save_to_file()` - Write JSON to filesystem (no compression yet)
   - `load_from_file()` - Read JSON from filesystem
   - `entity_count()` - Helper for logging (computes entities.len())

2. **`/src/snapshot/tests.rs`** - Unit tests (7 tests)
   - `test_snapshot_serialize_deserialize_roundtrip` - JSON round-trip
   - `test_snapshot_save_and_load` - Filesystem save/load
   - `test_load_missing_file` - Error handling for missing files
   - `test_load_invalid_json` - Error handling for corrupt JSON
   - `test_snapshot_from_state_engine` - Create snapshot from StateEngine
   - `test_snapshot_to_hashmap` - Convert snapshot to HashMap
   - `test_snapshot_entity_count` - Helper method

### Files Modified

1. **`/src/lib.rs`**
   - Added `pub mod snapshot;` to expose snapshot module

---

## Snapshot Format

**JSON structure (matching ADR-002 spec):**
```json
{
  "snapshot_version": "1",
  "created_at": "2026-02-12T10:30:00Z",
  "sequence_number": 12345,
  "entities": {
    "entity_id_1": {
      "id": "entity_id_1",
      "properties": {
        "temp": 22.5,
        "status": "active"
      },
      "last_updated": "2026-02-12T10:29:55Z"
    }
  }
}
```

**Note:** `entity_count` field omitted from struct (computed dynamically via `entity_count()` method when needed).

---

## Design Decisions

### 1. Snapshot Struct
```rust
pub struct Snapshot {
    pub snapshot_version: String,     // "1" for schema evolution
    pub created_at: DateTime<Utc>,    // Snapshot timestamp
    pub sequence_number: u64,         // NATS sequence at snapshot time
    pub entities: HashMap<String, Entity>,  // All entities (id -> Entity)
}
```

**Why HashMap for entities:**
- Direct lookup by entity_id
- Natural JSON object representation
- Easy to load into StateEngine DashMap

### 2. from_state_engine() Implementation
- Calls `StateEngine::get_all_entities()` to get Vec<Entity>
- Converts to HashMap<String, Entity> using entity.id as key
- Captures current timestamp and provided sequence number

### 3. save_to_file() - Simple Write
- Uses `serde_json::to_string_pretty()` for human-readable JSON
- Direct `fs::write()` (no compression or atomic write yet)
- Task 5 will add gzip compression and atomic rename

### 4. Error Handling
- Uses `anyhow::Result` for error propagation
- Contextual errors: "Failed to serialize", "Failed to write", etc.
- Tests verify error cases (missing file, invalid JSON)

---

## Tests

**All 7 tests passing:**

```
test snapshot::tests::test_load_invalid_json ... ok
test snapshot::tests::test_snapshot_entity_count ... ok
test snapshot::tests::test_load_missing_file ... ok
test snapshot::tests::test_snapshot_to_hashmap ... ok
test snapshot::tests::test_snapshot_serialize_deserialize_roundtrip ... ok
test snapshot::tests::test_snapshot_save_and_load ... ok
test snapshot::tests::test_snapshot_from_state_engine ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured
```

**Run tests:**
```bash
cargo test --lib snapshot::tests
```

---

## What's NOT Included (Future Tasks)

- ❌ Compression (gzip) - Task 5
- ❌ Atomic writes (temp + rename) - Task 5
- ❌ Background snapshot loop - Task 3
- ❌ Sequence number tracking in StateEngine - Task 2
- ❌ Snapshot manager - Task 3
- ❌ Recovery on startup - Task 4

---

## Usage Example (Manual Snapshot)

```rust
use flux::snapshot::Snapshot;
use flux::state::StateEngine;
use std::path::Path;

// Create state engine with data
let engine = StateEngine::new();
engine.update_property("sensor_1", "temp", json!(22.5));

// Create snapshot
let snapshot = Snapshot::from_state_engine(&engine, 1000);
println!("Snapshot has {} entities", snapshot.entity_count());

// Save to file
let path = Path::new("/tmp/snapshot-test.json");
snapshot.save_to_file(path)?;

// Load from file
let loaded = Snapshot::load_from_file(path)?;
let entities = loaded.to_hashmap();
println!("Loaded {} entities", entities.len());
```

---

## Next Steps

**Task 2: Sequence Number Tracking**
- Add `last_processed_sequence: AtomicU64` to StateEngine
- Update `run_subscriber()` to track NATS sequence from message metadata
- Add `get_last_processed_sequence()` method

**Dependencies for Task 2:**
- Needs: Task 1 complete ✅
- Blocks: Task 3 (manager needs sequence to create snapshots)

---

## Checklist

- [x] Read CLAUDE.md
- [x] Read ADR-002 (persistence design)
- [x] Read current state engine code
- [x] Read phase 2 planning doc
- [x] Created `/src/snapshot/mod.rs`
- [x] Created `/src/snapshot/tests.rs`
- [x] Updated `/src/lib.rs`
- [x] All tests passing (7/7)
- [x] Session notes documented

---

## Summary

Task 1 complete. Snapshot core implemented with JSON serialization, filesystem save/load, and comprehensive tests. Ready for Task 2 (sequence tracking).
