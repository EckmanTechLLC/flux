# Session: Phase 1 Python Client Examples

**Date:** 2026-02-11
**Task:** Implement Python client examples for Flux
**Status:** Complete ✅

---

## Objective

Create 3 Python example scripts demonstrating Flux client usage:
1. Event publishing (HTTP POST)
2. WebSocket subscription (real-time updates)
3. State querying (HTTP GET)

---

## Implementation

### Files Created

**Scripts:**
- `/examples/python/publish_event.py` - Publish events to Flux
- `/examples/python/subscribe_websocket.py` - Subscribe to state updates
- `/examples/python/query_state.py` - Query current entity state

**Supporting:**
- `/examples/python/requirements.txt` - Dependencies (requests, websockets)
- `/examples/python/README.md` - Usage documentation

### Design Decisions

**1. Argument Parsing**
- Used `argparse` for CLI interface
- Property parsing: `key=value` format with JSON type detection
- Environment variable support: `FLUX_URL` (default: http://localhost:3000)

**2. Error Handling**
- Clear error messages for common failures
- Connection refused → "Is Flux running?"
- Timeout, HTTP errors with details
- No silent failures (all errors printed to stderr)

**3. Output Formatting**
- Default: Human-readable formatted output
- `--compact` flag for one-line summaries
- `--json` flag for raw JSON (easy piping)
- `--verbose` flag for debug mode (WebSocket)

**4. WebSocket Subscription**
- Used `websockets` library (sync client)
- Graceful Ctrl+C handling
- Timeout on receive to allow interruption
- Formats updates compactly by default

### API Usage

**Event Publishing (POST /api/events):**
```python
event = {
    "stream": stream,
    "source": source,
    "timestamp": int(time.time() * 1000),  # Unix epoch ms
    "payload": {
        "entity_id": entity_id,
        "properties": properties,
    },
}
```

**State Query (GET /api/state/entities/:id):**
```python
response = requests.get(f"{flux_url}/api/state/entities/{entity_id}")
```

**WebSocket Subscribe:**
```python
subscribe_msg = {
    "type": "subscribe",
    "entityId": entity_id  # Optional, omit for all entities
}
websocket.send(json.dumps(subscribe_msg))
```

---

## Testing

### Manual Test Commands

**Publish event:**
```bash
cd /home/etl/projects/flux/examples/python
./publish_event.py --stream sensors --source demo --entity sensor-1 temperature=22.5
```

**Subscribe to updates:**
```bash
./subscribe_websocket.py --entity sensor-1
```

**Query state:**
```bash
./query_state.py --entity sensor-1
```

### Expected Behavior

1. **publish_event.py** should return:
   - Event ID (UUIDv7)
   - Stream name
   - Entity ID
   - Properties sent

2. **subscribe_websocket.py** should:
   - Connect to WebSocket
   - Print "Connected"
   - Display state updates as they arrive
   - Handle Ctrl+C gracefully

3. **query_state.py** should:
   - Return entity with properties
   - Format as JSON or human-readable
   - Return 404 if entity not found

---

## Code Quality

**Meets standards:**
- ✅ Clear variable/function names
- ✅ Inline comments for complex logic (property parsing, WebSocket loop)
- ✅ Error handling (connection, timeout, HTTP errors)
- ✅ Help text and examples in argparse
- ✅ No TODOs or commented-out code
- ✅ Functions <100 lines
- ✅ Follows Python conventions (PEP 8 style)

**Documentation:**
- ✅ README.md with usage examples
- ✅ Complete example workflow (publish → subscribe → query)
- ✅ Error message guidance
- ✅ Configuration instructions

---

## Integration

**Updated documentation:**
- Examples README lists all 3 Python scripts
- Main README.md already documented the API (no changes needed)

**Installation:**
```bash
pip install -r requirements.txt
```

Or with venv:
```bash
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
```

---

## Key Features

**publish_event.py:**
- Supports all event envelope fields (stream, source, key, schema)
- Auto-generates timestamp
- Parses properties with type detection (numbers, booleans, strings)
- Clear success message with event ID

**subscribe_websocket.py:**
- Subscribe to all entities or specific entity
- Real-time formatted updates
- Verbose mode for debugging
- Graceful disconnect on Ctrl+C

**query_state.py:**
- List all entities or query specific entity
- Compact and JSON output modes
- Clear 404 handling
- Multi-entity list formatting

---

## Next Steps

**Suggested follow-up:**
1. Test scripts with live Flux instance
2. Add to examples/README.md (Python section)
3. Consider batch publishing example (future)
4. Consider unsubscribe example (future)

**User verification:**
1. Install dependencies: `pip install -r requirements.txt`
2. Start Flux: `docker-compose up -d`
3. Run examples as documented in README

---

## Deliverables

✅ 3 Python scripts (publish, subscribe, query)
✅ requirements.txt with dependencies
✅ README.md in examples/python/
✅ Scripts are executable (chmod +x)
✅ FLUX_URL environment variable support
✅ Clear error messages, no silent failures
✅ Session notes created

**Files modified/created:**
- examples/python/publish_event.py (new)
- examples/python/subscribe_websocket.py (new)
- examples/python/query_state.py (new)
- examples/python/requirements.txt (new)
- examples/python/README.md (new)
- docs/sessions/2026-02-11-phase1-python-examples.md (new)

---

## Notes

- Used `websockets` (sync client) for simplicity (no async required)
- Property parsing handles common types (numbers, booleans, strings)
- All scripts support `--help` for usage details
- Error messages guide users to solutions (e.g., "Is Flux running?")
- WebSocket uses timeout on receive to allow Ctrl+C interruption
- JSON output mode enables piping to jq or other tools
- Compact mode useful for monitoring many entities

---

**Session complete.** Python examples ready for testing and use.
