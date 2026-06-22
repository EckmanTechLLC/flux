# ADR-010: SingularisPrime-Inspired Messaging Primitives

**Status:** Proposed  
**Date:** 2026-03-05  
**Author:** Kannaka (via Nick Flach)  
**Origin:** [SingularisPrime](https://github.com/NickFlach/SingularisPrime) substrate spec

## Context

Flux's current WebSocket subscription model is a firehose: clients subscribe to `*` or individual entity IDs, and every state update is serialized and transmitted as a full JSON message. For dashboards with dozens of entities updating every second, this creates unnecessary bandwidth and CPU overhead.

SingularisPrime defines a cognitive substrate with efficient event primitives — prefix subscriptions, QoS tiers, priority lanes, and compact binary framing. These concepts map directly onto Flux's existing architecture (Tokio broadcast channels, NATS JetStream, DashMap state) without requiring a full runtime port.

## Decision

Implement three SingularisPrime-inspired messaging upgrades:

### 1. Prefix Subscriptions (SP: `event.subscribe(prefix, filter)`)

**Current:** Client sends `{"type":"subscribe","entity_id":"*"}` or a single entity ID.

**New:** Support prefix-glob patterns and property filters:

```json
{"type":"subscribe","entity_id":"pure-jade/scada-*","properties":["status","current"]}
```

- `entity_id` supports trailing `*` for prefix matching
- Optional `properties` array limits which property updates are forwarded
- Reduces message volume: a client watching 6 SCADA entities but only caring about `status` gets ~80% fewer messages

### 2. QoS Tiers (SP: `QoS { best_effort, at_least_once, exactly_once }`)

**Current:** All WebSocket messages are fire-and-forget. If a client lags, `broadcast::Receiver` skips messages silently.

**New:** Per-subscription QoS hint:

```json
{"type":"subscribe","entity_id":"pure-jade/scada-*","qos":"reliable"}
```

| Tier | Behavior |
|------|----------|
| `realtime` (default) | Current behavior — drop on lag, lowest latency |
| `reliable` | Buffer up to N messages during lag, send catch-up batch on reconnect |
| `snapshot` | Only send latest state on reconnect, no streaming (poll mode) |

Maps to NATS JetStream semantics Flux already uses: `realtime` = ephemeral push, `reliable` = durable consumer with ack, `snapshot` = last-value-cache.

### 3. Delta Compression (SP: `StateHandle` minimal writes)

**Current:** Every property change sends the full value:
```json
{"type":"state_update","entity_id":"x","property":"current","value":2041,"timestamp":"..."}
```

**New:** For numeric properties, optionally send deltas:
```json
{"type":"state_delta","entity_id":"x","property":"current","delta":-12,"timestamp":"..."}
```

Client opts in via subscribe message:
```json
{"type":"subscribe","entity_id":"pure-jade/scada-*","delta":true}
```

For SCADA telemetry where values change by small amounts (voltage 67→55), deltas are 40-60% smaller on the wire.

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `src/subscription/protocol.rs` | Extended `ClientMessage` with prefix, properties, qos, delta fields |
| `src/subscription/manager.rs` | Prefix matching, property filtering, delta tracking, QoS buffering |
| `src/state/engine.rs` | No changes — broadcast channel unchanged |
| `src/api/websocket.rs` | No changes — upgrade handler unchanged |

### Wire Protocol (Backward Compatible)

Existing clients sending `{"type":"subscribe","entity_id":"*"}` continue to work identically. New fields are optional — zero breaking changes.

### Data Efficiency Gains

| Scenario | Current msg/s | With prefix+property filter | Reduction |
|----------|---------------|----------------------------|-----------|
| 50 entities, 5 props each, 1 client watching 10 entities, 2 props | 250 | 20 | 92% |
| 6 SCADA assets, all props, 3 dashboard clients | 54 | 54 | 0% (no filter) |
| 6 SCADA + 200 ships + 50 aircraft, 1 SCADA-only client | 2560 | 60 | 97% |

## Consequences

- **Positive:** Dramatic bandwidth reduction for filtered clients, NATS-backed reliability for critical alerts, smaller payloads for telemetry
- **Positive:** Zero breaking changes — pure additive protocol extension
- **Negative:** Per-connection state increases (property filter sets, delta tracking, QoS buffers)
- **Negative:** Delta mode requires clients to maintain running state (not suitable for stateless consumers)

## References

- SingularisPrime substrate spec: `substrate/spec.sp`
- SingularisPrime lowering rules: `substrate/lowering.md`
- Flux subscription protocol: `src/subscription/protocol.rs`
