# ADR-007: Universal Connector Framework

**Status:** Proposed
**Date:** 2026-02-20
**Supersedes:** ADR-005 Phase 3+ (Gmail, LinkedIn, Calendar, Community SDK)

---

## Context

ADR-005 established a connector framework with hand-built Rust connectors. Phase 1
(framework infrastructure) and Phase 2 (GitHub connector) are complete and validated
in production.

**The problem with continuing to build hand-built connectors:**

- Each new connector requires 4+ Rust tasks (OAuth config, API client, transformer,
  tests), recompile, redeploy
- Phase 3 alone (Gmail, LinkedIn, Calendar) = ~12 tasks of bespoke integration work
- "Community SDK" (Phase 4) requires building a marketplace from scratch
- We'd be reinventing two mature ecosystems

**What already exists:**

**Bento** (MIT fork of Benthos):
- Single Go binary, zero runtime dependencies
- Config-driven: polls ANY HTTP URL on a schedule, transforms JSON, POSTs to HTTP
- Built-in processors: `http_client`, `jq` transforms, scheduling
- Covers any REST API without code: RSS, stocks, crypto, weather, custom endpoints
- No connector-specific code to write or maintain

**Singer taps** (singer.io / Meltano Hub):
- 300+ standalone Python CLI scripts for named APIs
- Covers: GitHub, Gmail (via Google APIs), CoinGecko, Alpha Vantage, Stripe, Salesforce, etc.
- MIT licensed, run as subprocesses via `stdin`/`stdout` JSON protocol
- Installation: `pip install tap-github`, `tap-coinbase`, etc.

**User requirement:** ALL configuration through Flux UI. No YAML files, no CLI.

---

## Decision

Extend the connector-manager to support two new connector types powered by
Bento and Singer taps, in addition to the existing hand-built Rust trait connectors.

### Three Connector Types

| Type | Backend | Use Case | Config |
|------|---------|----------|--------|
| **Built-in** | Rust trait (existing) | Core integrations (GitHub) | Code |
| **Generic** | Bento subprocess | Any HTTP URL/API | UI form → Bento config |
| **Named** | Singer tap subprocess | 300+ named APIs | UI catalog → tap config |

---

## Architecture

```
Flux UI
  ├── Add Generic Source → URL, interval, field mappings
  │       ↓ POST /api/connectors/generic
  ├── Add Named Source  → pick tap, fill credentials
  │       ↓ POST /api/connectors/named/:tap
  └── Status panel ← GET /api/connectors/status

Connector Manager
  ├── Built-in connectors (existing Rust trait, unchanged)
  ├── Generic runner: renders Bento config → spawns bento process
  └── Named runner: installs tap if missing → spawns tap process
          ↓ (stdout JSON → parse → Flux events)
          POST http://flux:3000/api/events
```

The connector-manager remains the single orchestrator. Bento and Singer processes
are subprocesses it starts, monitors, and kills. No new services.

---

## Generic Connectors (Bento)

**What the user configures in the UI:**
- Source URL (e.g., `https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd`)
- Poll interval (seconds)
- Namespace + entity key (how to map response to Flux entity ID)
- Field mappings (optional): which JSON fields become entity properties
- Auth: none / Bearer token / API key header

**What Flux does:**
1. Stores config in SQLite (alongside credential store)
2. Connector-manager renders a Bento YAML config from the template
3. Spawns `bento -c /tmp/flux-bento-{id}.yaml` as subprocess
4. Bento polls URL, transforms, POSTs events to Flux ingestion endpoint
5. Manager monitors process, restarts on crash, captures stderr for status

**Bento config template (Flux generates, user never sees):**
```yaml
input:
  http_client:
    url: ${URL}
    verb: GET
    headers:
      Authorization: "${AUTH_HEADER}"
    rate_limit: ""
    timeout: 30s

pipeline:
  processors:
    - bloblang: |
        root.stream = "generic/${SOURCE_ID}"
        root.source = "bento"
        root.key = "${ENTITY_KEY_EXPR}"
        root.payload = this

output:
  http_client:
    url: http://flux:3000/api/events
    verb: POST
    headers:
      Content-Type: application/json
      Authorization: "Bearer ${FLUX_TOKEN}"
```

**No Bento expertise required from user.** UI generates valid config.

---

## Webhook / Push Sources (Bento)

Bento can also receive inbound HTTP requests, flipping the model from
polling to push for APIs that support webhooks.

**Use cases:** GitHub webhooks, Stripe events, Shopify orders — any API
that can POST to a URL on change.

**What the user configures in the UI:**
- Source name and type (Generic → Webhook)
- Flux generates a stable webhook URL: `https://flux.example.com/webhooks/{source-id}`
- User pastes this URL into the external service's webhook settings
- Optional: shared secret for payload signature verification

**What Flux does:**
- Connector-manager spawns a Bento process with `http_server` input
- Bento listens on a per-source port (internal only, proxied by Flux)
- Flux API proxies `POST /webhooks/{source-id}` to the Bento process
- Bento transforms payload → Flux event → POSTs to ingestion endpoint

**Benefit:** Real-time ingestion instead of 5-minute polling. No polling
overhead for high-frequency sources.

**Phase:** Included in Phase 3C (polish). Polling generic sources ship first.

---

## Named Connectors (Singer Taps)

**What the user configures in the UI:**
- Pick from catalog (populated from Meltano Hub JSON at startup)
- Fill in credentials form (rendered from tap's `--discover` schema)
- Optionally: select which streams to sync

**What Flux does:**
1. On first use: `pip install tap-{name}` (or checks if installed)
2. Runs `tap-{name} --config /tmp/flux-tap-{id}-config.json` as subprocess
3. Reads Singer JSON records from stdout
4. Transforms Singer `RECORD` messages → Flux events
5. Publishes to Flux ingestion endpoint
6. Runs on configured schedule (tap exits after sync, manager reschedules)

**Singer → Flux mapping:**
- Singer stream name → Flux stream (`taps/{tap_name}/{stream}`)
- Singer `RECORD.value` → Flux entity `key` (configurable per tap)
- Singer `RECORD.value` properties → Flux entity properties
- Singer `STATE` messages → stored for incremental sync

**Catalog:** At startup, connector-manager fetches the Meltano Hub tap list
(cached locally). UI displays name, description, logo, and credential fields.

---

## UI Configuration Model

**No YAML is ever shown to users.** The UI owns the configuration surface.

**Generic source form:**
```
Source Name:    [Bitcoin Price        ]
URL:            [https://api.coingecko.com/...]
Poll Every:     [300] seconds
Entity Key:     [bitcoin              ]  ← fixed string or JSON path
Namespace:      [personal             ]
Auth Type:      [None ▾]
[ Add Source ]
```

**Named source form:**
```
Select a Source: [ GitHub ▾ ]

GitHub Personal Access Token:
  [ ghp_...                            ]

Streams to sync:
  [x] repositories   [x] issues   [ ] pull_requests

Namespace: [personal]
[ Connect ]
```

**Connector status panel** (existing, extended):
- Shows all three connector types in one list
- Last sync time, entity count, error (if any)
- Enable/disable toggle
- Delete credentials button

---

## Credential Flow

**Generic connectors:** Bearer token / API key stored in existing encrypted
credential store under `generic/{source-id}`. Connector-manager decrypts and
passes to Bento config as environment variable (not written to config file).

**Named connectors:** Tap config JSON written to temp file (`/tmp/flux-tap-{id}-config.json`)
with `0600` permissions, deleted after process exits. Sensitive fields (tokens,
passwords) pulled from encrypted credential store at runtime.

**Bento processes:** Auth headers injected via environment variables, not the
generated config file. Config files are safe to log.

---

## Process Orchestration

Connector-manager manages subprocess lifecycle:

```
start(config) → spawn subprocess → monitor stdout/stderr
                     ↓
              on exit (Singer): reschedule after interval
              on crash: exponential backoff, update status, alert UI
              on disable: kill subprocess, remove config/temp files
```

**Resource limits:** Max concurrent subprocesses configurable (default: 10).
Bento processes run continuously (long-lived). Singer taps run to completion
and exit (short-lived, restarted on schedule).

**Status tracking:** Existing `ConnectorStatus` struct extended with:
- `connector_type`: `builtin | generic | named`
- `source_id`: unique ID for this configured source
- `last_sync_count`: entities updated in last poll

---

## Status Reporting

Existing `/api/connectors` API extended:

```
GET /api/connectors
→ [
    { name: "github", type: "builtin", enabled: true, status: "running", last_poll: "...", entity_count: 47 },
    { name: "bitcoin-price", type: "generic", enabled: true, status: "running", last_poll: "...", entity_count: 1 },
    { name: "tap-coinbase", type: "named", enabled: true, status: "scheduled", next_poll: "...", entity_count: 12 }
  ]

POST /api/connectors/generic        → add generic source (returns source_id)
POST /api/connectors/named/:tap     → add named tap source
DELETE /api/connectors/:id          → remove source + credentials
```

---

## GitHub Connector Migration

The existing hand-built GitHub connector (Phase 2, validated in production) is
**kept as-is**. It becomes the reference implementation of the built-in connector
type.

**Rationale:** It works, it's tested, it runs in production with real data.
`tap-github` is an option but provides no benefit over what's already running.

**No migration required.** The three connector types coexist in the manager.

If `tap-github` becomes useful (more streams, community maintained, etc.),
it can be offered as a Named connector alongside the built-in. User chooses.

---

## Implementation Phases

### Phase 3A: Generic Connector (Bento)

1. **Config storage** — SQLite table for generic source config (URL, interval,
   mappings, auth type). Extend credential store for generic tokens.
2. **Bento runner** — `GenericRunner`: render config template, spawn bento,
   monitor process, capture stderr for error status.
3. **API endpoints** — `POST /api/connectors/generic`, `DELETE /api/connectors/:id`
4. **UI form** — Add Generic Source form, status display

**Deliverable:** User can add any HTTP URL as a data source via UI.

### Phase 3B: Named Connector (Singer)

1. **Tap catalog** — Fetch + cache Meltano Hub tap list, render credential schemas
2. **Singer runner** — `NamedRunner`: install tap, write temp config, spawn,
   parse stdout (RECORD/STATE/SCHEMA messages), publish to Flux
3. **State management** — Persist Singer STATE between runs (incremental sync)
4. **API endpoints** — `POST /api/connectors/named/:tap`, catalog endpoint
5. **UI catalog** — Browse taps, fill credentials form, stream selection

**Deliverable:** User can connect 300+ named APIs via UI tap catalog.

### Phase 3C: Polish

1. Error alerting — UI notification when connector fails repeatedly
2. Manual trigger — "Sync now" button in UI
3. Resource limits — Max subprocess count, configurable per deployment
4. Documentation — User guide for adding generic and named sources

---

## Consequences

### Positive

- No custom connector code for Gmail, LinkedIn, Calendar, or most APIs
- 300+ sources available immediately (Singer ecosystem)
- Generic type covers anything with a URL
- UI-only configuration (zero YAML exposure)
- Existing GitHub connector unaffected
- Community can use Singer taps directly — no marketplace to build

### Negative

- Bento binary dependency (single Go binary, ~50MB, easy to ship)
- Singer requires Python + pip at runtime (already present on most servers)
- Tap quality varies across Singer ecosystem (community-maintained)
- Singer taps exit and restart (not long-lived like built-in connectors)
- Incremental sync state management adds complexity (Phase 3B)

### Neutral

- ADR-005 Phase 3 (hand-built Gmail/LinkedIn/Calendar) is superseded
- ADR-005 Phase 4 (community SDK + marketplace) is superseded
- Built-in connector trait remains available for future high-value integrations

---

## References

- ADR-005: Connector Framework (built-in connector type, credential store, GitHub)
- [Bento](https://github.com/warpstreamlabs/bento) — MIT fork of Benthos
- [Meltano Hub](https://hub.meltano.com/taps/) — Singer tap catalog
- [Singer Spec](https://github.com/singer-io/getting-started) — stdout JSON protocol
- [Bloblang](https://www.benthos.dev/docs/guides/bloblang/about) — Bento mapping language
