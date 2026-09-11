from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "bin" / "migrate_script_action.py"


class MigrateScriptActionTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.action = self.root / "silaoshi_resident_webhook_action.py"
        self.handler = self.root / "silaoshi_resident_card_from_message.py"
        self.sender = self.root / "silaoshi_wecom_send.py"
        self.transform_source = self.root / "source.py"
        self.transform_target = self.root / "profile" / "scripts" / "transform.py"
        self.config = self.root / "etc" / "config.json"
        self.secret = self.root / "etc" / "secret"
        self.subscriptions = self.root / "webhook_subscriptions.json"
        self.sender.write_text("print('sent')\n")
        self.handler.write_text(
            f"SEND_TEXT_SCRIPT = {str(self.sender)!r}\nCOMPANY_GROUP_ID = 'fixture-group'\n"
        )
        self.action.write_text(f"HANDLER = {str(self.handler)!r}\nprint('ok')\n")
        self.transform_source.write_text("print('[SILENT]')\n")
        self.original = {
            "silaoshi-base-notify": {
                "events": ["base_event"],
                "secret": "fixture-route-secret",
                "script_action": {
                    "script": str(self.action),
                    "timeout": 900,
                    "success_template": "success {returncode}",
                    "failure_template": "failure {stderr}",
                },
            },
            "other-route": {"secret": "unchanged"},
        }
        self.subscriptions.write_text(json.dumps(self.original))

    def tearDown(self):
        self.tmp.cleanup()

    def run_migration(self, apply=False, rollback=False):
        command = [
            sys.executable,
            str(SCRIPT),
            "--subscriptions",
            str(self.subscriptions),
            "--transform-source",
            str(self.transform_source),
            "--transform-target",
            str(self.transform_target),
            "--config-target",
            str(self.config),
            "--secret-target",
            str(self.secret),
        ]
        if apply:
            command.append("--apply")
        if rollback:
            command.append("--rollback")
        return subprocess.run(command, text=True, capture_output=True)

    def test_dry_run_does_not_write_or_disclose_bindings(self):
        result = self.run_migration()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "ready")
        self.assertNotIn("fixture-group", result.stdout)
        self.assertNotIn("fixture-route-secret", result.stdout)
        self.assertEqual(json.loads(self.subscriptions.read_text()), self.original)
        self.assertFalse(self.config.exists())

    def test_apply_preserves_routes_and_creates_atomic_backup(self):
        result = self.run_migration(apply=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        migrated = json.loads(self.subscriptions.read_text())
        self.assertEqual(migrated["other-route"], self.original["other-route"])
        self.assertNotIn("script_action", migrated["silaoshi-base-notify"])
        self.assertEqual(migrated["silaoshi-base-notify"]["script"], str(self.transform_target))
        self.assertEqual(json.loads(self.config.read_text())["action"]["timeout_seconds"], 900)
        self.assertTrue(self.secret.is_file())
        self.assertEqual(json.loads(self.subscriptions.with_suffix(".json.pre-bridge").read_text()), self.original)

    def test_apply_refuses_to_overwrite_backup(self):
        self.assertEqual(self.run_migration(apply=True).returncode, 0)
        self.assertNotEqual(self.run_migration(apply=True).returncode, 0)

    def test_rollback_restores_original_subscription(self):
        self.assertEqual(self.run_migration(apply=True).returncode, 0)
        result = self.run_migration(rollback=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "rolled_back")
        self.assertEqual(json.loads(self.subscriptions.read_text()), self.original)


if __name__ == "__main__":
    unittest.main()
