# Flux State Model

**Status:** Phase 1 Implementation
**Last Updated:** 2026-02-11

---

## Overview

Flux maintains world state as **generic entities with properties**. State is derived from immutable events and stored in-memory for fast access.

**Key principle:** Flux is domain-agnostic. The state model has no built-in entity types or schemas.

---

## Entity Structure

```rust
Entity {
    id: string,
    properties: Map<string, value>,
    last_updated: timestamp
}
```

**Fields:**

- **`id`** - Unique entity identifier (e.g., "sensor-01", "agent-42")
  - Application-defined, no format requirements
  - Must be unique within Flux instance
  - Used for queries and subscriptions

- **`properties`** - Key-value map of entity properties
  - Keys: String property names
  - Values: Any valid JSON value (string, number, boolean, array, object, null)
  - No schema validation
  - Application defines property semantics

- **`last_updated`** - ISO 8601 timestamp of last property change
  - UTC timezone
  - Updated automatically when any property changes
  - Used for staleness detection

**Example entity:**

```json
{
  "id": "sensor-01",
  "properties": {
    "temperature": 22.5,
    "humidity": 45.0,
    "unit": "celsius",
    "status": "active",
    "location": {
      "room": "lab-A",
      "floor": 2
    }
  },
  "last_updated": "2026-02-11T10:30:45.123Z"
}
```

---

## Event-to-State Derivation

State is derived from event payloads. Events must include:

**Required payload structure:**
```json
{
  "entity_id": "sensor-01",
  "properties": {
    "temperature": 22.5,
    "humidity": 45.0
  }
}
```

**Derivation rules:**

1. **Extract entity_id** - Identifies which entity to update
   - Must be a string
   - Creates entity if it doesn't exist
   - Skips event if missing

2. **Extract properties** - Key-value pairs to update
   - Must be a JSON object
   - Each key becomes/updates an entity property
   - Skips event if missing

3. **Apply updates** - For each property in the event:
   - Set `entity.properties[key] = value`
   - Overwrite existing values
   - Create new properties if not present
   - Update `entity.last_updated` to current time

4. **Broadcast changes** - For each property updated:
   - Create StateUpdate message
   - Send to all subscribers
   - Include old and new values

**Example event processing:**

```
Event payload:
{
  "entity_id": "sensor-01",
  "properties": {
    "temperature": 23.0,
    "status": "active"
  }
}

Resulting state change:
- entity.properties["temperature"] = 23.0
- entity.properties["status"] = "active"
- entity.last_updated = <current time>

StateUpdate broadcasts (2 messages):
1. { entity_id: "sensor-01", property: "temperature", old_value: 22.5, new_value: 23.0 }
2. { entity_id: "sensor-01", property: "status", old_value: null, new_value: "active" }
```

---

## State Updates

When entity properties change, Flux broadcasts **StateUpdate** messages to subscribers.

**StateUpdate structure:**

```json
{
  "entity_id": "sensor-01",
  "property": "temperature",
  "old_value": 22.5,
  "new_value": 23.0,
  "timestamp": "2026-02-11T10:31:00.456Z"
}
```

**Fields:**

- **`entity_id`** - Which entity changed
- **`property`** - Which property changed
- **`old_value`** - Previous value (null if new property)
- **`new_value`** - Current value
- **`timestamp`** - When the change occurred (Flux time, not event time)

**Key characteristics:**

- Property-level granularity (not entity-level)
- One StateUpdate per property changed
- Subscribers see state changes, not raw events
- Timestamp reflects when Flux processed the event

---

## lastUpdated Semantics

The `last_updated` field tracks when an entity's state changed.

**Update rules:**

- Updated whenever any property changes
- Set to current Flux time (not event timestamp)
- Same timestamp for all properties in a single event
- Used to detect stale state

**Use cases:**

- **Staleness detection:** Check if entity hasn't updated recently
- **Cache invalidation:** Compare with cached timestamp
- **Ordering:** Determine most recent entity changes
- **Monitoring:** Track update frequency

**Example:**

```json
{
  "id": "sensor-01",
  "properties": { "temp": 22.5 },
  "last_updated": "2026-02-11T10:30:00.000Z"
}
```

If no updates for 5 minutes, application can mark sensor as "stale" or "offline".

---

## Property Mutation Semantics

**Overwrite behavior:**
- Properties are **replaced**, not merged
- Setting `{"status": "active"}` overwrites entire status value
- For nested objects, entire object is replaced

**Example:**

```
Initial state:
{
  "location": {"room": "A", "floor": 1}
}

Event:
{
  "properties": {
    "location": {"room": "B"}
  }
}

Result:
{
  "location": {"room": "B"}  // floor is lost
}
```

**Best practice:** Include all fields when updating objects, or use separate properties.

**Property deletion:**
- Properties persist until overwritten
- No explicit delete operation in Phase 1
- Set to `null` to mark as cleared

---

## State Persistence

**Phase 1: Ephemeral state**
- State is in-memory only
- Lost on Flux restart
- Rebuilt by replaying events from NATS

**Future phases:**
- Periodic snapshots to disk
- Fast recovery (snapshot + recent events)
- State survives restarts

---

## Domain Examples

Flux is domain-agnostic. Applications define entity semantics.

**IoT sensors:**
```json
{
  "id": "temp-sensor-01",
  "properties": {
    "temperature": 22.5,
    "unit": "celsius",
    "battery": 85,
    "status": "online"
  }
}
```

**Multi-agent systems:**
```json
{
  "id": "agent-42",
  "properties": {
    "status": "idle",
    "location": "room-A",
    "task": null,
    "observations": ["door open", "light on"]
  }
}
```

**Game state:**
```json
{
  "id": "player-007",
  "properties": {
    "health": 85,
    "position": {"x": 100, "y": 200},
    "inventory": ["sword", "potion"],
    "level": 5
  }
}
```

**SCADA equipment:**
```json
{
  "id": "pump-12",
  "properties": {
    "flow_rate": 45.2,
    "pressure": 3.5,
    "state": "running",
    "alarm": false
  }
}
```

Flux doesn't know what these entities represent. Applications bring the semantics.

---

## Limitations

**Phase 1 constraints:**

- No property deletion (only overwrite)
- No transactions across entities
- No complex queries (filters, joins)
- No schema validation
- In-memory only (no persistence)

**Future improvements:**

- Snapshot persistence
- Property removal
- Query DSL for filtering
- Optional schema validation
- Indexing for fast lookups
