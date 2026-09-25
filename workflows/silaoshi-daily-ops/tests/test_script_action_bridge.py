from __future__ import annotations

import importlib.util
import json
import os
import sqlite3
import tempfile
import time
import unittest
from unittest.mock import patch
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

    def test_action_output_is_bounded_while_streams_are_drained(self):
        self.action.write_text(
            "import sys\n"
            "sys.stdout.write('o' * 200000)\n"
            "sys.stderr.write('e' * 200000)\n"
        )
        self.cfg["action"]["script_sha256"] = self.bridge.hashlib.sha256(self.action.read_bytes()).hexdigest()
        stdout, stderr, returncode = self.bridge._run_bounded_action(
            self.bridge._validate_action(self.cfg), "{}", 10
        )
        self.assertEqual(returncode, 0)
        self.assertEqual(len(stdout.encode()), self.bridge.MAX_OUTPUT_BYTES)
        self.assertEqual(len(stderr.encode()), self.bridge.MAX_OUTPUT_BYTES)

    def test_timeout_terminates_child_process_group(self):
        child_pid = Path(self.tmp.name) / "child.pid"
        self.action.write_text(
            "import pathlib,subprocess,sys,time\n"
            "child=subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])\n"
            f"pathlib.Path({str(child_pid)!r}).write_text(str(child.pid))\n"
            "time.sleep(30)\n"
        )
        self.cfg["action"]["script_sha256"] = self.bridge.hashlib.sha256(self.action.read_bytes()).hexdigest()
        _, _, returncode = self.bridge._run_bounded_action(
            self.bridge._validate_action(self.cfg), "{}", 1
        )
        self.assertEqual(returncode, -1)
        pid = int(child_pid.read_text())
        time.sleep(0.1)
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)

    def test_terminal_result_runs_notification_without_replaying_action(self):
        notification = Path(self.tmp.name) / "notification.py"
        receipt = Path(self.tmp.name) / "notification.txt"
        notification.write_text(
            "import pathlib,sys\npathlib.Path(sys.argv[1]).write_text(sys.stdin.read())\n"
        )
        self.cfg["action"].update(
            {
                "notification_command": [os.sys.executable, str(notification), str(receipt)],
                "success_template": "completed rc={returncode} hash={result_hash}",
            }
        )
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.bridge.run_one(self.conn, self.cfg, now=1000)
        message = receipt.read_text()
        self.assertRegex(message, r"^completed rc=0 hash=[0-9a-f]{64}$")
        self.assertEqual(
            self.conn.execute("SELECT notification_status FROM jobs").fetchone()[0], "sent"
        )
        self.assertFalse(self.bridge.run_one(self.conn, self.cfg, now=1001))

    def test_notification_cannot_include_action_output(self):
        notification = Path(self.tmp.name) / "notification.py"
        receipt = Path(self.tmp.name) / "notification.txt"
        notification.write_text(
            "import pathlib,sys\npathlib.Path(sys.argv[1]).write_text(sys.stdin.read())\n"
        )
        self.action.write_text("import sys\nprint('stdout-secret')\nprint('stderr-secret', file=sys.stderr)\n")
        self.cfg["action"].update(
            {
                "script_sha256": self.bridge.hashlib.sha256(self.action.read_bytes()).hexdigest(),
                "notification_command": [os.sys.executable, str(notification), str(receipt)],
                "success_template": "done {returncode} {result_hash}",
            }
        )
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.bridge.run_one(self.conn, self.cfg, now=1000)
        message = receipt.read_text()
        self.assertNotIn("stdout-secret", message)
        self.assertNotIn("stderr-secret", message)

    def test_notification_template_rejects_output_fields(self):
        with self.assertRaises(self.bridge.BridgeError):
            self.bridge._notification_message("failure {stderr}", 1, "0" * 64)

    def test_restart_recovers_an_interrupted_job(self):
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        self.conn.execute("UPDATE jobs SET status='running'")
        self.conn.commit()
        self.conn.close()
        self.conn = self.bridge.connect_db(self.cfg)
        self.assertEqual(self.conn.execute("SELECT status FROM jobs").fetchone()[0], "retry")

    def application_mode(self):
        self.cfg["application_intake"] = {"local_only": True,
            "resource_alias": "resident-application", "record_path": ["record_id"]}
        return patch.dict(os.environ, {"QINTOPIA_APPLICATION_LOCAL_ENABLE": "1"})

    def test_application_mode_is_explicit_and_does_not_execute_legacy_jobs(self):
        self.bridge.enqueue(self.conn, self.envelope(), now=1000)
        with self.application_mode():
            self.assertFalse(self.bridge.run_one(self.conn, self.cfg, now=1000))
            self.assertEqual(self.conn.execute("SELECT status,attempts FROM jobs").fetchone(), ("queued", 0))
        with patch.dict(os.environ, {"QINTOPIA_APPLICATION_LOCAL_ENABLE": "0"}):
            with self.assertRaises(self.bridge.BridgeError):
                self.bridge.run_one(self.conn, self.cfg, now=1000)

    def test_same_record_callback_wakes_again_even_when_legacy_delivery_already_exists(self):
        envelope = self.envelope()
        self.bridge.enqueue(self.conn, envelope, now=1000)
        with self.application_mode(), patch.object(self.bridge, "application_readback", return_value={"status": "accepted"}) as read:
            self.bridge.enqueue(self.conn, envelope, now=1000, cfg=self.cfg)
            self.assertTrue(self.bridge.run_one(self.conn, self.cfg, now=1000))
            self.bridge.enqueue(self.conn, envelope, now=1001, cfg=self.cfg)
            self.assertTrue(self.bridge.run_one(self.conn, self.cfg, now=1001))
            self.assertEqual(read.call_count, 2)
            self.assertEqual(self.conn.execute("SELECT requested_version,completed_version FROM application_wakes").fetchone(), (2, 2))
            self.assertEqual(self.conn.execute("SELECT attempts FROM jobs").fetchone()[0], 0)

    def test_readback_failure_preserves_hint_without_running_old_script(self):
        with self.application_mode(), patch.object(self.bridge, "application_readback", side_effect=ValueError("private-source-body")):
            self.bridge.enqueue(self.conn, self.envelope(), now=1000, cfg=self.cfg)
            self.bridge.run_one(self.conn, self.cfg, now=1000)
            self.assertFalse(self.bridge.run_one(self.conn, self.cfg, now=1001))
            self.assertEqual(self.conn.execute("SELECT requested_version,completed_version,last_error_code FROM application_wakes").fetchone(), (1, 0, "readback_pending"))
            self.assertEqual(self.conn.execute("SELECT count(*) FROM jobs").fetchone()[0], 0)

    def test_callback_arriving_during_readback_remains_pending(self):
        def readback(*_):
            self.bridge.enqueue(self.conn, self.envelope(), now=1001, cfg=self.cfg)
            return {"status": "duplicate"}
        with self.application_mode(), patch.object(self.bridge, "application_readback", side_effect=readback):
            self.bridge.enqueue(self.conn, self.envelope(), now=1000, cfg=self.cfg)
            self.bridge.run_one(self.conn, self.cfg, now=1000)
            self.assertEqual(self.conn.execute("SELECT requested_version,completed_version FROM application_wakes").fetchone(), (2, 1))

    def test_committed_source_followup_pending_retries_without_losing_new_wake(self):
        for key, state in [("candidate_projection", "projection_unconfirmed"),
                           ("welcome", "handoff_unconfirmed")]:
            with self.subTest(state=state):
                self.conn.execute("DELETE FROM application_wakes")
                self.conn.execute("DELETE FROM jobs")
                self.conn.commit()
                self.bridge.enqueue(self.conn, self.envelope(), now=999)
                with self.application_mode():
                    self.bridge.enqueue(self.conn, self.envelope(), now=1000, cfg=self.cfg)
                    with patch.object(self.bridge, "application_readback", return_value={"status": "accepted", key: {"status": state}}):
                        self.assertTrue(self.bridge.run_one(self.conn, self.cfg, now=1000))
                        self.assertFalse(self.bridge.run_one(self.conn, self.cfg, now=1001))
                    self.assertEqual(self.conn.execute("SELECT requested_version,completed_version,last_error_code FROM application_wakes").fetchone(), (1, 0, "followup_pending"))
                    def recovered(*_):
                        self.bridge.enqueue(self.conn, self.envelope(), now=1031, cfg=self.cfg)
                        return {"status": "duplicate", "candidate_projection": {"stored": True}, "welcome": {"status": "awaiting_reliable_stay_link"}}
                    with patch.object(self.bridge, "application_readback", side_effect=recovered):
                        self.assertTrue(self.bridge.run_one(self.conn, self.cfg, now=1030))
                    self.assertEqual(self.conn.execute("SELECT requested_version,completed_version,last_error_code FROM application_wakes").fetchone(), (2, 1, None))
                    with patch.object(self.bridge, "application_readback", return_value={"status": "duplicate", "candidate_projection": {"status": "not_eligible"}, "welcome": {"status": "awaiting_confirmation"}}):
                        self.assertTrue(self.bridge.run_one(self.conn, self.cfg, now=1031))
                    self.assertEqual(self.conn.execute("SELECT requested_version,completed_version FROM application_wakes").fetchone(), (2, 2))
                    self.assertEqual(self.conn.execute("SELECT status,attempts FROM jobs").fetchone(), ("queued", 0))

    def test_new_intake_uses_only_configured_reference_and_stores_no_payload(self):
        payload = {"record_id": "recABCDEFGH", "person": "private-person", "withdrawn": True, "approved": True}
        with self.application_mode():
            self.bridge.enqueue(self.conn, self.envelope(payload), now=1000, cfg=self.cfg)
            self.assertNotIn("private-person", str(self.conn.execute("SELECT * FROM application_wakes").fetchall()))
            with self.assertRaises(self.bridge.BridgeError):
                self.bridge.enqueue(self.conn, self.envelope({"nested": payload}), now=1001, cfg=self.cfg)


if __name__ == "__main__":
    unittest.main()
