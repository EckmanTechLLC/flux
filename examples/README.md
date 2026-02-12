# Flux Examples

Example publishers for testing and demonstrating Flux capabilities.

---

## 1. Random Sensor Publisher

**Use case:** IoT sensors, environmental monitoring

**Usage:**
```bash
./random-publisher.sh

# Custom settings
FLUX_URL=https://your-flux.com INTERVAL=2 ./random-publisher.sh
```

**What it publishes:**
- Stream: `sensors`
- Entities: `sensor-1` through `sensor-5`
- Properties: `temperature` (18-28°C), `humidity` (40-80%), `status`
- Interval: 2 seconds (configurable)

**Output:**
```
[21:19:53] Published sensor-1: temp=19.4°C, humidity=76% → 019c4dd2...
[21:19:55] Published sensor-3: temp=27.8°C, humidity=62% → 019c4dd4...
```

---

## 2. System Metrics Publisher

**Use case:** Infrastructure monitoring, server health

**Usage:**
```bash
./system-metrics-publisher.sh

# Monitor 5 hosts, update every 10s
NUM_HOSTS=5 INTERVAL=10 ./system-metrics-publisher.sh
```

**What it publishes:**
- Stream: `infrastructure`
- Entities: `host-01`, `host-02`, `host-03` (configurable)
- Properties: `cpu_percent`, `memory_percent`, `disk_percent`, `load_avg`, `status`
- Interval: 5 seconds (configurable)
- Status: `healthy`, `elevated`, `warning` (color coded)

**Output:**
```
[21:45:01] healthy  host-01: CPU= 45.2% MEM= 62.1% DISK= 55.3% LOAD=1.23 → 019c4dd2...
[21:45:06] warning  host-02: CPU= 92.8% MEM= 88.5% DISK= 68.2% LOAD=3.45 → 019c4dd4...
```

---

## 3. Application Events Publisher

**Use case:** Application observability, event logs, user activity

**Usage:**
```bash
./app-events-publisher.sh

# Faster, more bursty pattern
MIN_INTERVAL=0 MAX_INTERVAL=2 ./app-events-publisher.sh
```

**What it publishes:**
- Stream: `application`
- Entity: `app-events`
- Event types: `user-login`, `page-view`, `api-call`, `payment-processed`, `error-occurred`, etc.
- Properties: `event_type`, `severity`, `user_id`, `duration_ms`, `event_count`
- Pattern: Bursty (random 1-5s intervals)
- Weighted frequency (errors are rare, page-views are common)

**Output:**
```
[21:50:12] info    page-view            user-1523 (145ms) → 019c4dd2...
[21:50:14] error   error-occurred       user-1876 (342ms) → 019c4dd4...
[21:50:17] info    payment-processed    user-1234 (89ms) → 019c4dd6...
```

---

## Running Multiple Publishers

Test multi-stream coordination:

```bash
# Terminal 1 - Sensors
./random-publisher.sh

# Terminal 2 - Infrastructure
./system-metrics-publisher.sh

# Terminal 3 - Application events
./app-events-publisher.sh
```

All publish to the same Flux instance, creating a rich multi-domain world state.

---

## Configuration

All publishers support:

| Variable | Default | Description |
|----------|---------|-------------|
| `FLUX_URL` | `http://localhost:3000` | Flux API endpoint |
| `INTERVAL` | Varies | Seconds between publishes |

Additional per-publisher:
- **Sensors:** None
- **System Metrics:** `NUM_HOSTS` (default: 3)
- **App Events:** `MIN_INTERVAL`, `MAX_INTERVAL` (default: 1-5s)

---

## Python Client Examples

**Use case:** Programmatic Flux interaction, custom clients

**Location:** `/examples/python/`

**Installation:**
```bash
cd python/
pip install -r requirements.txt
```

**Scripts:**
1. **publish_event.py** - Publish events via HTTP API
2. **subscribe_websocket.py** - Subscribe to real-time state updates
3. **query_state.py** - Query current entity state

**Usage examples:**
```bash
# Publish sensor reading
./python/publish_event.py --stream sensors --source demo --entity sensor-1 temperature=22.5

# Subscribe to entity updates
./python/subscribe_websocket.py --entity sensor-1

# Query current state
./python/query_state.py --entity sensor-1
```

See `/examples/python/README.md` for complete documentation.

---

## Use Cases

- **Development:** Test Flux with realistic data
- **Demo:** Show multi-domain state coordination
- **Stress testing:** Run multiple publishers simultaneously
- **Integration testing:** Verify WebSocket subscriptions
- **Debugging:** Generate predictable test data
- **Custom clients:** Python examples as reference implementation
