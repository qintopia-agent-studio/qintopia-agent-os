from __future__ import annotations

import importlib.util
import json
import os
import sqlite3
import tempfile
import time
import unittest
from pathlib import Path


def load_bridge():
    path = Path(__file__).resolve().parents[1] / "bin" / "script_action_bridge.py"
    spec = importlib.util.spec_from_file_location("silaoshi_script_action_bridge", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ScriptActionBridgeTest(unittest.TestCase):
    def setUp(self):
        self.bridge = load_bridge()
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.secret = root / "secret"
        self.secret.write_text("s" * 48)
        self.action = root / "action.py"
        self.action.write_text("import sys\nprint(sys.stdin.read())\n")
        self.cfg = {
            "socket_path": str(root / "bridge.sock"),
            "database_path": str(root / "jobs.sqlite3"),
            "secret_file": str(self.secret),
            "action": {
                "name": "silaoshi-base-notify",
                "command": [os.sys.executable, str(self.action)],
                "script_sha256": self.bridge.hashlib.sha256(self.action.read_bytes()).hexdigest(),
                "timeout_seconds": 10,
                "max_retries": 1,
            },
        }
        self.conn = self.bridge.connect_db(self.cfg)

    def tearDown(self):
        self.conn.close()
        self.tmp.cleanup()

    def envelope(self, payload=None):
        return self.bridge.signed_envelope(payload or {"record_id": "recABCDEFGH"}, self.cfg, now=1000)

    def test_signature_and_timestamp_are_required(self):
        envelope = self.envelope()
        self.bridge.verify_envelope(envelope, self.cfg, now=1000)
        envelope["payload"]["changed"] = True
        with self.assertRaises(self.bridge.BridgeError):
            self.bridge.verify_envelope(envelope, self.cfg, now=1000)

    def test_delivery_is_persistently_idempotent(self):
        envelope = self.envelope()
        self.assertEqual(self.bridge.enqueue(self.conn, envelope, now=1000), "accepted")
        self.assertEqual(self.bridge.enqueue(self.conn, envelope, now=1001), "duplicate")
        self.assertEqual(self.conn.execute("SELECT count(*) FROM jobs").fetchone()[0], 1)

    def test_successful_job_is_terminal(self):
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.assertTrue(self.bridge.run_one(self.conn, self.cfg, now=1000))
        status, attempts, returncode = self.conn.execute(
            "SELECT status,attempts,returncode FROM jobs"
        ).fetchone()
        self.assertEqual((status, attempts, returncode), ("succeeded", 1, 0))

    def test_failure_retries_once_then_stops(self):
        self.action.write_text("raise SystemExit(7)\n")
        self.cfg["action"]["script_sha256"] = self.bridge.hashlib.sha256(self.action.read_bytes()).hexdigest()
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.bridge.run_one(self.conn, self.cfg, now=1000)
        self.assertEqual(self.conn.execute("SELECT status FROM jobs").fetchone()[0], "retry")
        self.bridge.run_one(self.conn, self.cfg, now=1030)
        self.assertEqual(
            self.conn.execute("SELECT status,attempts,returncode FROM jobs").fetchone(),
            ("failed", 2, 7),
        )

    def test_script_digest_drift_fails_closed(self):
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.action.write_text("print('changed')\n")
        self.bridge.run_one(self.conn, self.cfg, now=1000)
        self.assertEqual(self.conn.execute("SELECT status FROM jobs").fetchone()[0], "retry")

    def test_timeout_is_retried(self):
        self.action.write_text("import time\ntime.sleep(2)\n")
        self.cfg["action"]["script_sha256"] = self.bridge.hashlib.sha256(self.action.read_bytes()).hexdigest()
        self.cfg["action"]["timeout_seconds"] = 1
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        started = time.monotonic()
        self.bridge.run_one(self.conn, self.cfg, now=1000)
        self.assertLess(time.monotonic() - started, 1.8)
        self.assertEqual(self.conn.execute("SELECT status FROM jobs").fetchone()[0], "retry")

    def test_terminal_result_runs_notification_without_replaying_action(self):
        notification = Path(self.tmp.name) / "notification.py"
        receipt = Path(self.tmp.name) / "notification.txt"
        notification.write_text(
            "import pathlib,sys\npathlib.Path(sys.argv[1]).write_text(sys.stdin.read())\n"
        )
        self.cfg["action"].update(
            {
                "notification_command": [os.sys.executable, str(notification), str(receipt)],
                "success_template": "completed rc={returncode}",
            }
        )
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.bridge.run_one(self.conn, self.cfg, now=1000)
        self.assertEqual(receipt.read_text(), "completed rc=0")
        self.assertEqual(
            self.conn.execute("SELECT notification_status FROM jobs").fetchone()[0], "sent"
        )
        self.assertFalse(self.bridge.run_one(self.conn, self.cfg, now=1001))

    def test_restart_recovers_an_interrupted_job(self):
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.conn.execute("UPDATE jobs SET status='running'")
        self.conn.commit()
        self.conn.close()
        self.conn = self.bridge.connect_db(self.cfg)
        self.assertEqual(self.conn.execute("SELECT status FROM jobs").fetchone()[0], "retry")


if __name__ == "__main__":
    unittest.main()
