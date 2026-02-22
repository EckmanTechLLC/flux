# Session: Event Replay API + UI

**Date:** 2026-02-22
**Status:** Complete

## What Was Done

Implemented `GET /api/events` endpoint for fetching raw stored events from NATS JetStream, plus a UI History panel per entity.

## Files Created/Modified

- **CREATE** `src/api/history.rs` — `HistoryAppState`, `create_history_router`, `get_events` handler
- **MODIFY** `src/api/mod.rs` — added `pub mod history`, re-exported `create_history_router`/`HistoryAppState`
- **MODIFY** `src/main.rs` — wired `HistoryAppState` and `create_history_router` into merged router
- **MODIFY** `Cargo.toml` — added `time = "0.3"` (needed for `time::OffsetDateTime` in NATS `DeliverPolicy::ByStartTime`)
- **MODIFY** `ui/public/index.html` — History button on each entity row, modal with scrollable event list, JS functions
- **CREATE** `docs/sessions/2026-02-22-replay-api.md` — this file

## API

```
GET /api/events?entity=X&since=T&limit=N
```

- `entity` (required): entity ID, e.g. `crypto/bitcoin`
- `since` (optional): ISO 8601, default 24h ago
- `limit` (optional): max events returned, default 100, max 500
- Returns: JSON array of `FluxEvent` objects, newest first
- No auth required

## Implementation Notes

- Uses `async_nats::jetstream::consumer::ordered::Config` with `DeliverPolicy::ByStartTime`
- 200ms idle timeout stops message iteration when stream is caught up
- Entity filtering done client-side (filter by `payload.entity_id`)
- `time::OffsetDateTime` conversion from `chrono::DateTime<Utc>` via `from_unix_timestamp()`
- 4 unit tests for param parsing logic

## Rebuild Required

```
docker compose build --no-cache flux && docker compose up -d flux
docker compose build --no-cache flux-ui && docker compose up -d flux-ui
```
