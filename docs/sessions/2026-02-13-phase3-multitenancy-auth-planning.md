# Phase 3 Planning: Multi-tenancy & Authentication

**Date:** 2026-02-13
**Session:** phase3-multitenancy-auth-planning
**Phase:** Phase 3 Planning
**Status:** Planning Complete

---

## Objective

Plan Phase 3 of Flux: Add multi-tenancy and authentication to support both internal (no auth) and public (token-based) deployment models.

---

## Context

- Phase 1 (state engine MVP) complete: Event ingestion, state derivation, WebSocket subscriptions
- Phase 2 (persistence/recovery) complete: Snapshots, sequence tracking, recovery on startup
- Current state: No authentication, no namespace isolation (suitable for internal use only)
- Requirement: Support public/shared instances with namespace ownership and write control

**Reference documents:**
- `/FLUX-DESIGN.md` - Phase 3 scope (lines 500-506)
- `/docs/decisions/000-flux-internal-public-instances` - Deployment models
- `/docs/decisions/001-flux-state-engine-architecture.md` - Current architecture
- `/docs/decisions/002-persistence-and-recovery.md` - Phase 2 context

---

## Deliverables

### ADR-003: Multi-tenancy and Authentication

Created: `/docs/decisions/003-multitenancy-and-authentication.md`

**Key decisions:**

1. **Two deployment modes:**
   - Internal: No auth, no namespaces (default, backward compatible)
   - Public: Namespace registration + token-based write control

2. **Namespace model:**
   - User registers namespace: POST /api/namespaces {"name": "matt"}
   - System generates: namespace_id (ns_{random}) + token (UUID)
   - Entity IDs become: namespace/entity_id (public) or entity_id (internal)

3. **Token model:**
   - Simple bearer tokens (UUID v4)
   - In-memory storage (DashMap)
   - Stateful (sufficient for Phase 3)
   - Future: JWT for stateless auth

4. **Authorization enforcement:**
   - Write-only: Token required for POST /api/events (if auth_enabled)
   - Reads open: No auth for queries or subscriptions
   - Validate: token owns namespace extracted from entity_id

5. **Configuration:**
   - `auth_enabled = false` (default)
   - Opt-in for public instances
   - Backward compatible

**Implementation broken into 7 tasks:**
1. Namespace Model & Registry (Small)
2. Token Generation & Extraction (Small)
3. Entity ID Parsing (Small)
4. Authorization Middleware (Medium)
5. Namespace Registration API (Medium)
6. Entity Query Filtering & Discovery (Small)
7. Configuration & Documentation (Small)

**Total estimated effort:** 14-17 hours

---

## Design Principles

**From ADR-003:**

- Internal deployments must work exactly as today (zero friction)
- Auth and namespacing are opt-in (not mandatory)
- Reading is always open (core principle: "The world is open for observation")
- Writing is controlled (only namespace owner can publish)
- Simple implementation (UUID tokens, in-memory storage)
- Phase 3 scope: single-instance, trusted users
- Advanced features deferred to Phase 4 (JWT, persistence, rate limiting)

---

## Key Files Created

1. `/docs/decisions/003-multitenancy-and-authentication.md`
   - ADR with full design and rationale
   - 6 implementation tasks with dependencies
   - ~380 lines (concise, as requested)

2. `/docs/sessions/2026-02-13-phase3-multitenancy-auth-planning.md` (this file)
   - Session notes
   - Planning summary

---

## Next Actions

**User approval required:**
1. Review ADR-003
2. Approve design approach
3. Confirm 6-task breakdown is acceptable

**Once approved, implementation sequence:**
1. Task 1: Namespace Model & Registry
2. Task 2: Token Generation & Extraction
3. Task 3: Entity ID Parsing
4. Task 4: Authorization Middleware
5. Task 5: Namespace Registration API
6. Task 6: Entity Query Filtering & Discovery
7. Task 7: Configuration & Documentation

**Testing plan:**
- Test internal mode (auth_enabled=false) - must work as before
- Test public mode (auth_enabled=true) - namespace registration and auth
- Integration test: Register namespace → Publish with token → Subscribe (no auth)

---

## Updates

**2026-02-13 (later):** Added Task 6 - Entity Query Filtering & Discovery
- Query params for namespace/prefix filtering on GET /api/state/entities
- Documents `_directory` and property conventions (optional patterns)
- Keeps Flux domain-agnostic (string matching on entity IDs)
- Renumbered original Task 6 → Task 7

## Open Questions

None - design is complete and ready for review.

---

## References

- ADR-003: `/docs/decisions/003-multitenancy-and-authentication.md`
- Phase 1 ADR: `/docs/decisions/001-flux-state-engine-architecture.md`
- Phase 2 ADR: `/docs/decisions/002-persistence-and-recovery.md`
- Design discussion: `/docs/decisions/000-flux-internal-public-instances`
- CLAUDE.md: Project context and workflow
