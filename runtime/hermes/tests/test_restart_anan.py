import importlib.util
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("restart_anan", Path(__file__).parents[1] / "restart_anan.py")
restart = importlib.util.module_from_spec(spec)
spec.loader.exec_module(restart)


class Control:
    def __init__(self):
        self.marker = None
        self.cleared = False

    def read_drain_request(self, **_):
        return self.marker

    def write_drain_request(self, **kwargs):
        self.marker = kwargs
        return self.marker

    def clear_drain_request(self, **_):
        self.marker = None
        self.cleared = True
        return True


class RestartTests(unittest.TestCase):
    def setUp(self):
        self.control = Control()
        self.tick = 0
        self.action = Mock()

    def sleep(self, seconds):
        self.tick += seconds

    def record(self, count=0, **extra):
        return {"pid": 42, "gateway_state": "draining", "active_agents": count,
                "updated_at": str(self.tick), **extra}

    def run_drain(self, snapshot):
        restart.drain_and_restart(self.control, snapshot, self.action, home=Path("/simulated"),
                                  pid=42, clock=lambda: self.tick, sleep=self.sleep, timeout=6)

    def test_changed_interpreter_or_service_entrypoint_refuses_before_drain(self):
        with patch.object(restart.sys, "executable", "/unexpected/python"), patch.object(restart, "systemctl") as systemctl:
            with self.assertRaisesRegex(restart.Deferred, "interpreter_mismatch"):
                restart.main()
            systemctl.assert_not_called()
        with patch.object(restart.sys, "executable", str(restart.CORE / "venv/bin/python")), patch.object(
                restart, "systemctl", return_value="{ path=/unexpected/python ; }") as systemctl:
            with self.assertRaisesRegex(restart.Deferred, "service_entrypoint_mismatch"):
                restart.main()
            systemctl.assert_called_once_with("show", restart.SERVICE, "--property=ExecStart", "--value")

    def test_waits_for_work_then_restarts_once_and_cancels_marker(self):
        self.run_drain(lambda: self.record(count=1 if self.tick < 2 else 0))
        self.assertEqual(self.tick, 4)
        self.action.assert_called_once_with()
        self.assertTrue(self.control.cleared)

    def test_timeout_missing_stale_and_invalid_counts_never_restart(self):
        for snapshot in [lambda: self.record(1), lambda: None,
                         lambda: self.record(updated_at="unchanged"),
                         lambda: self.record(False), lambda: self.record(gateway_state="running")]:
            with self.subTest(snapshot=snapshot):
                self.setUp()
                with self.assertRaisesRegex(restart.Deferred, "active_work_timeout"):
                    self.run_drain(snapshot)
                self.action.assert_not_called()
                self.assertTrue(self.control.cleared)

    def test_preexisting_and_replaced_operator_markers_are_not_removed(self):
        self.control.marker = {"principal": "operator"}
        with self.assertRaisesRegex(restart.Deferred, "existing_drain"):
            self.run_drain(lambda: self.record())
        self.assertFalse(self.control.cleared)
        self.control.marker = None
        def replace():
            self.control.marker = {"principal": "operator"}
            return self.record(1)
        with self.assertRaisesRegex(restart.Deferred, "drain_changed"):
            self.run_drain(replace)
        self.action.assert_not_called()
        self.assertFalse(self.control.cleared)

    def test_pid_change_defers_and_restart_failure_releases_owned_drain(self):
        with self.assertRaisesRegex(restart.Deferred, "gateway_identity_changed"):
            self.run_drain(lambda: self.record(pid=43))
        self.action.assert_not_called()
        self.assertTrue(self.control.cleared)
        self.setUp()
        self.action.side_effect = RuntimeError("simulated systemd failure")
        with self.assertRaises(RuntimeError):
            self.run_drain(lambda: self.record())
        self.action.assert_called_once()
        self.assertTrue(self.control.cleared)


if __name__ == "__main__":
    unittest.main()
