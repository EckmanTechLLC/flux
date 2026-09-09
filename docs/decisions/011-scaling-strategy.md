# ADR-011: Scaling Strategy

**Status:** Proposed
**Date:** 2026-05-09
**Extends:** ADR-001 (State Engine Architecture), ADR-002 (Persistence and Recovery)

---

## Background

Flux's public instance on .107 has grown organically to ~110,000 entities across 21 namespaces. Live data sources (aviation, ships, earthquakes, crypto, weather, stocks, commodities, economic, energy, internet, ISS) are stable and the system is healthy, but two forms of pressure are building:

1. **Artificial container limits** in `docker-compose.yml` (flux: 512 MB / 1 CPU) cap throughput well below VM capacity (32 GB / 4 CPU)
2. **No formal trigger conditions** for the next architectural decision — eviction, sharding, or tiered storage were deferred at MVP ("YAGNI") with no documented re-evaluation criteria

The intent of this ADR is **not** to set a target entity count. Flux is general-purpose infrastructure; we cannot predict adoption. The intent is to define **trigger conditions** that force the next architectural decision when crossed, and to record the operational policies that govern current scaling.

## Architectural premise (unchanged)

Flux remains an in-memory state engine over a NATS-backed event log:
- State lives in a `DashMap` (RAM-bound, lock-free reads)
- Events are persisted in NATS JetStream
- State is recovered at startup from snapshot + replay

This ADR does NOT change that model. It defines when we would change it.

## Decisions

### 1. Container resource limits are operational, not architectural

The current limits (flux: 512 MB / 1 CPU) were never deliberate scaling decisions — they were defaults from the MVP compose file. Removing or raising them is config, not architecture. Container limits should provide ample headroom over measured usage; tightening them is only justified by host pressure.

**Policy:** Container limits scale with host resources. Treat them as guardrails, not capacity planning.

### 2. NATS retention is bounded by storage size, not time

Current NATS state: 6.97M messages, 2.96 GB on disk, ~7 days of retention naturally produced by current event rate. Max storage allowed: 43 GB.

**Policy:** Continue with size-based retention. Do not introduce time-based expiry. The 7-day natural window is more than sufficient overlap with the 5-minute snapshot cadence to guarantee recovery from any planned or unplanned restart.

### 3. Snapshot cadence stays at 5 minutes until entity count or replay time triggers a change

5-minute snapshots at 110k entities produce 7.8 MB compressed snapshots. Replay of ~7,500 events is sub-second.

**Policy:** No change to cadence at current scale. Revisit only when a trigger below fires.

### 4. Trigger conditions

The following are the **explicit conditions** under which this ADR is reopened and a follow-up ADR drafted. Crossing any one of them is a signal, not an emergency.

| Trigger | Threshold | Likely follow-up |
|---|---|---|
| Steady-state flux container memory | > 70% of container limit | Raise container limit (operational) |
| Steady-state flux container memory at host RAM ceiling | VM cannot accommodate increase | Bump VM RAM; if host is also constrained, draft sharding ADR |
| Cold-start replay time | > 60 seconds | Draft snapshot tuning ADR (more frequent snapshots, parallel replay) |
| Single-namespace entity count | > 1,000,000 | Evaluate per-namespace eviction policy |
| NATS storage growth | > 80% of `max_storage` (43 GB) for 24 h | Tune retention or grow NATS volume |
| WebSocket subscriber count | > 1,000 concurrent | Evaluate subscription manager fanout architecture |
| 99p event-ingest latency | > 100 ms | Profile ingestion path; consider partitioning |
| Total entity count | > 5,000,000 | Mandatory architecture review (eviction vs. sharding vs. tiered storage) |

These are **trigger thresholds**, not targets. No work is performed until one is crossed.

### 5. Monitoring requirement

Trigger-based scaling only works if triggers are observed. The flux-monitor service (already running on .107) must surface enough metrics to detect each trigger. If it does not, a follow-up task expands its coverage.

**Policy:** Every trigger in the table above must be observable from monitoring. Triggers that cannot be measured are not triggers.

### 6. Log rotation is mandatory for all containers

Container logs on .107 grew to ~20 GB unbounded. This was avoidable.

**Policy:** Every service in every compose file must declare `logging:` with `max-size` and `max-file` limits.

## Constraints (what NOT to do)

- **Do not introduce eviction.** Flux's value proposition is that state persists. Eviction breaks that contract and must only be considered after a trigger fires and only with a dedicated ADR.
- **Do not shard preemptively.** ADR-001 explicitly chose YAGNI on sharding. That choice stands until a trigger forces revisiting it.
- **Do not introduce tiered storage** (hot in RAM, warm on disk, cold archived). Current architecture is one tier. Adding tiers is a major ADR, not an optimization.
- **Do not change the event model** (envelope, schema, opaque payload) for performance reasons. Domain-agnostic design is non-negotiable.
- **Do not couple flux to a specific deployment topology** (single VM vs. cluster). Container limits and snapshot cadence are tunable; the architecture is not.
- **Do not expose NATS to consumers** even if it would simplify high-volume use cases.

## Operational notes

- Pre-flight before any restart: verify a snapshot exists < 5 min old, capture entity count baseline, back up `data/snapshots/` and the `nats-data` Docker volume.
- After any restart: confirm entity count matches baseline; if not, restore from backup and investigate.
- Quick wins captured in Task 01 (container limits + log rotation) implement decisions 1 and 6 above.

## Amendment — 2026-09-09

Two gaps found the hard way during the reliability audit.

### A1. Disk is a trigger, and it is coupled to event retention

The trigger table had no disk row. It should have: **JetStream derives `max_storage`
from free disk at startup**, so disk pressure silently shrinks the event-retention
window, and past a threshold prevents the stream from loading at all.

On 2026-09-09 the VM reached 92% (48 GB of it an unrelated project, 10 GB a runaway
syslog). `max_storage` fell from the 43 GB recorded in this ADR to **9.30 GB** — below
the stream's own 10 GB `max_bytes` — and NATS refused to restore `FLUX_EVENTS`:

```
Error recreating stream "FLUX_EVENTS": insufficient storage resources available (10047)
```

Flux crash-looped and the public API returned 502 until disk was freed. No data was
lost, but **any** NATS restart past that threshold would have done it, unattended and
at any hour.

| Trigger | Threshold | Follow-up |
|---|---|---|
| Root filesystem usage | > 80% | Reclaim space; identify the consumer before assuming it is Flux |
| Free disk vs. stream ceiling | `max_storage` < 2× `max_bytes` | Free disk or lower `FLUX_NATS_MAX_BYTES` |

Addressed in code (commit `7c07908`): `max_bytes` default lowered 10 GB → 5 GB,
`ensure_stream` now reconciles config on an existing stream rather than returning
early, and a startup preflight names both numbers when the ceiling exceeds available
storage.

### A2. Memory triggers must name their metric

Decision 4's memory triggers do not say *which* memory number they mean, and the two
available disagree by a factor of four. On 2026-09-09 Proxmox reported 30.89 GB of
36 GB for the Flux VM while the guest reported 6.7 GB used, 24 GB buff/cache,
28 GB available.

Both are correct. Proxmox counts pages the guest has *touched*; Linux never returns
page-cache pages without a balloon driver doing free-page-reporting, and this VM
churns disk hard (NATS append, a 5 MB snapshot every 5 minutes, journald). The
hypervisor figure ratchets up and effectively never falls.

**Policy:** all memory triggers in decision 4 refer to the **guest's own `used`
figure** — `free -h` used, or `psutil.virtual_memory().used`, as published by
`flux-core/vm`. Hypervisor-reported memory is not a trigger and should not be
treated as one.

## References

- ADR-001: State Engine Architecture
- ADR-002: Persistence and Recovery
- ADR-008: Namespace Persistence
- `.odin/tasks/task-01-container-limits-and-log-rotation.md`
