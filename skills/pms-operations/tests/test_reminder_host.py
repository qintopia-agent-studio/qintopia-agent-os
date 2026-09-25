from __future__ import annotations
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import uuid

spec = importlib.util.spec_from_file_location("reminder_host", Path(__file__).resolve().parents[1] / "reminder_host.py")
reminder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reminder)


class ReminderHostTests(unittest.TestCase):
    def request(self):
        return {"status": "send", "claim": str(uuid.uuid4()), "text": "模拟待办：待到店、待收款",
                "plan": {"profile": "anan", "simulated": True, "signature": "synthetic-content-version",
                         "destination": {"group": "synthetic-group", "chat_id": "synthetic-chat", "platform": "wecom"}}}

    def test_recording_adapter_retains_exact_receipt_and_rejects_repeat(self):
        with tempfile.TemporaryDirectory() as directory:
            adapter = reminder.LocalRecordingAdapter(directory, enabled=True, profile="anan")
            request = self.request()
            receipt = adapter.send(request)
            self.assertEqual(adapter.lookup(request["claim"]), receipt)
            with self.assertRaises(FileExistsError):
                adapter.send(request)
            self.assertEqual(len(list(Path(directory).iterdir())), 1)
            self.assertNotIn("token", json.dumps(receipt))

    def test_no_channel_or_profile_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            for profile, enabled in [("anan", False), ("default", True), ("erhua", True), (None, True)]:
                with self.assertRaises(ValueError):
                    reminder.LocalRecordingAdapter(directory, enabled=enabled, profile=profile)
            adapter = reminder.LocalRecordingAdapter(directory, enabled=True, profile="anan")
            request = self.request()
            request["plan"]["simulated"] = False
            with self.assertRaises(ValueError):
                adapter.send(request)
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_unknown_without_receipt_never_reads_pms_or_sends(self):
        class ForbiddenPms:
            def read(self, *args, **kwargs):
                raise AssertionError("unknown must not resubmit")
        calls = []
        claim = str(uuid.uuid4())
        def host(a):
            calls.append(a["action"])
            if a["action"] == "list": return {"works": ["work"], "next": None}
            if a["action"] == "context": return {"state": {"phase": "unknown", "claim": claim}}
            raise AssertionError("unexpected host mutation")
        with tempfile.TemporaryDirectory() as directory:
            adapter = reminder.LocalRecordingAdapter(directory, enabled=True, profile="anan")
            result = reminder.run_once(ForbiddenPms(), host, adapter)
            self.assertEqual(result["results"], [{"status": "unknown"}])
            self.assertEqual(calls, ["list", "context"])
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_lost_broker_ack_recovers_exact_adapter_receipt_without_resend(self):
        calls = []
        with tempfile.TemporaryDirectory() as directory:
            adapter = reminder.LocalRecordingAdapter(directory, enabled=True, profile="anan")
            request = self.request()
            receipt = adapter.send(request)
            def host(a):
                calls.append(a["action"])
                if a["action"] == "list": return {"works": ["work"], "next": None}
                if a["action"] == "context": return {"state": {"phase": "unknown", "claim": request["claim"]}}
                if a["action"] == "settle":
                    self.assertEqual(a["receipt"], receipt)
                    return {"status": "sent", "simulated": True}
                raise AssertionError("unexpected action")
            result = reminder.run_once(None, host, adapter)
            self.assertEqual(result["results"][0]["status"], "sent")
            self.assertEqual(calls, ["list", "context", "settle"])
            self.assertEqual(len(list(Path(directory).iterdir())), 1)
