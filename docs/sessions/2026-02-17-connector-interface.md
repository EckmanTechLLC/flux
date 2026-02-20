# Session: Connector Interface Foundation

**Date:** 2026-02-17
**Task:** Phase 1, Task 3 - Create Connector Interface
**Reference:** ADR-005 lines 73-96
**Status:** ✅ COMPLETE

---

## Objective

Create the base connector interface as a separate crate that will be used by both the connector manager and individual connectors.

## Files Created

1. **connector-manager/Cargo.toml** - New crate manifest
   - Dependencies: flux (parent crate), async-trait, serde, chrono, anyhow
   - Re-uses FluxEvent from flux crate for compatibility

2. **connector-manager/src/lib.rs** - Crate root
   - Module exports: connector, types
   - Re-exports: Connector, OAuthConfig, Credentials, FluxEvent
   - Comprehensive crate-level documentation with architecture diagram

3. **connector-manager/src/types.rs** - Core data types
   - `OAuthConfig`: OAuth endpoints and scopes
   - `Credentials`: Access token, refresh token, expiration
   - Full doc comments with examples and security notes

4. **connector-manager/src/connector.rs** - Connector trait
   - `Connector` trait with 4 methods:
     - `name()` - Returns connector identifier
     - `oauth_config()` - Returns OAuth configuration
     - `fetch(credentials)` - Async fetch, returns Vec<FluxEvent>
     - `poll_interval()` - Returns poll interval in seconds
   - Comprehensive trait documentation with lifecycle explanation
   - Example mock implementation in doc comments

## Files Modified

1. **src/lib.rs** - Added FluxEvent re-export
   - Added `pub use event::FluxEvent;` for external crate access
   - Minimal change to existing code

## Design Decisions

### Re-use FluxEvent
- Connector-manager depends on flux crate and re-exports FluxEvent
- Ensures type compatibility (no conversion needed)
- Connectors return events in native Flux format

### Async Trait Pattern
- Used `async-trait` crate for async methods in traits
- `fetch()` is async (network I/O)
- Standard Rust pattern for async trait methods

### Stateless Connectors
- Connectors have no persistent state
- Credentials managed externally by connector manager
- Poll schedule managed externally
- Simplifies connector implementation

### Type Organization
- `types.rs`: Pure data structures (OAuthConfig, Credentials)
- `connector.rs`: Behavior definition (Connector trait)
- `lib.rs`: Public API surface

## Testing

### Build Verification
```bash
cd connector-manager && cargo build
```
**Result:** ✅ Compiled successfully (50.72s)

### Doc Test Verification
```bash
cargo test --doc
```
**Result:** ✅ 3 doc tests passed
- OAuthConfig example compiles
- Credentials example compiles
- Mock GitHubConnector implementation compiles

### Interface Validation
- Mock connector successfully implements all trait methods
- Types are properly serializable (Serde)
- Async trait methods work as expected

## Architecture Notes

### Connector Lifecycle
1. Manager calls `oauth_config()` to get OAuth endpoints
2. User authorizes via OAuth flow (Flux UI)
3. Manager stores encrypted credentials
4. Manager calls `fetch(credentials)` on schedule
5. Connector returns Flux events
6. Manager publishes events to Flux

### Connector Responsibilities
- ✅ Authenticate with external API (using provided credentials)
- ✅ Fetch data from API
- ✅ Transform data to Flux events
- ✅ Handle rate limits, pagination, errors
- ❌ NOT responsible for: credential storage, scheduling, event publishing

### Manager Responsibilities (Future)
- Store/encrypt credentials
- Schedule polling based on `poll_interval()`
- Decrypt credentials before calling `fetch()`
- Publish returned events to Flux
- Handle retries, backoff, error reporting

## Next Steps (Phase 1)

1. **Task 1: Credential Storage** (Medium)
   - SQLite schema, AES-256-GCM encryption
   - Files: `connector-manager/src/credentials/storage.rs`

2. **Task 2: OAuth Flow** (Large)
   - OAuth endpoints, callback handler
   - Files: `flux/src/api/oauth.rs`, UI components

3. **Task 4: Connector Manager Core** (Large)
   - Load connectors, schedule polling, publish events
   - Files: `connector-manager/src/manager.rs`

## Dependencies

### External Crates
- `async-trait@0.1` - Async trait support
- `serde@1.0` - Serialization
- `chrono@0.4` - Timestamp handling
- `anyhow@1.0` - Error handling

### Internal Crates
- `flux` (parent) - FluxEvent type

## Observations

### What Worked Well
- Re-using FluxEvent avoided duplication
- async-trait made async methods ergonomic
- Doc examples serve as integration tests
- Comprehensive documentation upfront

### Potential Issues
- None identified at this stage
- Interface is simple and focused

### Questions for Later
- Should connectors implement incremental sync?
  - Track "last fetched" timestamp per connector
  - Only fetch new data (reduce API calls)
  - Requires connector state storage (violates stateless principle)
  - Defer to Phase 5

- Should we support bidirectional sync?
  - Write back to external API from Flux
  - Complex conflict resolution
  - Out of scope for Phase 1-3

## Session Metrics

- **Files created:** 4
- **Files modified:** 1
- **Lines of code:** ~250 (including docs)
- **Build time:** 50.72s (first build, dependencies downloaded)
- **Tests passing:** 3/3 doc tests
- **Time spent:** ~30 minutes

---

## Checklist Completion

### 1. READ FIRST
- [x] Read CLAUDE.md
- [x] Read ADR-005 (Connector Framework)
- [x] Read existing event model (src/event/mod.rs)
- [x] Verified understanding

### 2. VERIFY ASSUMPTIONS
- [x] Confirmed FluxEvent exists and is compatible
- [x] Verified async-trait is standard pattern
- [x] Confirmed crate structure requirements
- [x] Listed files to create before creating them

### 3. MAKE CHANGES
- [x] Created connector-manager crate
- [x] Defined Connector trait (4 methods)
- [x] Defined OAuthConfig, Credentials types
- [x] Re-exported FluxEvent from flux crate
- [x] Added comprehensive doc comments
- [x] Included usage examples in docs

### 4. TEST & VERIFY
- [x] Cargo build successful
- [x] Doc tests pass (3/3)
- [x] Mock connector compiles
- [x] Interface is usable

### 5. DOCUMENT
- [x] Created session notes
- [x] Documented design decisions
- [x] Listed files created/modified
- [x] Noted next steps

### 6. REPORT
- [x] Provided summary to user
- [x] No blockers encountered
- [x] Scope not exceeded
- [x] Next steps identified

---

**Status:** Ready for Phase 1 Task 1 (Credential Storage)
