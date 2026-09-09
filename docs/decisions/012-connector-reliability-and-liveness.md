# ADR-012: Connector Reliability and Liveness

**Status:** Accepted
**Date:** 2026-09-09
**Extends:** ADR-011 (Scaling Strategy)

---

## Background

Flux ingests from 16 standalone Python source services on .107. Each is a hand-copied
variant of the same poll-and-publish pattern, living in `/home/etl/flux-<domain>/`,
untracked by any repository and deployed by rsync.

On 2026-09-09 two were found dead, having failed silently for 111 and 71 days. Both
reported `active (running)` to systemd for the entire period.

- **flux-spaceweather** — SWPC retired `plasma-1-day.json` (HTTP 404). `fetch_and_publish`
  runs four feeds in one unprotected loop, so the raise in feed 2 meant feeds 3 and 4 never
  executed. Three of four entities froze; the first feed kept publishing, which made the
  namespace look alive.
- **flux-airquality** — the OpenAQ key began returning 401. `discover_all_cities()` runs
  exactly once, before the main loop; it returned `{}`, and the poll loop has iterated an
  empty dict every 30 minutes since, logging `Poll complete — 0 published/updated` at INFO.

Both share one structural defect: `while True` wrapped around a bare `except Exception`
with capped backoff. **The process cannot exit.** `Restart=on-failure` is therefore
unreachable, and a permanent stall is indistinguishable from health at every layer —
systemd, the logs, and monitoring alike.

Monitoring compounds this. `flux-core/directory` publishes per-namespace entity *counts*
but never *freshness*, so a namespace frozen for four months looks identical to a healthy one.

Eight further sources are queued (Tier 4, drafted 2026-05). Building them on the current
pattern multiplies a known failure surface by nine.

## Decisions

### 1. Extract a shared `fluxsource` module

The same class of defect arose independently in two scripts, and an upstream shape-change
has now bitten twice. Sixteen copies cannot be fixed once. The module owns: HTTP with
timeout/retry/backoff, Flux publish and namespace provisioning, dedup via the `_MISSING`
sentinel, tombstoning, the liveness heartbeat, and structured logging.

### 2. Per-feed error isolation is mandatory

A source polling multiple feeds must isolate each one. A single upstream failure must never
prevent sibling feeds from publishing. This decision alone would have preserved three of
flux-spaceweather's four entities.

### 3. Fail loud — define fatal versus degraded

- **Fatal** (exit non-zero; let systemd restart and alert): zero-yield startup such as 0 of
  N targets discovered; auth failure across all targets; sustained total failure beyond a
  bounded number of cycles.
- **Degraded** (log at ERROR and continue): individual feed or target failure while others
  still succeed.

`Restart=on-failure` only means something if failure can terminate the process.

### 4. Discovery is periodic, never once-at-startup

Re-run discovery on an interval and whenever yield transitions to zero. A key that recovers,
or a station that returns, must not require a manual restart.

### 5. Every source publishes a liveness heartbeat

On every cycle, including cycles that publish no data. Entity freshness alone cannot
distinguish "upstream is legitimately quiet" — hurricane off-season, no active alerts — from
"source is broken." A heartbeat can.

### 6. Staleness is monitored and alerted

flux-monitor gains a per-namespace freshness check against a declared expected-max-age, and
alerts on breach. ADR-011 decision 5 already requires that triggers be observable; this
extends the same principle to source liveness.

### 7. Source-side tombstoning is standing policy

Default 30 days, overridable per source. Publishers tombstone their own stale entities, as
aviation and ships already do. This keeps eviction out of the engine and preserves ADR-011's
constraint. Note the trade: NATS retention is 7 days, so tombstoned state is permanently
unrecoverable.

### 8. Schema migrations coexist

When an upstream change forces a property-schema change, publish both old and new keys for a
defined overlap window, as was done for flux-weather. Consumers such as gene-observer are not
coordinated with Flux deploys and must never be broken by one.

### 9. The library is versioned; the sources are not

*Superseded 2026-09-09 by the operator.* The original decision put all 16 source
directories under version control. That conflates two different things.

Sources are **applications** — deployment-specific, and in a public repo they would amount
to publishing one operator's particular choices as though they were part of Flux. Anyone
running Flux picks their own. They stay untracked in `/home/etl/flux-*`.

`fluxsource` is **infrastructure** — the runtime every source needs and which has repeatedly
been got wrong when hand-copied. It ships in this repo at `clients/python/fluxsource.py`,
alongside the existing Python examples, and is useful to anyone building a source rather
than only to us.

Sources import it by path: `PYTHONPATH=/home/etl/flux/clients/python`, so a `git pull`
updates every source at once without touching any of them.

The rsync hazards that motivated the original decision (`.env` clobber, token swap) are
already handled by `flux-deploy`, which hardcodes `--exclude='.env'` and captures tokens
from the journal rather than by hand.

## Constraints

- **Do not add eviction to the Flux engine.** ADR-011's constraint stands. Tombstoning is
  publisher-side only.
- **Do not couple `fluxsource` to Flux internals** beyond the public HTTP API.
- **Do not make Flux domain-aware.** The engine stays payload-agnostic; all source semantics
  live in the sources.
- **Do not rewrite working sources wholesale.** Migrate to the module incrementally, starting
  with the two already broken and ending with the healthy ones.
- **Do not build new Tier-4 sources** until the module and decisions 2–6 are in place.

## References

- ADR-011: Scaling Strategy
- `.odin/memory.md` — task 10/12 tombstoning, task 02/04 upstream shape-change incidents
- Diagnosis session 2026-09-09
