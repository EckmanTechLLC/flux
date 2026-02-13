# ADR-002: Persistence and Recovery

**Status:** Proposed
**Date:** 2026-02-12
**Deciders:** Architecture Team

---

## Context

Phase 1 has in-memory state only (DashMap). State is lost on restart. NATS JetStream persists events, but we don't rebuild state from them on startup.

**Problems:**
- State lost on crashes/restarts (unacceptable for production)
- Full event replay slow for large histories (minutes to hours)
- No point-in-time recovery (can't debug historical state)
- No disaster recovery capability

**Requirements:**
- State survives restarts (snapshots + replay)
- Fast recovery (<10 seconds for 100k entities)
- Event replay from arbitrary points (time-travel, debugging)
- Production-ready durability (no data loss)

---

## Decision

### Snapshot + Incremental Replay Strategy

**Core approach:**
```
Startup:
  1. Load latest snapshot (if exists) → in-memory state
  2. Replay events since snapshot → catch up
  3. Resume normal operation

Runtime:
  1. Process events → update state
  2. Every N minutes → save snapshot
  3. Continue processing
```

**Key principle:** Snapshots are point-in-time checkpoints. Events are source of truth.

---

## Snapshot Design

### Format: JSON

**Structure:**
```json
{
  "snapshot_version": "1",
  "created_at": "2026-02-12T10:30:00Z",
  "sequence_number": 12345,
  "entity_count": 1000,
  "entities": {
    "entity_id_1": {
      "id": "entity_id_1",
      "properties": {"temp": 22.5, "status": "active"},
      "last_updated": "2026-02-12T10:29:55Z"
    },
    "entity_id_2": { ... }
  }
}
```

**Why JSON:**
- Human-readable (debugging, inspection)
- Schema evolution (add fields without breaking)
- Language-agnostic (any client can read)
- Good-enough performance (gzip compression)

**Not binary:**
- JSON is "good enough" for 100k-1M entities (10-100MB compressed)
- Premature optimization to use binary format
- JSON readability is valuable for operations

### Storage: Filesystem

**Location:** `/var/lib/flux/snapshots/`

**Naming:** `snapshot-{timestamp}-seq{sequence}.json.gz`

Example: `snapshot-20260212-103000-seq12345.json.gz`

**Why filesystem:**
- Simple (no external dependencies)
- Fast (local disk)
- Easy backup (rsync, tar, cloud sync)
- Easy inspection (gunzip, jq)

**Not S3/database:**
- Phase 2 scope: single instance
- Filesystem sufficient for now
- Future: Abstract storage interface for S3/etc

### Snapshot Frequency

**Trigger:** Time-based (configurable, default: 5 minutes)

**Why time-based:**
- Predictable recovery time (max 5 min replay)
- Simple implementation (tokio::time::interval)
- Low overhead (5 min allows 100k+ events between snapshots)

**Not event-count based:**
- Unpredictable recovery time (spiky traffic)
- Complex threshold tuning

**Configuration:**
```toml
[snapshot]
enabled = true
interval_minutes = 5
directory = "/var/lib/flux/snapshots"
keep_count = 10  # Retain last 10 snapshots
```

### Atomicity

**Approach:** Write to temp file, rename

```
1. Serialize state to memory buffer
2. Write buffer to snapshot-{timestamp}-seq{seq}.tmp.gz
3. Fsync temp file
4. Rename to snapshot-{timestamp}-seq{seq}.json.gz (atomic)
5. Delete old snapshots (keep last N)
```

**Why rename:**
- Atomic on POSIX filesystems
- No partial snapshots visible
- Safe concurrent reads

---

## Recovery Design

### Startup Sequence

```
1. Initialize StateEngine (empty DashMap)

2. Find latest snapshot:
   - List /var/lib/flux/snapshots/
   - Sort by timestamp descending
   - Select first valid snapshot

3. If snapshot exists:
   a. Load snapshot into DashMap
   b. Get snapshot sequence number (N)
   c. Log: "Loaded snapshot seq={N}, entities={count}"

4. Connect to NATS JetStream:
   a. Create/get consumer on FLUX_EVENTS
   b. Start from sequence number N+1 (after snapshot)
   c. Replay all events since snapshot

5. Once caught up (no more historical events):
   a. Log: "Recovery complete, entities={count}"
   b. Switch to real-time mode

6. Start snapshot interval timer
```

### Replay Mechanism

**NATS JetStream consumer:**
```rust
// Phase 1: From beginning (StartSequence 0)
let consumer = stream.get_or_create_consumer(
    "flux-state-engine",
    Config {
        deliver_policy: DeliverPolicy::All,  // From start
        ...
    }
).await?;

// Phase 2: From snapshot sequence (StartSequence N+1)
let consumer = stream.get_or_create_consumer(
    "flux-state-engine",
    Config {
        deliver_policy: DeliverPolicy::ByStartSequence {
            start_sequence: snapshot_seq + 1
        },
        ...
    }
).await?;
```

**Why JetStream sequence numbers:**
- Built-in, guaranteed monotonic
- Survives NATS restarts
- No custom bookkeeping needed

### Fast Recovery Performance

**Target:** <10 seconds for 100k entities

**Snapshot load:** ~2 seconds (100MB gzipped JSON)
**Event replay:** ~5 minutes of events = ~50k events @ 10k/sec
**Replay time:** ~5 seconds @ 10k events/sec processing
**Total:** ~7 seconds (well under target)

---

## Event Replay API

### Query Replay (Future, not Phase 2)

**Use case:** Debugging, time-travel, analysis

**API endpoint:**
```
POST /api/replay
{
  "from_sequence": 1000,
  "to_sequence": 2000,  // Optional (default: latest)
  "callback_url": "https://client.example.com/replay"  // Optional
}
```

**Response:**
```json
{
  "replay_id": "uuid",
  "status": "running",
  "progress": {"current": 1050, "total": 1000}
}
```

**Not in Phase 2:**
- Complex to implement safely (concurrent state)
- Phase 2 focus: basic persistence/recovery
- Phase 3 feature

---

## Implementation Details

### Snapshot Structure

**Code additions:**
```rust
// src/snapshot/mod.rs
pub struct Snapshot {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub sequence_number: u64,
    pub entities: HashMap<String, Entity>,
}

impl Snapshot {
    pub fn from_state_engine(engine: &StateEngine, seq: u64) -> Self { ... }
    pub fn save_to_file(&self, path: &Path) -> Result<()> { ... }
    pub fn load_from_file(path: &Path) -> Result<Self> { ... }
}

// src/snapshot/manager.rs
pub struct SnapshotManager {
    config: SnapshotConfig,
    state_engine: Arc<StateEngine>,
}

impl SnapshotManager {
    pub async fn run_snapshot_loop(&self) -> Result<()> { ... }
    pub fn load_latest_snapshot(&self) -> Result<Option<(Snapshot, u64)>> { ... }
    pub fn cleanup_old_snapshots(&self) -> Result<()> { ... }
}
```

### Sequence Number Tracking

**Where:** NATS message metadata

```rust
// During event processing
while let Some(msg) = messages.next().await {
    let sequence = msg.info()?.stream_sequence;  // NATS sequence
    let event = serde_json::from_slice::<FluxEvent>(&msg.payload)?;

    state_engine.process_event(&event);
    state_engine.update_last_processed_sequence(sequence);

    msg.ack().await?;
}
```

**Why message metadata:**
- NATS provides sequence per message
- No custom sequence tracking
- Guaranteed monotonic

### Snapshot Timing

**Background task:**
```rust
pub async fn run_snapshot_loop(&self) -> Result<()> {
    let mut interval = tokio::time::interval(
        Duration::from_secs(self.config.interval_minutes * 60)
    );

    loop {
        interval.tick().await;

        let seq = self.state_engine.get_last_processed_sequence();
        let snapshot = Snapshot::from_state_engine(&self.state_engine, seq);

        snapshot.save_to_file(&self.snapshot_path(seq))?;
        self.cleanup_old_snapshots()?;

        info!("Snapshot saved: seq={}, entities={}", seq, snapshot.entities.len());
    }
}
```

---

## Configuration

**Add to config.toml:**
```toml
[snapshot]
enabled = true
interval_minutes = 5
directory = "/var/lib/flux/snapshots"
keep_count = 10

[nats]
url = "nats://localhost:4222"
stream_name = "FLUX_EVENTS"

[recovery]
auto_recover = true  # Load snapshot on startup
```

---

## Consequences

### Positive

- ✅ State survives restarts (production-ready)
- ✅ Fast recovery (<10 sec for 100k entities)
- ✅ Simple implementation (filesystem, JSON)
- ✅ Human-readable snapshots (ops-friendly)
- ✅ Event replay capability (debugging)
- ✅ No external dependencies (no S3/database)

### Negative

- ⚠️ Filesystem-only (no cloud storage yet) → *Phase 3: abstract storage*
- ⚠️ JSON overhead (~2x binary size) → *acceptable trade-off for readability*
- ⚠️ No online replay API (only startup recovery) → *Phase 3 feature*
- ⚠️ Snapshot creates pause (~100ms for 100k entities) → *acceptable, happens every 5 min*

### Neutral

- Snapshots increase disk usage (~100MB per snapshot, 10 snapshots = 1GB)
- Recovery time depends on event rate (5 min @ 10k/sec = ~7 sec recovery)

---

## Phase 2 Tasks (Prioritized)

### Task 1: Snapshot Core (Small)
- `src/snapshot/mod.rs` - Snapshot struct, serialize/deserialize
- `src/snapshot/tests.rs` - Unit tests
- Support: Save/load snapshot to/from filesystem
- No background loop yet (manual snapshot for testing)

### Task 2: Sequence Number Tracking (Small)
- `src/state/engine.rs` - Add last_processed_sequence field
- Update run_subscriber() to track NATS sequence
- Add get_last_processed_sequence() method
- Tests: Verify sequence advances

### Task 3: Snapshot Manager (Medium)
- `src/snapshot/manager.rs` - SnapshotManager struct
- Background snapshot loop (tokio interval)
- Cleanup old snapshots (keep last N)
- Config: interval, directory, keep_count
- Integration test: Verify snapshot creation

### Task 4: Recovery on Startup (Medium)
- `src/main.rs` - Recovery logic before starting subscriber
- Load latest snapshot → populate StateEngine
- Start NATS consumer from snapshot sequence + 1
- Logging: recovery progress
- Integration test: Restart with snapshot

### Task 5: Compression & Atomicity (Small)
- Add gzip compression to snapshot save/load
- Atomic write (temp file + rename)
- Error handling: corrupt snapshot fallback
- Tests: Verify atomicity

### Task 6: Documentation & Config (Small)
- Update `/docs/architecture.md` - Add persistence section
- Update `/README.md` - Add snapshot config
- Update `config.toml` - Add snapshot settings
- Document recovery process

**Dependencies:**
- Task 2 → Task 1 (need sequence before snapshot)
- Task 3 → Task 1, Task 2 (manager needs snapshot + sequence)
- Task 4 → Task 1, Task 2, Task 3 (recovery needs all)
- Task 5 → Task 3 (compression part of manager)
- Task 6 → Task 4 (docs after implementation)

**Estimated Complexity:**
- Small: 1-2 hours, single file, focused scope
- Medium: 3-5 hours, multiple files, integration

---

## References

- Phase 1: `/docs/decisions/001-flux-state-engine-architecture.md`
- NATS JetStream Consumer: https://docs.nats.io/nats-concepts/jetstream/consumers
- DashMap: https://docs.rs/dashmap
- Atomic file operations: POSIX rename(2)
