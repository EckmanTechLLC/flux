# Session: Ratzilla UX Improvements
**Date:** 2026-02-28
**Status:** Complete ✅

## Task
Three UX improvements to the Ratzilla WASM UI: search/filter, mouse click to select, mouse wheel scroll.

## Changes Made

### 1. Search/Filter (`ui/src/main.rs`)
- Added `search_mode: bool`, `search_query: String`, `terminal_rows: u16` to `AppState`
- Added `filtered_entity_ids()` — sorts then applies case-insensitive substring filter
- Updated `selected_entity_data()` and `delete_entity()` to use filtered list
- Panel title shows: `Entities (42/7120) /query█` (blinking cursor) when searching, `Entities (42/7120)` when filter active but not in search mode, `Entities (7120)` normally
- Key handler: `/` enters search mode; `Esc` clears query and exits; `Backspace` removes last char; any other char appends; `↑↓` navigate filtered list; all other keys (j/k, Tab) work only in normal mode
- Selection resets to 0 on every filter change

### 2. Mouse Click to Select (`ui/index.html`)
- JS overlay reads `data-flux-row`, `data-flux-count`, `data-flux-rows` attributes set on `<pre>` by Rust each draw frame
- Calculates terminal row from click Y: `Math.floor((clientY - pre.top) / (pre.height / totalRows))`
- Entity index = termRow - 4 (skip header=2, border=1, col header=1)
- Only acts on left panel clicks (x < pre.left + pre.width/2)
- Dispatches synthetic `ArrowDown`/`ArrowUp` keydown events on document (picked up by Ratzilla's key handler)

### 3. Mouse Wheel Scroll (`ui/index.html`)
- `document.addEventListener("wheel", ...)` with `{ passive: false }` to allow `preventDefault()`
- Dispatches `ArrowDown` (deltaY > 0) or `ArrowUp` (deltaY < 0) synthetic keydown events

### 4. DOM State Bridge (`ui/src/main.rs`)
- `update_dom_state(selected, filtered_count, terminal_rows)` sets data attributes on `<pre>` each rAF frame
- Requires web-sys `Document` + `Element` features (added to `ui/Cargo.toml`)

### 5. Help Bar
- Updated to show `/` search hint and `scroll/click to select` in normal mode
- Shows `Esc clear search  type to filter` in search mode

## Files Changed
- `ui/Cargo.toml` — added web-sys `Document`, `Element` features
- `ui/src/main.rs` — search state, filtered_entity_ids, DOM bridge, key handler, render_entity_list, render_help
- `ui/index.html` — JS wheel + click event handlers

## Build
`trunk build --release` ✅ — 1 pre-existing warning (unused `timestamp` field in `AgentMessage`), no errors.

## Deploy
User runs from `/home/etl/projects/flux/`:
```
docker compose build --no-cache flux-ui && docker compose up -d flux-ui
```

## Notes
- Ratzilla 0.3 has `on_mouse_event` on the `WebRenderer` trait but the pure-JS overlay approach was used instead — avoids WASM/JS boundary crossing and is simpler
- The data-attribute bridge (Rust sets, JS reads) is clean and has no WASM export complexity
- Click dispatches multiple Arrow key events synchronously; all state updates complete before next rAF render
