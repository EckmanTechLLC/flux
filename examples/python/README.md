# Flux Python Client Examples

Python scripts demonstrating how to interact with Flux.

## Installation

```bash
pip install -r requirements.txt
```

Or using a virtual environment:

```bash
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
```

## Scripts

### 1. Publish Events

**publish_event.py** - Send events to Flux

```bash
# Basic usage
./publish_event.py --stream sensors --source demo --entity sensor-1 temperature=22.5 humidity=45

# With optional key and schema
./publish_event.py --stream sensors --source demo --entity sensor-1 --key sensor-1 --schema v1 temp=22.5

# Multiple properties
./publish_event.py --stream infrastructure --source monitor --entity host-01 \
  cpu_percent=45.2 memory_percent=62.1 status=healthy

# Custom Flux URL
FLUX_URL=http://flux.example.com:3000 ./publish_event.py --stream test --source cli --entity test-1 value=42
```

**Property parsing:**
- Numbers: `temperature=22.5` → `{"temperature": 22.5}`
- Booleans: `active=true` → `{"active": true}`
- Strings: `status=online` → `{"status": "online"}`

### 2. Subscribe to State Updates

**subscribe_websocket.py** - Real-time state updates via WebSocket

```bash
# Subscribe to all entities
./subscribe_websocket.py

# Subscribe to specific entity
./subscribe_websocket.py --entity sensor-1

# Verbose mode (show raw JSON)
./subscribe_websocket.py --entity sensor-1 --verbose

# Custom Flux URL
FLUX_URL=http://flux.example.com:3000 ./subscribe_websocket.py
```

**Output:**
```
[2026-02-11T22:30:45] sensor-1: temperature=22.5, humidity=45, status=active
[2026-02-11T22:30:47] sensor-3: temperature=27.8, humidity=62
```

### 3. Query Current State

**query_state.py** - Query entity state via HTTP

```bash
# List all entities
./query_state.py

# Get specific entity
./query_state.py --entity sensor-1

# Compact output
./query_state.py --compact

# Raw JSON output
./query_state.py --json

# Custom Flux URL
FLUX_URL=http://flux.example.com:3000 ./query_state.py --entity sensor-1
```

## Configuration

All scripts support the `FLUX_URL` environment variable:

```bash
export FLUX_URL=http://localhost:3000
```

Default: `http://localhost:3000`

## Complete Example

```bash
# Terminal 1 - Subscribe to updates
./subscribe_websocket.py --entity sensor-1

# Terminal 2 - Publish events
./publish_event.py --stream sensors --source demo --entity sensor-1 temperature=22.5
./publish_event.py --stream sensors --source demo --entity sensor-1 temperature=23.0
./publish_event.py --stream sensors --source demo --entity sensor-1 temperature=23.5

# Terminal 3 - Query current state
./query_state.py --entity sensor-1
```

You'll see real-time updates in Terminal 1 and the current state in Terminal 3.

## Error Handling

All scripts provide clear error messages:

- **Connection refused:** Is Flux running? Try `docker-compose up -d`
- **Timeout:** Check network connectivity and Flux URL
- **HTTP 404:** Entity not found (for query_state.py)
- **Invalid format:** Check property syntax (key=value)

## See Also

- **Bash examples:** `/examples/*.sh` - Random publishers for testing
- **API documentation:** `/README.md` - Full API reference
- **OpenClaw skill:** `/examples/openclaw-skill/` - Agent integration
