"""fluxsource — shared runtime for Flux data source services.

Sources are *applications*: deployment-specific, living wherever their operator
puts them. This module is *infrastructure*: the parts every source needs and
which have repeatedly been got wrong when hand-copied.

It exists because two independently-written sources failed silently for 111 and
71 days while systemd reported both `active (running)`. See ADR-012.

Design rules encoded here:

* **A stalled source must be loud.** `run()` exits non-zero on sustained total
  failure, so `Restart=on-failure` means something and the failure is visible.
* **One bad feed must not take down its siblings.** Every feed is isolated.
* **Quiet is not the same as dead.** A heartbeat is emitted on a timer — not per
  item — so a source that publishes nothing still says so.
* **Absent is not the same as unchanged.** Dedup uses a sentinel, never `None`.

Usage:

    import fluxsource as fx

    src = fx.FluxSource(namespace="flux-example", source="example.org")

    def temperature(src, state):
        data = fx.get_json("https://api.example.org/temp")
        if not fx.changed(state, "temp", data["time"]):
            return False
        src.publish("temperature", {"celsius": data["value"]})
        return True

    src.run(feeds=[temperature], poll_interval=300)
"""

from __future__ import annotations

import logging
import os
import sys
import time
from datetime import datetime, timezone
from typing import Callable, Iterable

import requests

__all__ = [
    "FluxSource",
    "FatalSourceError",
    "MISSING",
    "changed",
    "get_json",
    "setup_logging",
]

log = logging.getLogger("fluxsource")

# Sentinel for dedup. A dict.get() default of None makes "absent" compare equal
# to "legitimately None", which silently suppressed every publish in task-04.
MISSING = object()

DEFAULT_TIMEOUT = 45
DEFAULT_RETRIES = 3
HEARTBEAT_ENTITY = "_heartbeat"


class FatalSourceError(RuntimeError):
    """Unrecoverable condition. Terminates the process so systemd restarts it."""


def setup_logging() -> None:
    """Configure logging. `LOG_LEVEL=DEBUG` surfaces per-item detail."""
    logging.basicConfig(
        level=getattr(logging, os.environ.get("LOG_LEVEL", "INFO").upper(), logging.INFO),
        format="%(asctime)s %(levelname)s %(message)s",
    )


def changed(state: dict, key: str, value) -> bool:
    """True if `value` differs from the last seen value for `key`, recording it.

    Uses MISSING rather than None so a genuinely-absent upstream value never
    compares equal to a never-seen key.
    """
    if state.get(key, MISSING) == value:
        return False
    state[key] = value
    return True


def get_json(url: str, *, timeout: int = DEFAULT_TIMEOUT, retries: int = DEFAULT_RETRIES,
             headers: dict | None = None, params: dict | None = None):
    """GET and parse JSON, retrying transient failures with backoff.

    Raises on final failure — callers inside `run()` are isolated per feed.
    """
    last: Exception | None = None
    for attempt in range(1, retries + 1):
        try:
            resp = requests.get(url, timeout=timeout, headers=headers, params=params)
            resp.raise_for_status()
            return resp.json()
        except Exception as exc:  # noqa: BLE001 — deliberately broad; re-raised below
            last = exc
            if attempt < retries:
                delay = 2.0 ** (attempt - 1)
                log.debug("GET %s failed (attempt %d/%d): %r — retrying in %.0fs",
                          url, attempt, retries, exc, delay)
                time.sleep(delay)
    raise last  # type: ignore[misc]


class FluxSource:
    """A source's connection to one Flux namespace."""

    def __init__(self, namespace: str, source: str, *, stream: str | None = None,
                 url: str | None = None, token: str | None = None,
                 admin_token: str | None = None, timeout: int = 10):
        self.namespace = namespace
        self.source = source
        self.stream = stream or namespace.replace("-", ".")
        self.url = (url or os.environ.get("FLUX_URL", "http://localhost:3000")).rstrip("/")
        self.timeout = timeout
        self._admin_token = admin_token if admin_token is not None else os.environ.get("FLUX_ADMIN_TOKEN", "")
        self._poll_interval: float | None = None
        self.token = token if token is not None else os.environ.get("FLUX_NAMESPACE_TOKEN", "")
        if not self.token:
            self.token = self._provision()

    # -- namespace ---------------------------------------------------------

    def _provision(self) -> str:
        """Create the namespace, or fail loudly with what the operator must do."""
        if not self._admin_token:
            raise FatalSourceError(
                f"No FLUX_NAMESPACE_TOKEN for '{self.namespace}' and no FLUX_ADMIN_TOKEN "
                "to provision one. Set one of them in the environment."
            )
        log.info("Provisioning namespace: %s", self.namespace)
        resp = requests.post(
            f"{self.url}/api/namespaces",
            json={"name": self.namespace},
            headers={"Authorization": f"Bearer {self._admin_token}"},
            timeout=self.timeout,
        )
        if resp.status_code == 409:
            raise FatalSourceError(
                f"Namespace '{self.namespace}' already exists but FLUX_NAMESPACE_TOKEN is "
                "unset. Recover the token and set it in the environment."
            )
        resp.raise_for_status()
        token = resp.json()["token"]
        log.warning("Namespace provisioned — PERSIST THIS: FLUX_NAMESPACE_TOKEN=%s", token)
        return token

    @property
    def _headers(self) -> dict:
        return {"Authorization": f"Bearer {self.token}", "Content-Type": "application/json"}

    # -- publishing --------------------------------------------------------

    def publish(self, entity_key: str, properties: dict) -> bool:
        """Publish one entity update. Returns True on success."""
        body = {
            "stream": self.stream,
            "source": self.source,
            "timestamp": int(time.time() * 1000),
            "payload": {
                "entity_id": f"{self.namespace}/{entity_key}",
                "properties": properties,
            },
        }
        try:
            resp = requests.post(f"{self.url}/api/events", json=body,
                                 headers=self._headers, timeout=self.timeout)
        except Exception as exc:  # noqa: BLE001
            log.warning("Publish %s failed: %r", entity_key, exc)
            return False
        if not resp.ok:
            log.warning("Publish %s failed: %s %s", entity_key, resp.status_code, resp.text[:160])
            return False
        log.debug("Published %s", entity_key)
        return True

    # -- state -------------------------------------------------------------

    def list_entities(self, timeout: int = 120) -> list:
        """All entities currently held in this namespace."""
        resp = requests.get(f"{self.url}/api/state/entities",
                            params={"namespace": self.namespace},
                            headers={"Authorization": f"Bearer {self.token}"},
                            timeout=timeout)
        resp.raise_for_status()
        return resp.json()

    def delete_entities(self, entity_ids: Iterable[str], batch_size: int = 500) -> int:
        """Tombstone entities in batches. One bad batch never aborts the rest."""
        ids = list(entity_ids)
        removed = 0
        for i in range(0, len(ids), batch_size):
            chunk = ids[i:i + batch_size]
            try:
                resp = requests.post(f"{self.url}/api/state/entities/delete",
                                     json={"entity_ids": chunk},
                                     headers=self._headers, timeout=120)
                resp.raise_for_status()
                removed += resp.json().get("deleted", 0)
            except Exception:  # noqa: BLE001
                log.exception("Tombstone batch %d failed", i // batch_size)
        return removed

    def tombstone_stale(self, max_age_seconds: float, batch_size: int = 500) -> int:
        """Delete entities not updated within `max_age_seconds`.

        Entities whose timestamp cannot be parsed are tombstoned defensively — an
        unreadable timestamp cannot be shown to be fresh. Keys beginning with "_"
        (the heartbeat) are never tombstoned.
        """
        try:
            entities = self.list_entities()
        except Exception:  # noqa: BLE001
            log.exception("Tombstone sweep: could not list entities")
            return 0

        now = datetime.now(timezone.utc)
        stale = []
        for e in entities:
            key = e["id"].split("/", 1)[-1]
            if key.startswith("_"):
                continue
            try:
                ts = datetime.fromisoformat(e.get("lastUpdated"))
                if ts.tzinfo is None:
                    ts = ts.replace(tzinfo=timezone.utc)
                age = (now - ts).total_seconds()
            except (TypeError, ValueError):
                age = float("inf")
            if age > max_age_seconds:
                stale.append(e["id"])

        if not stale:
            log.info("Tombstone sweep: %d entities, none stale", len(entities))
            return 0
        removed = self.delete_entities(stale, batch_size)
        log.info("Tombstone sweep: %d entities, %d stale (>%.0fh), %d removed",
                 len(entities), len(stale), max_age_seconds / 3600, removed)
        return removed

    def retire_absent(self, present_keys: Iterable[str], batch_size: int = 500) -> int:
        """Tombstone entities whose key is not in `present_keys`.

        For sources whose upstream publishes a complete current set each poll —
        active storms, current alerts, open incidents — where absence means "over"
        rather than "stale". Keys beginning with "_" are never retired.

        Safety: an empty upstream set is a legitimate state (no active storms), so
        this WILL clear the namespace when handed an empty collection. Only call it
        after a successful fetch, never on the error path.
        """
        present = {str(k) for k in present_keys}
        try:
            entities = self.list_entities()
        except Exception:  # noqa: BLE001
            log.exception("retire_absent: could not list entities")
            return 0

        gone = []
        for e in entities:
            key = e["id"].split("/", 1)[-1]
            if key.startswith("_") or key in present:
                continue
            gone.append(e["id"])

        if not gone:
            return 0
        removed = self.delete_entities(gone, batch_size)
        log.info("Retired %d entities no longer present upstream", removed)
        return removed

    # -- liveness ----------------------------------------------------------

    def publish_heartbeat(self, **fields) -> None:
        """Publish the liveness entity.

        Emitted every cycle including cycles that publish nothing — that is the
        whole point. Entity freshness alone cannot distinguish "upstream is
        legitimately quiet" from "this source is broken".
        """
        props = {"last_cycle": datetime.now(timezone.utc).isoformat(), "source": self.source}
        if self._poll_interval is not None:
            # Published so a monitor can derive a staleness threshold from the
            # namespace itself rather than from a separate config that drifts
            # out of step with the source (ADR-012 decision 6).
            props["poll_interval_s"] = self._poll_interval
        props.update(fields)
        self.publish(HEARTBEAT_ENTITY, props)

    # -- run loop ----------------------------------------------------------

    def run(self, feeds: list[Callable], poll_interval: float, *,
            heartbeat_interval: float = 300, max_total_failure_cycles: int = 10,
            tombstone_age: float | None = None, tombstone_interval: float = 3600,
            state: dict | None = None) -> None:
        """Run feeds forever with isolation, heartbeat, tombstoning and fail-loud.

        Each feed is `fn(src, state) -> bool` (True if it published). A feed that
        raises is logged and skipped; its siblings still run. When EVERY feed has
        failed for `max_total_failure_cycles` consecutive cycles the process exits
        non-zero, so systemd restarts it and the failure becomes visible instead
        of the loop spinning silently forever.

        `heartbeat_interval` is checked between cycles, so its EFFECTIVE period is
        `max(poll_interval, heartbeat_interval)`. An hourly poller heartbeats hourly
        no matter what is passed here. Staleness alerting must therefore expect a
        source's poll interval, not this value.
        """
        if not feeds:
            raise FatalSourceError("run() called with no feeds")

        self._poll_interval = poll_interval
        state = state if state is not None else {}
        total_failure_cycles = 0
        next_heartbeat = 0.0
        next_tombstone = time.monotonic() + tombstone_interval
        cycle = 0

        if tombstone_age is not None:
            self.tombstone_stale(tombstone_age)

        while True:
            cycle += 1
            published = failed = 0

            for fn in feeds:
                try:
                    if fn(self, state):
                        published += 1
                except Exception:  # noqa: BLE001 — isolation is the point
                    failed += 1
                    log.exception("Feed %s failed", getattr(fn, "__name__", fn))

            log.info("Cycle %d — %d published, %d/%d feeds failed",
                     cycle, published, failed, len(feeds))

            if failed == len(feeds):
                total_failure_cycles += 1
                log.error("All %d feeds failed (%d consecutive cycles)",
                          len(feeds), total_failure_cycles)
                if total_failure_cycles >= max_total_failure_cycles:
                    log.critical(
                        "Every feed has failed for %d consecutive cycles — exiting so "
                        "systemd restarts and the failure is visible",
                        total_failure_cycles,
                    )
                    sys.exit(1)
            else:
                total_failure_cycles = 0

            now = time.monotonic()
            if now >= next_heartbeat:
                self.publish_heartbeat(cycle=cycle, published=published,
                                       feeds_failed=failed, feeds_total=len(feeds))
                next_heartbeat = now + heartbeat_interval

            if tombstone_age is not None and now >= next_tombstone:
                self.tombstone_stale(tombstone_age)
                next_tombstone = time.monotonic() + tombstone_interval

            time.sleep(poll_interval)
