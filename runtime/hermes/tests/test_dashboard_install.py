"""Exercise activation transactions with synthetic artifacts and mocked systemd."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("dashboard_install", Path(__file__).resolve().parents[3] / "deploy/runner/install-hermes-dashboard.py")
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class DashboardInstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.commit = "a" * 40
        self.checksum = "b" * 64
        self.installed = self.root / "releases" / self.checksum
        self.installed.mkdir(parents=True)
        self.manifest = {"core_commit": self.commit, "upstream_typecheck": "passed"}
        (self.installed / "dashboard-manifest.json").write_text(json.dumps(self.manifest))
        self.dropin = self.root / "systemd/qintopia-release.conf"
        self.dropin.parent.mkdir()
        self.dropin.write_text("[Service]\nExecStart=previous\n")
        self.previous = self.dropin.read_bytes()
        patches = {
            "ROOT": self.root, "DROPIN": self.dropin,
            "REPO": Path("/home/ubuntu/qintopia-agent-os-releases") / self.commit,
        }
        for key, value in patches.items():
            item = patch.object(module, key, value)
            item.start()
            self.addCleanup(item.stop)
        for name in ("checked_directory", "validate", "digest", "restart", "smoke"):
            item = patch.object(module, name)
            setattr(self, name, item.start())
            self.addCleanup(item.stop)
        self.validate.return_value = self.manifest
        self.digest.return_value = self.checksum
        item = patch.object(module.os, "geteuid", return_value=0)
        item.start()
        self.addCleanup(item.stop)
        item = patch.object(module.subprocess, "run")
        self.run = item.start()
        self.addCleanup(item.stop)

    def invoke(self, *extra):
        with patch.object(sys, "argv", ["install", "--manifest-sha256", self.checksum, *extra]):
            module.main()

    def test_preflight_failure_keeps_unit_and_no_recovery_record(self):
        self.run.side_effect = subprocess.CalledProcessError(1, "precheck")
        with self.assertRaises(subprocess.CalledProcessError):
            self.invoke("--apply")
        self.assertEqual(self.dropin.read_bytes(), self.previous)
        self.assertFalse((self.root / "state" / self.checksum / "previous.json").exists())
        self.restart.assert_not_called()

    def test_apply_idempotent_and_explicit_rollback_restores_previous(self):
        self.invoke("--apply")
        self.assertNotEqual(self.dropin.read_bytes(), self.previous)
        self.invoke("--apply")
        self.assertEqual(self.restart.call_count, 1)
        self.invoke("--rollback")
        self.assertEqual(self.dropin.read_bytes(), self.previous)
        self.invoke("--apply")
        self.assertEqual(self.restart.call_count, 3)

    def test_failed_activation_rolls_back_and_can_retry(self):
        self.smoke.side_effect = module.ArtifactError("fixture_failure")
        with patch.object(module.time, "sleep"), self.assertRaises(module.ArtifactError):
            self.invoke("--apply")
        self.assertEqual(self.dropin.read_bytes(), self.previous)
        self.smoke.side_effect = None
        self.invoke("--apply")
        self.assertNotEqual(self.dropin.read_bytes(), self.previous)

    def test_operator_drift_blocks_retry_and_rollback(self):
        self.invoke("--apply")
        self.dropin.write_text("operator change")
        for action in ("--apply", "--rollback"):
            with self.assertRaises(module.ArtifactError):
                self.invoke(action)
        self.assertEqual(self.dropin.read_text(), "operator change")

    def test_failed_typecheck_cannot_activate_without_explicit_exception(self):
        self.manifest["upstream_typecheck"] = "failed"
        with self.assertRaises(module.ArtifactError):
            self.invoke("--apply")
        self.restart.assert_not_called()
        self.assertEqual(self.dropin.read_bytes(), self.previous)

    def test_preview_does_not_restart_or_create_state(self):
        self.invoke()
        self.restart.assert_not_called()
        self.assertFalse((self.root / "state").exists())

    def test_concurrent_activation_refuses_without_changing_unit(self):
        with module.activation_lock():
            with self.assertRaisesRegex(module.ArtifactError, "already_in_progress"):
                self.invoke("--apply")
        self.assertEqual(self.dropin.read_bytes(), self.previous)
        self.restart.assert_not_called()


if __name__ == "__main__":
    unittest.main()
