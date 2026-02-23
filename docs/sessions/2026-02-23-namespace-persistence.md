# Session: Namespace Persistence (ADR-008)

**Date:** 2026-02-23
**ADR:** docs/decisions/008-namespace-persistence.md
**Status:** Implementation complete — awaiting rebuild + test

---

## What Was Done

### Task 1: NamespaceStore

Created `src/namespace/store.rs`:
- `NamespaceStore { conn: Mutex<Connection> }`
- `new(db_path)` — opens/creates DB, creates table
- `insert(ns)` — persists a namespace record
- `load_all()` — returns all namespaces ordered by created_at
- 5 unit tests (insert+load, empty, multi round-trip, duplicate name/id fail)

Added `pub mod store; pub use store::NamespaceStore;` to `src/namespace/mod.rs`.

### Task 2: Wire into Registry and Main

Modified `src/namespace/mod.rs`:
- Added `store: Option<NamespaceStore>` field to `NamespaceRegistry`
- `new()` keeps `store: None` — existing tests unaffected
- `new_persistent(store)` loads all namespaces from DB into DashMaps at startup
- `register()` calls `store.insert()` before touching in-memory maps (fail fast)
- Added `RegistrationError::StoreFailed` variant

Modified `src/api/namespace.rs`:
- Added `StoreFailed => 500 Internal Server Error` arm to match

Modified `src/main.rs`:
- Reads `FLUX_NAMESPACE_DB` env var (default `"namespaces.db"`)
- Tries `NamespaceStore::new()` — on success uses `new_persistent()`, on failure warns and falls back to `new()`

Modified `docker-compose.yml`:
- Added `FLUX_NAMESPACE_DB=/data/namespaces.db` to flux service env

---

## Files Modified

| File | Change |
|------|--------|
| `src/namespace/store.rs` | Created (new) |
| `src/namespace/mod.rs` | Added store module, store field, new_persistent, write-through |
| `src/api/namespace.rs` | Added StoreFailed match arm |
| `src/main.rs` | Init NamespaceStore, use new_persistent |
| `docker-compose.yml` | Added FLUX_NAMESPACE_DB env var |

---

## To Deploy

```
docker compose build --no-cache flux && docker compose up -d flux
```

Verify in logs:
```
Namespace store initialized at /data/namespaces.db
```

## To Test

Register a namespace, restart flux, confirm it's still present:
```bash
curl -X POST http://localhost:3000/api/namespaces \
  -H "Content-Type: application/json" \
  -d '{"name": "test-persist"}'
# Save the token

# Restart flux:
docker compose restart flux

# Query namespaces — should still see test-persist
curl http://localhost:3000/api/namespaces
```

## Notes

- `/data` volume already mounted in docker-compose.yml (`./data:/data`), so no volume changes needed
- Fallback to in-memory is a safety net for edge cases; production should always have a writable `/data`
- `entity_count` is not persisted — it stays 0 on load, correct behavior per ADR-008
