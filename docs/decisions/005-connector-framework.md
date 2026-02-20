# ADR-005: Flux Connector Framework

**Status:** Proposed
**Date:** 2026-02-17
**Deciders:** Architecture Team

---

## Context

Flux currently requires external systems to publish events TO it. This works for services/agents that can push updates, but doesn't support pulling data FROM external APIs (GitHub, Gmail, LinkedIn, banks, calendars).

**Vision:** "Personal Life State Engine" - Flux maintains unified state from ALL your digital services via connectors.

**Current limitation:**
- No way to ingest data from external APIs
- Users must write custom scripts to poll and publish
- No OAuth integration for third-party services
- No standard pattern for data ingestion

**Use cases:**
- GitHub: repos, issues, PRs, notifications → Flux entities
- Gmail: emails, labels, threads → Flux entities
- LinkedIn: connections, messages, posts → Flux entities
- Calendar: events, meetings → Flux entities
- Banks: transactions, balances → Flux entities

**Design principles (from discussion 2026-02-14):**
- Hybrid approach (like Anthropic MCP): core + community connectors
- One-click OAuth via Flux UI (zero config files)
- Connectors are separate services, NOT part of Flux core
- Flux remains domain-agnostic (connectors add domain knowledge)
- Ease of use over flexibility (non-technical users can set up)

**Reference:**
- `/docs/decisions/001-flux-state-engine-architecture.md` - Domain-agnostic principles
- `/docs/decisions/003-multitenancy-and-authentication.md` - Auth patterns

---

## Decision

### Architecture Overview

```
External API (GitHub, Gmail, etc.)
         ↓
    OAuth (user authorizes)
         ↓
┌─────────────────────────────────────────┐
│       Connector (separate service)       │
│  - Poll external API                     │
│  - Transform to Flux events              │
│  - Publish to Flux                       │
└─────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────┐
│              Flux Core                   │
│  - Ingest events (unchanged)             │
│  - Derive state (unchanged)              │
│  - Expose via API (unchanged)            │
└─────────────────────────────────────────┘
         ↓
    Consumers (agents, UI, etc.)
```

**Critical separation:** Connectors are NOT part of Flux. They're independent services that happen to publish to Flux.

**Connector Manager:** Separate service that orchestrates connectors (start/stop, config, credentials).

**Flux UI:** Manages connectors (one-click OAuth, enable/disable, status).

### Connector Model

**Standard Interface:** All connectors implement:
- `name` - Connector identifier (e.g., "github")
- `oauth_config` - OAuth URLs, scopes for external API
- `fetch(credentials)` - Fetch data, return Flux events
- `poll_interval` - Seconds between polls

**Connector responsibilities:**
- Authenticate with external API (using OAuth tokens)
- Fetch data from API
- Transform to Flux events (domain knowledge here)
- Return events to connector manager
- Handle rate limits, errors, pagination

**Connector manager responsibilities:**
- Store encrypted credentials
- Schedule connector polling
- Call connector.fetch() on schedule
- Publish returned events to Flux
- Handle errors, retries, backoff
- Report connector status to UI

**Key principle:** Connectors are stateless (no local state, credentials managed externally).

### OAuth Integration

**One-click flow (user perspective):**
```
1. User opens Flux UI
2. Clicks "Connect GitHub"
3. Redirected to GitHub OAuth
4. Authorizes Flux connector
5. Redirected back to Flux UI
6. Connector automatically starts polling
```

**Technical flow:**
```
1. UI → GET /api/connectors/github/oauth/start
2. Flux → Redirect to GitHub OAuth (with callback URL)
3. User authorizes
4. GitHub → Redirect to Flux callback: /api/connectors/github/oauth/callback?code=...
5. Flux → Exchange code for access token
6. Flux → Store encrypted token
7. Flux → Enable connector
8. Connector Manager → Start polling
```

**OAuth callback URL:** `https://flux.example.com/api/connectors/{connector}/oauth/callback`
- Must be configured in external API (GitHub, Gmail, etc.)
- Single callback per Flux instance (not per user)
- User identified by session (Flux UI login)

**Security:**
- Access tokens stored encrypted (AES-256)
- Refresh tokens handled automatically
- Tokens never exposed via API
- Local deployment only (no cloud access)

### Credential Storage

**Storage backend:** SQLite (encrypted database)

**Data model:** Stores user_id (namespace), connector name, encrypted access/refresh tokens, expiration timestamp. One credential per user per connector.

**Encryption:**
- AES-256-GCM for token encryption
- Key derived from master secret (env var: `FLUX_ENCRYPTION_KEY`)
- Master secret never stored on disk
- Each token encrypted separately (unique nonce)

**Why SQLite:**
- Simple (no external database)
- Encrypted file (SQLCipher extension)
- Atomic updates (ACID)
- Easy backup (single file)

**Not in-memory:** Credentials must survive restarts.

### Connector Manager Service

**Separate binary:** `flux-connector-manager`

**Responsibilities:**
- Load connector plugins (Python modules or Rust crates)
- Schedule polling (tokio intervals)
- Decrypt credentials before passing to connectors
- Call connector.fetch() with credentials
- Publish returned events to Flux HTTP API
- Handle errors, retries, exponential backoff
- Report status metrics (last poll time, error count)

**Configuration:** TOML file specifies:
- Manager settings: polling threads, max retries, backoff duration
- Per-connector: enabled/disabled, poll interval (seconds)

**Deployment:** Docker Compose (separate container from Flux core)

### Core Connectors (Ship with Flux)

**1. GitHub Connector (First)**
- Repos: stars, forks, open issues
- Issues: state, assignee, labels
- PRs: state, reviewers, checks
- Notifications: unread count
- Events: Entity per repo, entity per issue/PR

**2. Gmail Connector**
- Threads: unread, labels, participants
- Labels: message counts
- Events: Entity per thread, entity per label

**3. LinkedIn Connector**
- Connections: count, recent
- Messages: unread count
- Notifications: count
- Events: Entity per connection, aggregate counts

**4. Calendar Connector (Google/Outlook)**
- Events: upcoming, today, this week
- Free/busy status
- Events: Entity per calendar event

**Why these four:**
- High value for "personal life state"
- Diverse data types (repos, messages, events)
- Prove the pattern (if these work, any API works)
- Common OAuth providers (well-documented)

### Community Connectors (SDK)

**Connector SDK:** Python package `flux-connector-sdk`

**Provides:**
- Base `Connector` class (implements standard interface)
- OAuth helpers (exchange code, refresh tokens)
- Flux event builders (transform API data → events)
- Testing utilities (mock credentials, verify events)
- Documentation templates (README, setup guide)

**Connector Marketplace:** (Future Phase)
- Registry of community connectors
- Installation via Flux UI
- Versioning, updates, ratings

---

## Implementation Phases

### Phase 1: Framework Infrastructure

**Goal:** Connector manager + OAuth flow + credential storage

**Tasks:**

1. **Credential Storage (Medium)**
   - Files: `connector-manager/src/credentials/storage.rs`
   - Scope: SQLite schema, encryption (AES-256-GCM), CRUD operations
   - Tests: Store/retrieve/update credentials, encryption round-trip

2. **OAuth Flow (Large)**
   - Files: `flux/src/api/oauth.rs`, UI: `flux-ui/src/components/ConnectorSetup.tsx`
   - Scope: OAuth start endpoint, callback handler, token exchange, UI redirect flow
   - Tests: Mock OAuth provider, test callback handling

3. **Connector Interface (Small)**
   - Files: `connector-manager/src/connector.rs`
   - Scope: Trait definition, OAuthConfig struct, FluxEvent output format
   - Tests: Mock connector implementation

4. **Connector Manager Core (Large)**
   - Files: `connector-manager/src/manager.rs`, `connector-manager/src/scheduler.rs`
   - Scope: Load connectors, schedule polling, decrypt credentials, publish to Flux
   - Tests: Mock connector, verify polling, verify event publishing

5. **Connector Status API (Small)**
   - Files: `flux/src/api/connectors.rs`
   - Scope: GET /api/connectors (list), GET /api/connectors/:name (status)
   - Tests: Verify status reporting

6. **UI Integration (Medium)**
   - Files: `flux-ui/src/pages/Connectors.tsx`
   - Scope: List connectors, enable/disable, OAuth button, status display
   - Tests: E2E test with mock OAuth

**Deliverable:** Working framework, no real connectors yet

### Phase 2: GitHub Connector (First Real Connector)

**Goal:** Prove the pattern with GitHub

**Tasks:**

1. **GitHub OAuth Config (Small)**
   - Files: `connectors/github/config.rs`
   - Scope: OAuth URLs, scopes, API base URL
   - Tests: Config loading

2. **GitHub API Client (Medium)**
   - Files: `connectors/github/api.rs`
   - Scope: Fetch repos, issues, PRs, notifications (REST API)
   - Tests: Mock GitHub API responses

3. **GitHub Event Transformer (Medium)**
   - Files: `connectors/github/transformer.rs`
   - Scope: Transform GitHub data → Flux events (entity_id format, properties)
   - Tests: Verify event structure

4. **GitHub Connector Implementation (Small)**
   - Files: `connectors/github/mod.rs`
   - Scope: Implement Connector trait, wire up API client + transformer
   - Tests: Integration test (mock API → Flux events)

5. **End-to-End Test (Medium)**
   - Files: `tests/integration/github_connector_test.rs`
   - Scope: OAuth flow → Polling → Events in Flux → Query via API
   - Tests: Full workflow with real Flux instance

**Deliverable:** Working GitHub connector, documented pattern

### Phase 3: Additional Core Connectors

**Goal:** Add Gmail, LinkedIn, Calendar connectors

**Tasks:** (Similar structure to Phase 2, per connector)

1. Gmail Connector (4 tasks: config, API client, transformer, tests)
2. LinkedIn Connector (4 tasks: config, API client, transformer, tests)
3. Calendar Connector (4 tasks: config, API client, transformer, tests)

**Deliverable:** Four core connectors shipping with Flux

### Phase 4: Community SDK + Marketplace

**Goal:** Enable community-built connectors

**Tasks:**

1. **Python SDK (Large)**
   - Files: `sdk/python/flux_connector_sdk/`
   - Scope: Base classes, OAuth helpers, event builders, testing utilities
   - Tests: Example connector using SDK

2. **Connector Registry (Medium)**
   - Files: `registry/` (separate repo or Flux UI backend)
   - Scope: List connectors, metadata (name, description, author), versioning
   - Tests: Register/list/search connectors

3. **Dynamic Connector Loading (Medium)**
   - Files: `connector-manager/src/loader.rs`
   - Scope: Load Python modules dynamically, validate interface, error handling
   - Tests: Load valid/invalid connectors

4. **Connector Marketplace UI (Large)**
   - Files: `flux-ui/src/pages/ConnectorMarketplace.tsx`
   - Scope: Browse connectors, install button, version management
   - Tests: E2E install workflow

**Deliverable:** Community can build and share connectors

---

## Technical Decisions

### Language: Python for Connectors (Rust for Manager)

**Why Python for connectors:**
- Mature HTTP libraries (httpx, requests)
- OAuth libraries well-documented (authlib)
- Lower barrier for community contributions
- Fast iteration for API integrations

**Why Rust for connector manager:**
- Performance (scheduling, encryption)
- Reliability (no runtime crashes)
- Integration with Flux core (same ecosystem)

**Interface:** Connector manager calls Python connectors via subprocess or FFI (PyO3).

### Polling vs Webhooks

**Phase 1-3: Polling only**
- Simpler to implement (no webhook server)
- Works for all APIs (even those without webhooks)
- Sufficient for 5-10 minute intervals

**Future (Phase 5):** Webhook support
- Lower latency for real-time updates
- Requires connector to expose HTTP endpoint
- Not all APIs support webhooks (banks, LinkedIn)

### Error Handling & Retries

**Retry strategy:**
- 3 retries with exponential backoff (60s, 120s, 240s)
- After max retries, disable connector (alert user)
- Rate limit errors → backoff 15 minutes
- Auth errors (expired token) → trigger re-auth flow

**Logging:**
- All connector errors logged (structured logs)
- UI displays last error + timestamp
- Metrics: success rate, avg poll duration

---

## Security Model

### Threat Model

**Trust boundary:** User's local Flux instance
- Connectors run on user's machine (not cloud)
- Credentials never leave user's environment
- No central service with access to tokens

**Assumptions:**
- User trusts Flux code (open source, auditable)
- Machine is secured (user's responsibility)
- Network is hostile (HTTPS everywhere)

### Security Properties

**Credential protection:**
- AES-256-GCM encryption at rest
- Never exposed via API (even to UI)
- Master key from environment (not stored)
- Per-token unique nonces (no key reuse)

**OAuth best practices:**
- Authorization code flow (not implicit)
- PKCE where supported (GitHub, Google)
- Minimal scopes (read-only where possible)
- Refresh token rotation (where supported)

**Audit logging:**
- All OAuth authorizations logged
- All connector polls logged (success/failure)
- Credential access logged (who, when, what)
- Logs include user_id (namespace)

### Deployment Recommendations

**Required:**
- Set `FLUX_ENCRYPTION_KEY` (32+ random bytes, base64)
- Use HTTPS for Flux UI (TLS termination)
- Secure OAuth callback URL (register with providers)

**Recommended:**
- Run Flux on trusted network (VPN, internal)
- Backup credentials database (encrypted at rest)
- Monitor connector logs for abuse

**Not recommended:**
- Exposing Flux to public internet without VPN
- Sharing encryption key across instances

---

## Consequences

### Positive

- ✅ Flux becomes "personal life state engine" (unified view of all services)
- ✅ One-click OAuth (non-technical users can set up)
- ✅ Extensible (community can add connectors)
- ✅ Secure (encrypted credentials, local deployment)
- ✅ Flux core unchanged (domain-agnostic maintained)
- ✅ Proven pattern (MCP-like hybrid approach)

### Negative

- ⚠️ Complexity (new service: connector manager)
- ⚠️ Polling latency (5-10 min updates, not real-time)
- ⚠️ OAuth setup required per instance (can't share tokens)
- ⚠️ Credential management (user stores encryption key)
- ⚠️ Rate limits (external APIs limit poll frequency)

### Neutral

- Connectors add domain knowledge (Flux stays generic)
- Each API requires connector development (one-time cost)
- OAuth callback URLs must be registered (admin task)

---

## Future Enhancements (Phase 5+)

**Webhook support:**
- Connector manager exposes HTTP endpoint
- External APIs push updates (lower latency)
- Requires port forwarding or Cloudflare tunnel

**Incremental sync:**
- Track last fetch timestamp per connector
- Only fetch new data (reduce API calls)
- Requires connector state storage

**Multi-instance credential sharing:**
- Sync credentials across Flux instances (encrypted)
- Use case: Desktop + mobile Flux
- Requires secure key exchange protocol

**Advanced connectors:**
- Bidirectional sync (write back to external API)
- Conflict resolution (Flux vs external state)
- Use case: Update GitHub issue from Flux

---

## References

- ADR-001: Flux State Engine Architecture (domain-agnostic principle)
- ADR-003: Multi-tenancy and Authentication (namespace model, token patterns)
- [Anthropic MCP](https://modelcontextprotocol.io/) - Similar hybrid approach
- [OAuth 2.0 RFC 6749](https://tools.ietf.org/html/rfc6749) - Authorization code flow
- [PKCE RFC 7636](https://tools.ietf.org/html/rfc7636) - OAuth security extension

---

## Next Steps

1. Review and approve ADR-005
2. Implement Phase 1 (framework infrastructure)
3. Register OAuth apps with GitHub (test instance)
4. Implement Phase 2 (GitHub connector)
5. Document connector development guide
6. Plan Phase 3 (additional core connectors)
