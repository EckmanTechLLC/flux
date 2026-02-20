# Session: API & Architecture Docs Update

**Date:** 2026-02-20
**Task:** Update docs/api.md and docs/architecture.md to reflect current implementation

---

## What Was Done

### docs/api.md

- Removed "Phase 1" status language (header, auth section, rate limits section, client libraries section)
- Updated date to 2026-02-20
- Updated **Authentication** section: now documented as implemented (Bearer token, FLUX_AUTH_ENABLED, WebSocket ?token= param)
- Updated **Rate Limits** section: 10k/min per namespace, auth-gated, body size limits documented
- Added **Namespace Management** section: POST /api/namespaces, GET /api/namespaces/:name
- Added **Connector Management** section: GET /api/connectors, GET /api/connectors/:name, POST /api/connectors/:name/token, DELETE /api/connectors/:name/token
- Added **Admin Config** section: GET /api/admin/config, PUT /api/admin/config
- Added WebSocket auth note (?token= query param, 401 before upgrade)
- Updated **Error Handling** section: added 401, 403, 413, 429 with descriptions

### docs/architecture.md

- Updated date to 2026-02-20
- Updated **Deployment Architecture** diagram: added flux-ui (port 8082) and connector-manager (internal only); corrected NATS port (4223 external / 4222 internal)
- Removed "Phase 1 limitation" note about snapshot persistence from State Engine section
- Removed "Snapshot persistence" from Performance "Future improvements" (snapshots are implemented)
- Added **Connector Framework** section: architecture diagram, credential storage, connector table, API cross-reference
- Added **Security Hardening** section: runtime config store, body size limits, rate limiting, WebSocket auth, admin API

---

## Files Modified

- `docs/api.md` - Complete rewrite (was Phase 1 only, now current)
- `docs/architecture.md` - Targeted edits (6 changes)

## Files NOT Modified

All source files left unchanged.

---

## Verification

No code was modified. Documentation reflects:
- `src/api/connectors.rs` — connector endpoints (GET list, GET detail, POST/DELETE token)
- `src/api/admin.rs` — GET/PUT /api/admin/config
- `src/api/namespace.rs` — POST/GET /api/namespaces
- `docs/decisions/005-connector-framework.md` — connector architecture
- `docs/decisions/006-security-hardening.md` — security controls
- `docker-compose.yml` — service layout and ports
