# ADR-003: Multi-tenancy and Authentication

**Status:** Proposed
**Date:** 2026-02-13
**Deciders:** Architecture Team

---

## Context

Phase 1 (state engine MVP) and Phase 2 (persistence/recovery) are complete. Flux currently has no authentication or namespace isolation - suitable for internal deployments, but not for public/shared instances.

**Two deployment scenarios:**

1. **Internal/Private Instances** (current behavior)
   - Trusted environment (VPN, internal network)
   - No authentication required
   - Simple entity IDs (`arc-01`, `sensor_42`)
   - Zero friction for internal use

2. **Public/Shared Instances** (new requirement)
   - Untrusted environment (public internet)
   - Multiple users sharing one Flux instance
   - Need namespace ownership (prevent name collisions)
   - Need write control (only owner can update their entities)
   - Open reading (anyone can observe/subscribe to any entity)

**Key principle:** Internal instances must work exactly as today (backward compatible). Auth and namespacing are opt-in via configuration.

**Reference:** `/docs/decisions/000-flux-internal-public-instances` (Arc's design discussion)

---

## Decision

### Two-Mode Architecture

**Configuration flag:** `auth_enabled = false` (default)

**Internal mode** (`auth_enabled = false`):
- No authentication
- No namespace registration
- Simple entity IDs (`entity_id`)
- Current behavior preserved

**Public mode** (`auth_enabled = true`):
- Namespace registration required
- Token-based write authorization
- Namespaced entity IDs (`namespace/entity_id`)
- Open reading (no auth required for queries/subscriptions)

### Namespace Model

**Registration flow:**
```
1. User → POST /api/namespaces {"name": "matt"}
2. Flux validates name (unique, [a-z0-9-_], 3-32 chars)
3. Flux generates system ID (ns_7x9f2a) and token (UUID)
4. Flux stores namespace record
5. Flux returns: {namespace_id, name, token}
6. User stores token, uses for all writes
```

**Namespace struct:**
```rust
pub struct Namespace {
    pub id: String,              // ns_{random} (system ID)
    pub name: String,            // user-chosen (matt, arc, sensor-team)
    pub token: String,           // UUID bearer token
    pub created_at: DateTime<Utc>,
    pub entity_count: u64,       // Stats (optional)
}
```

**Entity ID format:**
- Internal mode: `entity_id` (no prefix)
- Public mode: `namespace/entity_id` (e.g., `matt/arc-01`)
- Namespace extracted from entity_id in events
- Validation: entity_id must match token's namespace

**Storage:** In-memory registry (`DashMap<String, Namespace>`)
- Future: Persist to file/database for multi-instance deployments

### Token Model

**Format:** UUID v4 (simple bearer token)
```
Authorization: Bearer 550e8400-e29b-41d4-a716-446655440000
```

**Why UUID (not JWT):**
- Simpler implementation (no signature verification)
- Sufficient for Phase 3 (single-instance)
- Stateful (can revoke by removing from registry)
- Fast lookup (O(1) in DashMap)

**Future (Phase 4+):** JWT tokens for stateless auth, token expiration, refresh tokens

**Token generation:**
```rust
use uuid::Uuid;

pub fn generate_token() -> String {
    Uuid::new_v4().to_string()
}
```

**Token validation:**
```rust
pub fn validate_token(&self, token: &str, namespace: &str) -> Result<()> {
    let ns = self.namespaces.get(namespace)
        .ok_or(Error::NamespaceNotFound)?;

    if ns.token != token {
        return Err(Error::Unauthorized);
    }

    Ok(())
}
```

### Authorization Enforcement

**Where:** Event ingestion layer only (writes)

**Flow:**
```
POST /api/events
  ↓
Extract Authorization header (if auth_enabled)
  ↓
Parse entity_id from payload → extract namespace
  ↓
Validate: token owns namespace
  ↓
Allow/Deny event ingestion
```

**Reads remain open:**
- `GET /api/state/*` - No auth required
- WebSocket subscriptions - No auth required
- Core principle: "The world is open for observation"

**Code location:** `src/api/ingestion.rs`
```rust
async fn publish_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(event): Json<FluxEvent>,
) -> Result<Json<PublishResponse>> {
    // If auth enabled, validate token
    if state.config.auth_enabled {
        let token = extract_bearer_token(&headers)?;
        let namespace = extract_namespace_from_entity_id(&event)?;
        state.namespace_registry.validate_token(token, namespace)?;
    }

    // Proceed with event ingestion
    ...
}
```

### API Changes

**New endpoints:**
```
POST /api/namespaces
  Body: {"name": "matt"}
  Response: {"namespace_id": "ns_7x9f2a", "name": "matt", "token": "uuid"}

GET /api/namespaces/:name
  Response: {"namespace_id": "ns_7x9f2a", "name": "matt", "created_at": "...", "entity_count": 42}
```

**Modified endpoints:**
```
POST /api/events
  - If auth_enabled: Require Authorization header
  - Validate namespace ownership
  - Reject if unauthorized

POST /api/events/batch
  - Same authorization logic
  - Validate all events in batch
```

**Unchanged endpoints (no auth):**
```
GET /api/state/entities
GET /api/state/entities/:id
WebSocket /ws
```

### Configuration

**Add to config.toml:**
```toml
[auth]
enabled = false  # Default: internal mode
namespace_storage = "memory"  # Future: "file", "postgres"

[namespace]
name_pattern = "^[a-z0-9-_]{3,32}$"  # Validation regex
```

**Environment override:**
```bash
FLUX_AUTH_ENABLED=true cargo run
```

### Backward Compatibility

**Internal deployments (auth_enabled=false):**
- No code changes required
- No config changes required
- Entity IDs remain simple strings
- No authentication headers needed

**Migration path for existing internal deployments:**
- Continue with `auth_enabled=false` (default)
- Opt-in to public mode when needed
- No breaking changes to API

---

## Implementation Tasks

### Task 1: Namespace Model & Registry (Small)
**Files:**
- `src/namespace/mod.rs` - Namespace struct, registry
- `src/namespace/validation.rs` - Name validation
- `src/namespace/tests.rs` - Unit tests

**Scope:**
- Namespace struct (id, name, token, created_at)
- NamespaceRegistry (DashMap storage)
- register_namespace() method
- validate_token() method
- Name validation (regex, uniqueness)

### Task 2: Token Generation & Extraction (Small)
**Files:**
- `src/auth/mod.rs` - Token utilities
- `src/auth/middleware.rs` - Extract Authorization header
- `src/auth/tests.rs` - Unit tests

**Scope:**
- generate_token() - UUID v4
- extract_bearer_token(headers) - Parse "Bearer {token}"
- Error types (Unauthorized, InvalidToken)

### Task 3: Entity ID Parsing (Small)
**Files:**
- `src/entity_id.rs` - Parse namespace/entity_id
- Tests for parsing logic

**Scope:**
- parse_entity_id(id) → (namespace, entity_id)
- Handle both formats: "namespace/entity" and "entity"
- Validation: namespace exists (if auth_enabled)

### Task 4: Authorization Middleware (Medium)
**Files:**
- `src/api/auth.rs` - Authorization logic
- Modify `src/api/ingestion.rs` - Add auth checks

**Scope:**
- Extract token from request headers
- Extract namespace from event payload (entity_id)
- Validate token owns namespace
- Conditional: Only if auth_enabled
- Error responses (401, 403)

### Task 5: Namespace Registration API (Medium)
**Files:**
- `src/api/namespace.rs` - Namespace endpoints
- Modify `src/api/mod.rs` - Add namespace routes

**Scope:**
- POST /api/namespaces - Register namespace
- GET /api/namespaces/:name - Get namespace info
- Error handling (duplicate name, invalid format)
- Integration test (register → publish with token)

### Task 6: Entity Query Filtering & Discovery (Small)
**Files:**
- Modify `src/api/query.rs` - Add query parameters
- Add tests for filtering logic

**Scope:**
- Add query params to GET /api/state/entities:
  - `?namespace=matt` - Filter by namespace
  - `?prefix=matt/sensor` - Filter by entity ID prefix
- Implementation: Filter DashMap keys by string matching
- Document `_directory` convention (e.g., `matt/_directory`) as optional pattern for entity discovery
- Document property conventions (`type`, `tags`, `capabilities`) in API examples
- Keep Flux domain-agnostic: filtering on entity ID strings only, payload remains opaque

### Task 7: Configuration & Documentation (Small)
**Files:**
- Modify `config.toml` - Add [auth] section
- Update `docs/architecture.md` - Add auth section
- Update `README.md` - Document auth setup
- Create `docs/auth-guide.md` - Public instance setup

**Scope:**
- Config structure (AuthConfig)
- Load config in main.rs
- Pass config to API handlers
- Document two deployment modes
- Example: Setting up public instance

**Dependencies:**
- Task 2 → Task 1 (tokens need namespaces)
- Task 3 → Task 1 (parsing needs namespace validation)
- Task 4 → Task 1, Task 2, Task 3 (middleware needs all)
- Task 5 → Task 1, Task 2, Task 4 (API needs full auth)
- Task 6 → Task 1 (filtering uses namespace model)
- Task 7 → Task 5, Task 6 (docs after implementation)

**Estimated effort:**
- Small: 1-2 hours
- Medium: 3-4 hours
- Total: ~14-17 hours

---

## Consequences

### Positive

- ✅ Supports both internal and public deployments
- ✅ Backward compatible (no breaking changes)
- ✅ Simple token model (UUID, easy to implement)
- ✅ Open reading (encourages exploration and coordination)
- ✅ Namespace ownership (prevents collisions)
- ✅ Minimal overhead (in-memory registry, fast lookups)

### Negative

- ⚠️ Stateful tokens (can't scale horizontally without shared storage) → *Phase 4: JWT*
- ⚠️ No token expiration/revocation API (manual registry editing) → *Phase 4*
- ⚠️ In-memory only (lost on restart) → *Phase 4: persist namespaces*
- ⚠️ No rate limiting (single user can spam) → *Phase 4: quotas*

### Neutral

- Adds complexity (conditional auth logic throughout API)
- Entity IDs become longer in public mode (`namespace/entity` vs `entity`)
- Token management is user responsibility (store securely)

---

## Security Considerations

**What Phase 3 provides:**
- Write authorization (only namespace owner can publish)
- Namespace isolation (no name collisions)
- Simple bearer tokens (sufficient for trusted users)

**What Phase 3 does NOT provide:**
- Token expiration/rotation
- Rate limiting per namespace
- Read authorization (intentionally open)
- Token revocation API
- Audit logging
- HTTPS enforcement (deployment concern)

**Deployment recommendations for public instances:**
- Use HTTPS (TLS termination via reverse proxy)
- Distribute tokens securely (encrypted channels)
- Monitor for abuse (logs, metrics)
- Consider VPN or IP allowlisting for sensitive deployments

---

## Future Enhancements (Phase 4+)

**JWT tokens:**
- Stateless authentication
- Token expiration/refresh
- Embedded claims (namespace, permissions)
- Signature verification

**Namespace persistence:**
- Save to file or database
- Survive Flux restarts
- Multi-instance support (shared registry)

**Advanced features:**
- Token revocation API
- Rate limiting per namespace
- Namespace deletion
- Read authorization (private entities)
- Audit logging (who did what, when)

---

## References

- Phase 1: `/docs/decisions/001-flux-state-engine-architecture.md`
- Phase 2: `/docs/decisions/002-persistence-and-recovery.md`
- Design discussion: `/docs/decisions/000-flux-internal-public-instances`
- Token format: [UUID v4](https://en.wikipedia.org/wiki/Universally_unique_identifier#Version_4_(random))

---

## Next Steps

1. Review and approve ADR-003
2. Implement Phase 3 tasks (1-6) sequentially
3. Test both modes: internal (no auth) and public (with auth)
4. Deploy test instance with auth_enabled=true
5. Document public instance setup for users
6. Plan Phase 4 (JWT, persistence, advanced features)
