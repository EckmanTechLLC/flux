# Session: ADR-010 Task 2 — i3X Runner

**Date:** 2026-03-13
**Status:** Complete ✅

---

## What Was Done

Created `connector-manager/src/runners/i3x.rs` — the i3X SSE streaming runner.

### Files Modified
- `connector-manager/src/runners/i3x.rs` — **created** (new)
- `connector-manager/src/runners/mod.rs` — added `pub mod i3x;`

### Implementation

**`I3xStatus`** — serializable runtime status per source:
- `source_id`, `source_name`, `object_count`, `last_event`, `last_error`, `restart_count`

**`I3xRunner`** — manages one Tokio task per source:
- `start_source(config, api_key)` — spawns background task, initializes status
- `stop_source(source_id)` — aborts task, removes status
- `status()` — returns `Vec<I3xStatus>`
- Internal: `task_handles: Mutex<HashMap>` + `status_map: Arc<Mutex<HashMap>>`

**`run_i3x_loop`** — outer reconnect loop:
- Calls `stream_once`, resets backoff to 1s on clean close
- On error: logs, records in status, sleeps backoff, doubles (max 60s)

**`stream_once`** — one session:
1. `GET /objects` → discover element IDs
2. `POST /subscriptions` → create subscription
3. `POST /subscriptions/{id}/register` → register all objects
4. `GET /subscriptions/{id}/stream` → open SSE (no timeout, long-lived)
5. Read chunks via `response.chunk()` (no `futures_util` needed)
6. Parse `data:` lines → `SseEvent` → `build_properties` → Flux event → POST

On 404/410 from SSE open: returns `Err` (outer loop recreates subscription on next attempt).

**`build_properties`**:
- Object value → spread all keys + append `i3x_quality`, `i3x_timestamp`
- Scalar value → wrap under `"value"` key + append `i3x_quality`, `i3x_timestamp`

**`sanitize_element_id`**: replaces non-alphanumeric/`-`/`_`/`.` chars with `_`

**Flux event format:**
```json
{
  "stream": "i3x",
  "source": "i3x.{source_id}",
  "timestamp": <millis>,
  "key": "<elementId>",
  "payload": {
    "entity_id": "{namespace}/{sanitized_elementId}",
    "properties": { ... + "i3x_quality": ..., "i3x_timestamp": ... }
  }
}
```

Auth: `Authorization: Bearer {flux_namespace_token}` if token is non-empty.

### Tests (11 new in i3x.rs)
- `test_sanitize_element_id_passthrough`
- `test_sanitize_element_id_replaces_slashes_and_spaces`
- `test_parse_sse_scalar_value`
- `test_parse_sse_object_value`
- `test_parse_sse_missing_optional_fields`
- `test_build_properties_scalar`
- `test_build_properties_object_spread`
- `test_build_properties_null_value`
- `test_build_properties_string_value`
- `test_i3x_runner_status_empty`
- `test_i3x_status_serializes`

**Total connector-manager tests: 74 passed, 0 failed.**

---

## Notes

- No new Cargo dependencies required (`reqwest::Response::chunk()` avoids need for `futures_util`)
- One compile fix: `response` needed `mut` for `.chunk()` call
- No live i3X endpoint available — tests cover parsing/logic only, not HTTP calls
- Runner is ready for Task 3 (API endpoints) to call `start_source` / `stop_source` / `status`

---

## Next Task

**Task 3: API endpoints** — extend `api.rs` with:
- `POST /api/connectors/i3x` — add source + store credentials + start runner
- `DELETE /api/connectors/i3x/{id}` — stop runner + delete config + delete credentials
- `POST /api/connectors/i3x/{id}/sync` — one-shot sync via `/sync` endpoint
- Extend `GET /api/connectors` status to include i3x type
