# Session: NATS Replay Fix

**Date:** 2026-02-20
**Branch:** main
**Scope:** `src/state/engine.rs`, `src/state/tests.rs`

## Problem

`run_subscriber` used `get_or_create_consumer` for both startup paths:

- **No snapshot** (None): wanted `DeliverPolicy::All` to replay from the beginning
- **With snapshot** (Some): wanted `DeliverPolicy::ByStartSequence { seq+1 }`

The durable consumer `"flux-state-engine"` persists in NATS across Flux restarts.
`get_or_create_consumer` returns the _existing_ consumer at its current ack offset,
silently ignoring the requested `DeliverPolicy`. On restart without a snapshot, only
new events were delivered — all pre-restart entities were lost.

## Fix

Extracted `StateEngine::consumer_delivery(start_sequence)` returning `(should_reset, deliver_policy)`.

**None path (no snapshot):**
1. Delete existing `"flux-state-engine"` consumer (log error if not found, continue)
2. `create_consumer` with `DeliverPolicy::All`

**Some(seq) path (snapshot recovery):**
- Keep existing `get_or_create_consumer` with `DeliverPolicy::ByStartSequence { seq+1 }`

## Files Changed

- `src/state/engine.rs`: added `consumer_delivery()`, refactored `run_subscriber` consumer setup
- `src/state/tests.rs`: added 2 unit tests for `consumer_delivery`

## Tests

```
test state::tests::test_consumer_delivery_no_snapshot_resets_and_delivers_all ... ok
test state::tests::test_consumer_delivery_with_snapshot_resumes_from_next_sequence ... ok
```

All 19 `state::tests` pass.

## Notes

- Pre-existing `api::namespace::tests` failures (8 tests) require NATS running — unrelated.
- `delete_consumer` 404 errors (consumer not found on first start) are logged at `info` level and ignored.
