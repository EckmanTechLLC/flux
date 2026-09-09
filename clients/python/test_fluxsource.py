"""Tests for fluxsource.

Weighted toward the failure modes that actually occurred in production:
silent dedup suppression, one feed killing its siblings, and a stalled loop
that never exits so systemd never restarts it.
"""

import sys
import unittest
from datetime import datetime, timedelta, timezone
from unittest import mock

import fluxsource as fx


class StopLoop(Exception):
    """Breaks out of run()'s infinite loop in tests."""


def make_source(**kw):
    return fx.FluxSource(namespace="flux-test", source="test", token="tok",
                         url="http://flux.invalid", **kw)


class TestChanged(unittest.TestCase):
    def test_first_sighting_is_a_change(self):
        self.assertTrue(fx.changed({}, "k", "v"))

    def test_repeat_is_not_a_change(self):
        state = {}
        fx.changed(state, "k", "v")
        self.assertFalse(fx.changed(state, "k", "v"))

    def test_absent_is_distinguished_from_none(self):
        # task-04: dict.get(key) defaulting to None made a legitimately-None
        # upstream value compare equal to a never-seen key, silently skipping
        # every publish. The sentinel is what prevents that.
        state = {}
        self.assertTrue(fx.changed(state, "k", None),
                        "a None value on first sighting must count as changed")
        self.assertFalse(fx.changed(state, "k", None))


class TestFeedIsolation(unittest.TestCase):
    def test_one_failing_feed_does_not_stop_the_others(self):
        # flux-spaceweather: feeds ran in one unprotected loop, so a 404 in
        # feed 2 meant feeds 3 and 4 never executed for 71 days.
        ran = []

        def ok_a(src, state):
            ran.append("a"); return True

        def boom(src, state):
            ran.append("boom"); raise RuntimeError("upstream 404")

        def ok_b(src, state):
            ran.append("b"); return True

        src = make_source()
        with mock.patch.object(src, "publish_heartbeat"), \
             mock.patch("fluxsource.time.sleep", side_effect=StopLoop):
            with self.assertRaises(StopLoop):
                src.run([ok_a, boom, ok_b], poll_interval=1)

        self.assertEqual(ran, ["a", "boom", "b"],
                         "every feed must run even when one raises")


class TestFailLoud(unittest.TestCase):
    def test_exits_after_sustained_total_failure(self):
        # The process must be able to die. flux-airquality could not: bare
        # except + capped backoff meant Restart=on-failure never fired.
        def always_fails(src, state):
            raise RuntimeError("down")

        src = make_source()
        with mock.patch.object(src, "publish_heartbeat"), \
             mock.patch("fluxsource.time.sleep"):
            with self.assertRaises(SystemExit) as cm:
                src.run([always_fails], poll_interval=0, max_total_failure_cycles=3)
        self.assertEqual(cm.exception.code, 1)

    def test_partial_success_resets_the_failure_counter(self):
        calls = {"n": 0}

        def flaky(src, state):
            calls["n"] += 1
            if calls["n"] % 2:
                raise RuntimeError("intermittent")
            return True

        src = make_source()
        with mock.patch.object(src, "publish_heartbeat"), \
             mock.patch("fluxsource.time.sleep") as slept:
            slept.side_effect = lambda *_: (_ for _ in ()).throw(StopLoop) if calls["n"] >= 8 else None
            with self.assertRaises(StopLoop):
                src.run([flaky], poll_interval=0, max_total_failure_cycles=3)
        # Alternating failure must never accumulate to the exit threshold.


class TestHeartbeat(unittest.TestCase):
    def test_heartbeat_fires_even_when_nothing_publishes(self):
        # "Quiet" and "dead" must not look identical. flux-hurricanes legitimately
        # sits at zero entities for months and that is correct.
        def publishes_nothing(src, state):
            return False

        src = make_source()
        with mock.patch.object(src, "publish_heartbeat") as hb, \
             mock.patch("fluxsource.time.sleep", side_effect=StopLoop):
            with self.assertRaises(StopLoop):
                src.run([publishes_nothing], poll_interval=1)
        hb.assert_called_once()
        self.assertEqual(hb.call_args.kwargs["published"], 0)


class TestTombstone(unittest.TestCase):
    def _entities(self):
        now = datetime.now(timezone.utc)
        return [
            {"id": "flux-test/fresh", "lastUpdated": now.isoformat()},
            {"id": "flux-test/stale", "lastUpdated": (now - timedelta(days=40)).isoformat()},
            {"id": "flux-test/_heartbeat", "lastUpdated": (now - timedelta(days=40)).isoformat()},
            {"id": "flux-test/broken", "lastUpdated": "not-a-timestamp"},
        ]

    def test_selects_stale_and_unparseable_but_spares_heartbeat(self):
        src = make_source()
        with mock.patch.object(src, "list_entities", return_value=self._entities()), \
             mock.patch.object(src, "delete_entities", return_value=2) as delete:
            src.tombstone_stale(max_age_seconds=30 * 86400)

        deleted = sorted(delete.call_args.args[0])
        self.assertEqual(deleted, ["flux-test/broken", "flux-test/stale"])
        self.assertNotIn("flux-test/_heartbeat", deleted,
                         "the liveness entity must never be tombstoned")


class TestGetJson(unittest.TestCase):
    def test_retries_then_succeeds(self):
        good = mock.Mock()
        good.json.return_value = {"ok": True}
        good.raise_for_status.return_value = None
        with mock.patch("fluxsource.requests.get",
                        side_effect=[RuntimeError("boom"), good]) as get, \
             mock.patch("fluxsource.time.sleep"):
            self.assertEqual(fx.get_json("http://x.invalid"), {"ok": True})
        self.assertEqual(get.call_count, 2)

    def test_raises_after_exhausting_retries(self):
        with mock.patch("fluxsource.requests.get", side_effect=RuntimeError("boom")), \
             mock.patch("fluxsource.time.sleep"):
            with self.assertRaises(RuntimeError):
                fx.get_json("http://x.invalid", retries=2)


class TestProvisioning(unittest.TestCase):
    def test_no_token_and_no_admin_token_is_fatal(self):
        with mock.patch.dict("os.environ", {}, clear=True):
            with self.assertRaises(fx.FatalSourceError):
                fx.FluxSource(namespace="flux-test", source="t", url="http://flux.invalid")


if __name__ == "__main__":
    unittest.main(verbosity=2)
