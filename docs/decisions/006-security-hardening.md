# ADR-006: Security Hardening

**Status:** Accepted
**Date:** 2026-02-20
**Context:** GitHub Issue #1 — 15 security findings against Flux HTTP/WebSocket surface

---

## Context

A security review of the deployed Flux instance identified 15 findings. This ADR documents the decisions made for each and defines an implementation plan that maintains Flux's core performance guarantee: thousands of events/second throughput.

**Key constraint:** No overhead added to the hot event ingestion path. All configurable limits must be changeable at runtime without restart or rebuild.

**Reference:** ADR-003 (auth model), ADR-004 (metrics/deletion API)

---

## Findings Summary & Disposition

| # | Finding | Decision |
|---|---------|----------|
| 1 | No default auth enforcement | Accepted risk — internal mode is intentional (ADR-003) |
| 2 | Rate limiting absent | Fix — per-namespace, auth-gated, runtime-configurable |
| 3 | No body size limits | Fix — 1MB single / 10MB batch, runtime-configurable |
| 4 | WebSocket auth not enforced | Fix — enforce when auth_enabled=true |
| 5 | Batch delete unauthenticated | Fix — require auth when auth_enabled=true |
| 6–15 | Error message leakage | No action — messages are already non-leaking; findings were false positives |

---

## Decisions

### 1. Auth Default Stays False

`FLUX_AUTH_ENABLED=false` remains the default. Internal deployments (7 VMs publishing to the private instance) must continue working without any configuration change.

Opt-in to auth via:
```bash
FLUX_AUTH_ENABLED=true
```
or `config.toml`:
```toml
[auth]
enabled = true
```

No security controls in this ADR apply in `auth_enabled=false` mode (internal deployments are trusted networks).

### 2. Runtime Config Store + Admin API

All tunable limits are stored in a `RuntimeConfig` struct backed by `Arc<RwLock<...>>`. Changes take effect immediately for new requests — no restart required.

**Admin API** (auth-gated — requires admin token):
```
GET  /api/admin/config          # Read current runtime config
PUT  /api/admin/config          # Update one or more fields
```

**Configurable fields:**
```json
{
  "rate_limit_enabled": true,
  "rate_limit_per_namespace_per_minute": 10000,
  "body_size_limit_single_bytes": 1048576,
  "body_size_limit_batch_bytes": 10485760
}
```

**Admin token:** Separate from namespace tokens. Set via `FLUX_ADMIN_TOKEN` env var or `config.toml`. Required for `PUT /api/admin/config`. `GET` is readable by any authenticated user.

This is the **foundation** for Sessions 2–4. Nothing else ships until this exists.

### 3. Body Size Limits

Enforced at the Axum layer via `DefaultBodyLimit` middleware — before JSON deserialization. Zero overhead on valid requests.

**Defaults:**
- Single event (`POST /api/events`): **1 MB**
- Batch events (`POST /api/events/batch`): **10 MB**

Both limits are runtime-configurable via the admin API (Session 2 builds on Session 1).

Response on exceeded limit: `413 Payload Too Large`

### 4. Rate Limiting

**Scope:** Active only when `auth_enabled=true`. Internal deployments are unaffected.

**Granularity:** Per namespace (not per IP, not global). A single misbehaving namespace cannot starve others.

**Default:** 10,000 events/minute per namespace (~167 eps). Sufficient for all current publishers (7 VMs at ~1 eps each).

**Implementation:**
- Token bucket per namespace, stored in `DashMap<String, TokenBucket>`
- Token bucket checked in ingestion handler before NATS publish
- Bucket state is in-memory only (resets on restart — acceptable)
- Limit is runtime-configurable via admin API

**Hot path impact:** Single `DashMap` lookup + atomic counter decrement per event. Negligible at thousands of eps.

Response on exceeded limit: `429 Too Many Requests` with `Retry-After` header.

### 5. WebSocket Auth Enforcement

When `auth_enabled=true`, WebSocket upgrade requires a valid bearer token. Token passed as query parameter (WebSocket protocol limitation):

```
ws://host/ws?token=<bearer-token>
```

**Enforcement:**
- Token validated on upgrade (namespace existence + token match)
- Invalid/missing token: HTTP 401 before upgrade
- Valid token: connection proceeds normally

**Reads remain open in internal mode** (`auth_enabled=false`). No auth required on WebSocket in internal mode.

**Implementation:** Added to `src/api/websocket.rs` upgrade handler. Reuses existing `validate_token()` from ADR-003.

### 6. Batch Deletion Auth

Batch `POST /api/state/entities/delete` already requires auth when `auth_enabled=true` per ADR-004. This is correctly implemented. No change needed.

Confirmed: when `auth_enabled=false`, batch delete is open access (internal deployments are trusted).

### 7. Error Message Leakage — No Action

Reviewed all error response bodies. Messages are already generic (e.g., `"Unauthorized"`, `"Not Found"`). No stack traces, internal paths, or sensitive data are leaked. Findings 6–15 were false positives from the scanner misidentifying Rust's standard error types. No changes needed.

---

## Implementation Plan

### Session 1: Runtime Config Store + Admin API

**Why first:** Sessions 2–4 all depend on reading runtime config. No security feature ships without this.

**Files:**
- `src/config/runtime.rs` — `RuntimeConfig` struct, `Arc<RwLock<RuntimeConfig>>`
- `src/api/admin.rs` — `GET /api/admin/config`, `PUT /api/admin/config`
- Modify `src/api/mod.rs` — register admin routes
- Modify `src/main.rs` — initialize `RuntimeConfig`, pass to `AppState`

**Scope:**
- `RuntimeConfig` with all tunable fields (rate limits, body size limits)
- Load initial values from `config.toml` / env vars
- Admin token validation on `PUT`
- No-restart update: write lock, update fields, release

**Tests:** Config reads, config update, admin token enforcement.

### Session 2: Body Size Limits

**Depends on:** Session 1 (reads limits from `RuntimeConfig`)

**Files:**
- Modify `src/api/mod.rs` — apply `DefaultBodyLimit` middleware per route
- Add integration test for 413 responses

**Note:** Axum's `DefaultBodyLimit` is set at route registration time — for runtime-configurable limits, we use a custom extractor that reads from `RuntimeConfig` instead of Axum's built-in middleware.

**Scope:**
- Custom body size extractor reading from `Arc<RuntimeConfig>`
- Applied to `POST /api/events` and `POST /api/events/batch`
- Return `413` with `{"error": "payload too large"}` on exceeded limit

### Session 3: Rate Limiting

**Depends on:** Session 1 (reads limit from `RuntimeConfig`)

**Files:**
- `src/rate_limit/mod.rs` — `RateLimiter` (DashMap of token buckets)
- Modify `src/api/ingestion.rs` — check rate limit before NATS publish
- Modify `src/main.rs` — initialize `RateLimiter`, pass to `AppState`

**Scope:**
- Token bucket algorithm (refill rate = limit/60 tokens/sec)
- Per-namespace bucket, created lazily on first event
- Conditioned on `auth_enabled=true` (no-op otherwise)
- Runtime limit change applies to new refill calculations immediately
- `429` response with `Retry-After: 60`

**Tests:** Rate limit enforcement, auth-disabled bypass, limit update via admin API.

### Session 4: WebSocket Auth Enforcement

**Depends on:** Session 1 (reads auth config from `RuntimeConfig` or `AppConfig`)

**Files:**
- Modify `src/api/websocket.rs` — token validation on upgrade
- Add integration test for 401 on missing/invalid token

**Scope:**
- Extract `?token=` query param from upgrade request
- Validate against namespace registry (reuse `validate_token()`)
- Conditioned on `auth_enabled=true`
- Internal mode: no change to existing behavior

---

## Consequences

### Positive

- ✅ Rate limiting prevents namespace abuse without affecting others
- ✅ Body size limits prevent memory exhaustion from oversized payloads
- ✅ WebSocket auth closes the unauthenticated subscription gap in public mode
- ✅ Runtime config eliminates downtime for limit adjustments
- ✅ All controls are auth-gated — zero impact on internal deployments
- ✅ Hot path overhead is negligible (one DashMap lookup per event)

### Negative

- ⚠️ Rate limiter state is in-memory (resets on restart) — acceptable for Phase 6
- ⚠️ WebSocket token in query param is visible in server logs — mitigated by HTTPS at the Cloudflare layer
- ⚠️ Admin token is a single shared secret — sufficient for single-instance deployment

### Out of Scope (Not Addressed)

- Token expiration/rotation — deferred to Phase 4+ (ADR-003)
- Read authorization (private entities) — intentional design (world is open)
- HTTPS enforcement — handled at Cloudflare tunnel, not Flux's concern
- Audit logging — no current requirement

---

## References

- ADR-003: Multi-tenancy and Authentication (`docs/decisions/003-multitenancy-and-authentication.md`)
- ADR-004: Real-Time Metrics & Entity Management (`docs/decisions/004-realtime-metrics-and-deletion.md`)
- GitHub Issue #1: Security findings (15 items)
