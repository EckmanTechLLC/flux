# ADR-008: Namespace Persistence

**Date:** 2026-02-23
**Status:** Proposed
**Deciders:** etl

---

## Problem

`NamespaceRegistry` is purely in-memory. Every Flux restart loses all registered namespaces and tokens. This is a critical blocker for the public instance — users must re-register and update tokens after every deployment.

---

## Decision

Persist namespaces in SQLite using the same pattern already established in `connector-manager/src/generic_config.rs`. This is the smallest change that solves the problem with zero new dependencies.

---

## Schema

Single table in `namespaces.db`:

```sql
CREATE TABLE IF NOT EXISTS namespaces (
    id         TEXT PRIMARY KEY,    -- ns_{random_8chars}
    name       TEXT UNIQUE NOT NULL, -- user-chosen name
    token      TEXT NOT NULL,        -- Bearer token (UUID v4)
    created_at TEXT NOT NULL         -- RFC3339
);
```

`entity_count` is not stored — it is derived at runtime from in-memory state.

---

## Approach

Keep `NamespaceRegistry` as the hot-path in-memory store (no change to its public interface or DashMap lookups). Add a new `NamespaceStore` that owns the SQLite connection.

**Startup flow:**
1. `NamespaceStore::new(db_path)` opens/creates the DB, creates table if missing
2. `NamespaceStore::load_all()` returns all persisted namespaces
3. `NamespaceRegistry::new_persistent(store)` loads them into the DashMaps

**Write flow:**
1. `register()` writes to `NamespaceStore` first (fail fast if DB write fails)
2. Then inserts into in-memory DashMaps as before

**Configuration:**
- New env var: `FLUX_NAMESPACE_DB` (default: `namespaces.db`)
- If env var absent, use the default path (always-on, no opt-in flag needed)

---

## Files Changed

| File | Change |
|------|--------|
| `src/namespace/store.rs` | New — `NamespaceStore` (SQLite CRUD) |
| `src/namespace/mod.rs` | Add optional `store` field to `NamespaceRegistry`; `register()` writes through; add `new_persistent()` constructor |
| `src/main.rs` | Initialize `NamespaceStore`, use `NamespaceRegistry::new_persistent()` |
| `docker-compose.yml` | Add `FLUX_NAMESPACE_DB` env var, mount volume for `namespaces.db` |

No `Cargo.toml` changes — `rusqlite = { version = "0.32", features = ["bundled"] }` is already present.

---

## Implementation Tasks

### Task 1: NamespaceStore

Create `src/namespace/store.rs`:
- `NamespaceStore { conn: Mutex<Connection> }`
- `new(db_path: &str) -> Result<Self>` — opens DB, creates table
- `insert(ns: &Namespace) -> Result<()>`
- `load_all() -> Result<Vec<Namespace>>`

Add `mod store;` to `src/namespace/mod.rs`.

Add unit tests (`":memory:"` DB, insert + load round-trip).

### Task 2: Wire into Registry and Main

Modify `src/namespace/mod.rs`:
- Add `store: Option<NamespaceStore>` to `NamespaceRegistry`
- `new()` keeps `store: None` (existing tests unaffected)
- `new_persistent(store: NamespaceStore) -> Self` loads namespaces from store into DashMaps
- `register()` writes to store before inserting into memory (if store is `Some`)

Modify `src/main.rs`:
- Read `FLUX_NAMESPACE_DB` env var (default `"namespaces.db"`)
- Initialize `NamespaceStore`, propagate error as warning if it fails (fall back to `NamespaceRegistry::new()`)
- Use `NamespaceRegistry::new_persistent(store)` on success

Modify `docker-compose.yml`:
- Add `FLUX_NAMESPACE_DB=/data/namespaces.db` under flux service env
- Ensure `/data` volume is mounted (reuse existing data volume if present)

---

## Rejected Alternatives

**Persist in NATS:** Events are not the right medium for config data. Namespace records are mutable (future: token rotation), not append-only facts.

**Share connector-manager's SQLite:** Wrong service boundary. Flux core must not depend on connector-manager's DB.

**PostgreSQL:** No scale requirement justifies the operational overhead.
