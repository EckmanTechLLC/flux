# fluxsource

Shared runtime for Flux data source services.

Sources are *applications* — deployment-specific, and yours will differ from anyone
else's. This module is the *infrastructure* they share: the parts every source needs
and that have repeatedly been got wrong when hand-copied between scripts.

It exists because two independently-written sources failed silently for 111 and 71
days while systemd reported both `active (running)`. See
[ADR-012](../../docs/decisions/012-connector-reliability-and-liveness.md).

## What it gives you

| | |
|---|---|
| **Fail loud** | `run()` exits non-zero on sustained total failure, so `Restart=on-failure` means something |
| **Feed isolation** | one failing upstream never stops its siblings |
| **Liveness heartbeat** | emitted on a timer, so a source that publishes nothing still says so |
| **Dedup that works** | a sentinel, so "absent" never compares equal to "legitimately `None`" |
| **Tombstoning** | publisher-side stale-entity retirement, batched and fault-tolerant |
| **HTTP with retries** | timeout and exponential backoff by default |

## Use

```python
import fluxsource as fx

fx.setup_logging()
src = fx.FluxSource(namespace="flux-example", source="example.org")

def temperature(src, state):
    data = fx.get_json("https://api.example.org/temp")
    if not fx.changed(state, "temp", data["time"]):
        return False                     # unchanged upstream, nothing to publish
    src.publish("temperature", {"celsius": data["value"]})
    return True

src.run(feeds=[temperature], poll_interval=300, tombstone_age=30 * 86400)
```

A feed is `fn(src, state) -> bool`, returning True when it published. Raise and you
get logged and skipped; your siblings still run.

## Environment

| Variable | Purpose |
|---|---|
| `FLUX_URL` | Flux API base URL (default `http://localhost:3000`) |
| `FLUX_NAMESPACE_TOKEN` | namespace write token |
| `FLUX_ADMIN_TOKEN` | only needed to provision a namespace on first run |
| `LOG_LEVEL` | `DEBUG` surfaces per-item publish lines |

## Install

No build step. Put it on the path:

```ini
# in your systemd unit
Environment=PYTHONPATH=/path/to/flux/clients/python
```

A `git pull` then updates every source at once.

## Tests

```bash
cd clients/python && python3 -m unittest test_fluxsource -v
```
