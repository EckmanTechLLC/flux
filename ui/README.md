# Flux Monitor (Ratzilla)

Terminal-themed real-time Flux dashboard built with [Ratzilla](https://github.com/ratatui/ratzilla) (Ratatui + WASM).

## Features

- **Entity List** — all entities sorted by last updated, color-coded staleness (green <60s, yellow <5min, red >5min)
- **Entity Detail** — full property view for selected entity
- **Agent Messages** — chat-like view of entities with `message` + `message_to` properties
- **Metrics Bar** — real-time events/sec, entity count, publishers, WebSocket connections
- **Keyboard Navigation** — ↑↓/jk to browse entities, Tab to switch panels

## Prerequisites

```sh
# Install trunk (WASM build tool)
cargo install --locked trunk

# Add WASM target
rustup target add wasm32-unknown-unknown
```

## Development

```sh
# Serve with hot reload (default: http://localhost:8080)
trunk serve
```

By default, it connects to the same host it's served from (proxied through Flux's UI server or directly).

### Pointing at a specific Flux instance

The app uses `window.location` to derive the API base URL. To proxy to a remote Flux during dev, you can use trunk's proxy feature or run behind the existing `ui/server.js` proxy.

## Build for Production

```sh
trunk build --release
```

Output goes to `dist/`. Serve with any static file server, or integrate with Flux's existing UI server.

## Architecture

```
┌──────────────────────────────────────────────┐
│ ⟁ Flux Monitor                    ● LIVE     │
├──────────────────────┬───────────────────────┤
│                      │                       │
│  Entity List         │  Entity Detail        │
│  ● entity-01    2s   │  ID: entity-01        │
│  ● entity-02   15s   │  status: active       │
│  ● entity-03    4m   │  temp: 22.5           │
│                      ├───────────────────────┤
│                      │  Agent Messages        │
│                      │  agent-01 → agent-02  │
│                      │  Hello world           │
├──────────────────────┴───────────────────────┤
│ ⚡ 45.2 evt/s │ ◈ 1543 entities │ ⇅ 12 pub  │
│ ↑↓ navigate   Tab switch panel               │
└──────────────────────────────────────────────┘
```

## WebSocket Protocol

On startup:
1. HTTP GET `/api/state/entities` → load initial state
2. WebSocket `/api/ws` → subscribe `{"type": "subscribe", "entity_id": "*"}`
3. Receive `state_update`, `metrics_update`, `entity_deleted` messages
