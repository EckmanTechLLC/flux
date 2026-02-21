# Session: Generic Runner Crash Detection
**Date:** 2026-02-21
**Status:** Complete

## Task
GenericRunner did not detect or restart crashed Bento subprocesses. It stored raw `tokio::process::Child` handles with no monitoring loop.

## Changes

**File modified:** `connector-manager/src/runners/generic.rs`

- Replaced `process_handles: Mutex<HashMap<String, tokio::process::Child>>` with `task_handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>`
- `start_source`: initializes status entry, spawns `run_bento_loop` background task, stores `JoinHandle`
- `stop_source`: aborts task handle (no manual kill needed), cleans up config file
- Added `run_bento_loop` free fn (modeled on `NamedRunner::run_tap_loop`):
  - Writes YAML config each iteration
  - Updates `last_started` before spawn
  - If `bento` not found on PATH: logs warning and exits loop (not retriable)
  - If bento crashes (non-zero exit): sets `last_error`, increments `restart_count`, sleeps 5s, restarts
  - If bento exits cleanly (code 0): increments `restart_count`, sleeps 5s, restarts
  - Spawn errors: sets `last_error`, sleeps 5s, retries
- Updated tracing imports: added `error`, `info`

## API unchanged
`start_source` and `stop_source` signatures are identical. No callers required updates.

## Rebuild required
```
docker compose build --no-cache connector-manager && docker compose up -d connector-manager
```

## Verification
- Start a generic source, confirm it appears in logs: `Generic source started`, `Bento subprocess started`
- Kill the bento process manually (`kill <pid>`), confirm log: `Bento crashed — restarting in 5s`
- Confirm bento restarts 5s later and `restart_count` increments in status API
