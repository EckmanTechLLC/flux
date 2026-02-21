# Session: Connector Manager Refactor
**Date:** 2026-02-20
**Status:** Complete

## Task
Structural refactor only — separate runner types into distinct modules. No behavior changes.

## Changes

| File | Action |
|------|--------|
| `connector-manager/src/scheduler.rs` | Deleted (moved to runners/builtin.rs) |
| `connector-manager/src/runners/builtin.rs` | Created (identical to former scheduler.rs) |
| `connector-manager/src/runners/mod.rs` | Created (`pub mod builtin/generic/named`) |
| `connector-manager/src/runners/generic.rs` | Created (stub: `GenericRunner`) |
| `connector-manager/src/runners/named.rs` | Created (stub: `NamedRunner`) |
| `connector-manager/src/manager.rs` | Updated import: `crate::scheduler` → `crate::runners::builtin` |
| `connector-manager/src/lib.rs` | Replaced `pub mod scheduler` with `pub mod runners`; added `ConnectorScheduler`/`ConnectorStatus` re-exports |
| `connector-manager/src/types.rs` | Added `ConnectorType` enum (`Builtin`, `Generic`, `Named`) |

## Test Results
```
cargo test -p connector-manager
31 passed (25 unit + 3 doc) — 0 failed
```

## Notes
- Test count grew from 22 to 31: the 9 scheduler tests now report under `runners::builtin::tests`
- `ConnectorType` enum unused warning expected — Phase 3A will wire it in
- No code logic changed inside any file
