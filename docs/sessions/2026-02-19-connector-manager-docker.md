# Session: Connector Manager Docker Compose Integration

**Date:** 2026-02-19
**Status:** Complete

## What Was Done

Added Docker support for the connector-manager service.

### Files Created/Modified

- `connector-manager/Dockerfile` — new, multi-stage build
- `docker-compose.yml` — added `connector-manager` service

## Key Decision: Build Context

The connector-manager crate depends on the root flux crate via `{ path = "../" }` in its Cargo.toml. This means the Docker build context must be the **repo root** (`.`), not `./connector-manager`. The Dockerfile copies both:
- Root flux crate: `Cargo.toml`, `Cargo.lock`, `src/`
- connector-manager crate: `connector-manager/Cargo.toml`, `connector-manager/Cargo.lock`, `connector-manager/src/`

The binary output is at `/app/connector-manager/target/release/connector-manager`.

## docker-compose.yml Service Config

```yaml
connector-manager:
  build:
    context: .
    dockerfile: connector-manager/Dockerfile
  environment:
    - FLUX_API_URL=http://flux:3000
    - FLUX_ENCRYPTION_KEY=${FLUX_ENCRYPTION_KEY}
    - FLUX_CREDENTIALS_DB=/data/credentials.db
  volumes:
    - ./data:/data
  depends_on:
    - flux
  restart: unless-stopped
  resources: 0.5 CPU / 256MB RAM
```

Shares the `./data` volume with the `flux` service so `credentials.db` is accessible to both.

## Test Commands

```bash
docker compose build connector-manager
docker compose up -d connector-manager
docker compose logs connector-manager
```

Expected startup logs:
```
Connector Manager starting...
Configuration loaded flux_api_url="http://flux:3000" credentials_db="/data/credentials.db"
Credential store initialized
Connector manager started schedulers_started=0
```

Note: `schedulers_started=0` is expected at startup — schedulers start on-demand when OAuth credentials are configured.
