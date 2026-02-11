# Flux

**Persistent, shared, event-sourced world state engine**

Flux ingests immutable events, derives live in-memory state from them, and exposes that evolving world to agents, services, and humans through subscriptions and replay.

## What Flux Is

**Flux is a state engine, not just an event log.**

- **Event-sourced:** State is derived from immutable events
- **Persistent:** Events stored, state survives restarts
- **Shared:** Multiple systems observe the same world state
- **Real-time:** Updates propagate immediately to subscribers
- **Replay-capable:** Can reprocess history from any point
- **Domain-agnostic:** Works for any use case without encoding domain semantics

**Critical distinction:** Flux owns state derivation and persistence semantics. Consumers receive state updates from Flux, not raw events.

## Architecture

```
Producer → Event Ingestion → NATS (internal) → State Engine → WebSocket API → Consumers
```

Consumers observe Flux's canonical state. They never see raw events.

## Use Cases (Domain-Agnostic)

Flux is infrastructure that works for any domain:

- **Multi-agent LLM systems:** Agents coordinate through shared state
- **Industrial SCADA:** Real-time equipment state
- **Virtual worlds/games:** Shared game state, time-travel debugging
- **IoT platforms:** Device state aggregation
- **Collaborative systems:** Real-time document/project state

## Status

**Fresh start:** Building state engine from scratch using flux-reactor patterns.

Previous work (event backbone approach) archived in `archive/event-backbone` branch.

## Documentation

- `FLUX-DESIGN.md` - Complete vision and design principles
- `CLAUDE.md` - Development context for Claude Code
- `docs/workflow/` - Multi-session development workflow

## Technology

- **State Engine:** Rust (performance, safety, no GC pauses)
- **Event Transport (Internal):** NATS with JetStream
- **APIs:** Rust with Axum (WebSocket + HTTP REST)
- **Deployment:** Docker Compose

## Quick Start

*Coming soon - state engine implementation in progress*

## License

*To be determined*
