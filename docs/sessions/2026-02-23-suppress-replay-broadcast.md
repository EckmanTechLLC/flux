# Session: Suppress Broadcast During NATS Replay

**Date:** 2026-02-23
**Status:** Complete

## Problem

On startup, the state engine replays all NATS events into memory. During replay,
`update_property` and `delete_entity` broadcast every state change to WebSocket
subscribers. This caused:
- `broadcast::error::RecvError::Lagged` warnings (channel overflow, capacity 1000)
- Clients connected during replay getting flooded with redundant updates
- Hundreds of "WebSocket lagged" log lines on each restart

## Fix

Added a `replaying: AtomicBool` flag to `StateEngine` (initialized `true`).
Broadcasts are suppressed while the flag is set. `run_subscriber` uses a 500 ms
idle timeout to detect when the NATS backlog is drained, then calls `set_live()`
which flips the flag and logs "State engine live — broadcasting enabled".

## Files Modified

- `src/state/engine.rs` — main changes
- `src/state/tests.rs` — two existing broadcast tests needed `set_live()` added

## Changes in engine.rs

| Location | Change |
|---|---|
| imports | Added `AtomicBool` |
| `StateEngine` struct | Added `replaying: AtomicBool` field |
| `StateEngine::new()` | Init `replaying: AtomicBool::new(true)` |
| `update_property()` | Gate `state_tx.send()` behind `!replaying` |
| `delete_entity()` | Gate `deletion_tx.send()` behind `!replaying` |
| New method | `pub fn set_live(&self)` — stores false, logs info |
| `run_subscriber()` | Loop uses `tokio::time::timeout(500ms)` on messages.next(); timeout fires → `set_live()` |
| Tests added | 4 new tests covering suppression and resumption |

## Notes

- Metrics recording in `process_event` is **not** suppressed (counts stay accurate)
- State mutations (DashMap writes) still happen during replay — only broadcasts are skipped
- After `set_live()`, the idle-wait falls through to `messages.next().await` and the
  loop continues normally for live events
- The 500 ms window is conservative; with no events the timeout fires quickly after
  the last replayed message is processed

## Tests

- `broadcast_suppressed_during_replay` — new
- `broadcast_resumes_after_set_live` — new
- `deletion_suppressed_during_replay` — new
- `deletion_broadcast_after_set_live` — new
- `test_state_updates_broadcast_correctly` — fixed (added `set_live()`)
- `test_deletion_broadcast` — fixed (added `set_live()`)

All 196 tests pass.

## Deploy

```
docker compose build --no-cache flux && docker compose up -d flux
```
