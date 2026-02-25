# ADR-009: Stripe Provisioner — Automated Namespace Sales

**Date:** 2026-02-25
**Status:** Approved
**Deciders:** etl

---

## Problem

Flux namespaces are sold via flux-universe.com for $4.99 one-time. Currently
provisioning is manual (admin curl commands). This doesn't scale and creates
lag between payment and access. Need fully automated provisioning triggered
by successful Stripe payment.

---

## Decision

Build a standalone Python service (`flux-provisioner`) that handles Stripe
webhooks, provisions namespaces via the Flux admin API, stores customer
records, and emails credentials. Admin dashboard for manual management.

---

## Architecture

### Service: `flux-provisioner`
- **Language:** Python + Flask
- **Location:** `/home/etl/flux-provisioner/` on .107 (private, no git repo)
- **Port:** 3002
- **Process manager:** systemd (`flux-provisioner.service`)
- **Public URL:** `pay.flux-universe.com` → Cloudflare tunnel → localhost:3002

### Cloudflare Tunnel
Add to `/etc/cloudflared/config.yml` (before the catch-all):
```yaml
- hostname: pay.flux-universe.com
  service: http://localhost:3002
```
Restart cloudflared after change.

### Namespace Naming
Word-pair format: `adjective-noun` (e.g., `amber-river`, `swift-pine`).
~50 adjectives × ~50 nouns = 2500 combinations. Retry on collision.

### Email Provider
SMTP2GO — `mail.smtp2go.com:587` with STARTTLS.
Credentials in environment variables (`SMTP2GO_USER`, `SMTP2GO_PASS`).

### Storage
SQLite at `/home/etl/flux-provisioner/provisioner.db`:
```sql
CREATE TABLE namespaces (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    stripe_session_id TEXT UNIQUE NOT NULL,
    email           TEXT NOT NULL,
    namespace_name  TEXT NOT NULL,
    namespace_id    TEXT NOT NULL,
    token           TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active'  -- active | revoked
);
```

---

## Provisioning Flow

```
Stripe checkout.session.completed
    → verify webhook signature (STRIPE_WEBHOOK_SECRET)
    → extract customer email
    → generate word-pair namespace name
    → check uniqueness: GET /api/namespaces/{name} on api.flux-universe.com
    → retry with new name if taken
    → POST /api/namespaces with FLUX_ADMIN_TOKEN
    → store record in SQLite
    → send email via SMTP2GO
```

---

## API Endpoints

### Provisioner (`pay.flux-universe.com`)
- `POST /webhook` — Stripe webhook handler (public)

### Admin Panel (`http://192.168.50.107:3002/admin`)
- `GET /admin` — HTML dashboard (requires `?token=FLUX_ADMIN_TOKEN`)
- `GET /admin/api/namespaces` — JSON list of all provisioned namespaces
- `POST /admin/api/namespaces` — manually create namespace (name + email)
- `POST /admin/api/namespaces/:name/revoke` — revoke namespace

---

## Flux Changes Required

`DELETE /api/namespaces/:name` endpoint (gated by `FLUX_ADMIN_TOKEN`).
Removes namespace from registry and SQLite store.
Used by provisioner admin panel to actually revoke access.

---

## Environment Variables

On .107, stored in `/home/etl/flux-provisioner/.env`:

| Variable | Description |
|----------|-------------|
| `STRIPE_WEBHOOK_SECRET` | Stripe webhook signing secret |
| `FLUX_ADMIN_TOKEN` | Same token as Flux instance |
| `FLUX_API_URL` | `http://localhost:3000` |
| `SMTP2GO_USER` | SMTP2GO username |
| `SMTP2GO_PASS` | SMTP2GO password |
| `ADMIN_TOKEN` | Token to access /admin panel (reuse FLUX_ADMIN_TOKEN) |
| `DOCS_URL` | `https://flux-universe.com/docs` |

---

## Implementation Tasks

### Task 1: Flux — DELETE namespace endpoint
File: `src/api/namespace.rs`
- Add `DELETE /api/namespaces/:name` handler
- Gate behind `FLUX_ADMIN_TOKEN` (same pattern as namespace registration)
- Remove from `NamespaceRegistry` and `NamespaceStore` (SQLite)
- Add `delete()` method to `NamespaceStore`
- Tests: missing token → 401, wrong token → 401, unknown name → 404, valid → 204

### Task 2: Provisioner core
File: `/home/etl/flux-provisioner/app.py`
- Flask app skeleton
- Word-pair generator (embedded wordlists, ~50×50)
- `POST /webhook`: verify Stripe sig, provision, store, send email
- SQLite init and CRUD helpers
- SMTP2GO email sender (smtplib + STARTTLS)
- systemd service file: `flux-provisioner.service`
- `.env` file template

### Task 3: Admin panel
File: `/home/etl/flux-provisioner/app.py` (continued) + `templates/admin.html`
- `GET /admin` — token-gated HTML page
- `GET /admin/api/namespaces` — JSON list from SQLite
- `POST /admin/api/namespaces` — manual provision (name + email)
- `POST /admin/api/namespaces/:name/revoke` — call Flux DELETE + mark SQLite revoked
- Simple HTML table: namespace, email, created, status, revoke button

### Task 4: Cloudflare + Stripe wiring
- Add `pay.flux-universe.com` ingress to `/etc/cloudflared/config.yml`
- Restart cloudflared
- Create Stripe webhook in dashboard pointing to `https://pay.flux-universe.com/webhook`
- Copy signing secret to `.env`
- End-to-end test with Stripe test mode

---

## Rejected Alternatives

**Add provisioner to Flux repo:** Public repo is wrong place for Stripe secrets
and customer PII (email addresses). Private service on .107 is cleaner.

**Admin panel in Flux UI (8082):** Flux UI runs in Docker; provisioner runs on
host. Docker container can't cleanly reach host-side service without network
complexity. Provisioner has full customer data (email + payment), Flux UI doesn't.
Simpler to serve admin HTML directly from Flask.

**Stripe Payment Links without webhook:** No way to auto-provision without a
webhook. Payment Links alone just take money.

**PostgreSQL for storage:** No scale requirement. SQLite is sufficient and
keeps the service dependency-free.
