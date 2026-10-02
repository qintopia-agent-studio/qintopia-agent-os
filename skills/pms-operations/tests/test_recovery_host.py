from __future__ import annotations

import hashlib
import hmac
import importlib.util
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "pms_recovery_host", Path(__file__).resolve().parents[1] / "recovery_host.py")
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)

WORK = "ca371c22-2d18-4cbe-9e09-5fe3ee9614f0"
SECOND = "23b46446-aa4a-43ee-8cb3-56f97ecbd305"
BINDING = "f4e32c67-106b-4e0d-8e62-2f08f3d2c71b"


def item(work=WORK, state="pending"):
    return {"work_item": work, "kind": "payment", "status": "awaiting_review",
            "delivery_state": state}


def detail(work=WORK, *, contact=True, phase="pending", target=True, current=True):
    return {**item(work), "contact_required": contact,
            "target": {"binding_id": BINDING, "current": current,
                       "group_binding_id": SECOND,
                       "config_version": 3} if target else None,
            "send": {"key": None, "phase": phase, "receipt": None}}


class RecoveryTests(unittest.TestCase):
    def test_bounded_scan_only_hints_current_pending_work(self):
        sent = []
        calls = []

        def broker(args):
            calls.append(args)
            if args["action"] == "list":
                if "after" not in args:
                    return {"items": [item(), item(SECOND, "unknown")], "next": "page-two"}
                return {"items": [item("f5827ad5-e118-4be1-8889-01731219be24")], "next": None}
            return detail(args["work_item"])

        result = recovery.scan(broker, sent.append)
        self.assertEqual(sent, [WORK, "f5827ad5-e118-4be1-8889-01731219be24"])
        self.assertEqual(result, {"hint_accepted": 2, "held": 1, "incomplete": 0, "more": False})
        self.assertEqual(calls[0], {"action": "list", "limit": 100})
        self.assertEqual(calls[-2], {"action": "list", "limit": 100, "after": "page-two"})
        self.assertFalse(any(call.get("work_item") == SECOND for call in calls))

    def test_target_or_original_send_state_blocks_hint(self):
        for changes in ({"contact": False}, {"target": False}, {"current": False},
                        {"phase": "UNKNOWN"}, {"phase": "claimed"}):
            with self.subTest(changes=changes):
                sent = []
                def broker(args):
                    if args["action"] == "list":
                        return {"items": [item()], "next": None}
                    return detail(**changes)
                self.assertEqual(recovery.scan(broker, sent.append)["held"], 1)
                self.assertEqual(sent, [])

    def test_current_shared_route_shape_hints_only_before_first_claim(self):
        sent = []
        current = {"work_item": WORK, "kind": "payment", "status": "awaiting_review",
                   "contact_required": True, "binding_version": 2,
                   "configuration_version": 7,
                   "target": {"binding_id": SECOND, "binding_version": 4,
                              "conversation_id": BINDING, "platform": "wecom",
                              "chat_id": "simulated-staff-group"},
                   "send": {"key": None, "phase": None, "receipt": None}}
        def broker(args):
            if args["action"] == "list":
                return {"items": [{"work_item": WORK, "kind": "payment",
                                   "status": "awaiting_review", "contact_required": True}],
                        "next": None}
            return current
        self.assertEqual(recovery.scan(broker, sent.append)["hint_accepted"], 1)
        self.assertEqual(sent, [WORK])
        for change in ({"configuration_version": None},
                       {"target": None},
                       {"send": {"key": WORK, "phase": "UNKNOWN", "receipt": None}}):
            with self.subTest(change=change):
                sent.clear()
                current.update(change)
                self.assertEqual(recovery.scan(broker, sent.append)["held"], 1)
                self.assertEqual(sent, [])
                current.update({"configuration_version": 7,
                                "target": {"binding_id": SECOND, "binding_version": 4,
                                           "conversation_id": BINDING, "platform": "wecom",
                                           "chat_id": "simulated-staff-group"},
                                "send": {"key": None, "phase": None, "receipt": None}})

    def test_lost_hint_ack_is_incomplete_and_does_not_settle_or_retry(self):
        calls = []
        def broker(args):
            calls.append(args["action"])
            return {"items": [item()], "next": None} if args["action"] == "list" else detail()
        def lost(_):
            raise TimeoutError("simulated lost acknowledgement")
        self.assertEqual(recovery.scan(broker, lost),
                         {"hint_accepted": 0, "held": 0, "incomplete": 1, "more": False})
        self.assertEqual(calls, ["list", "detail"])

    def test_page_budget_and_cursor_or_duplicate_ref_fail_closed(self):
        with self.assertRaises(ValueError):
            recovery.scan(lambda _: {}, lambda _: None, max_pages=11)
        with self.assertRaisesRegex(ValueError, "cursor"):
            recovery.scan(lambda _: {"items": [], "next": "same"}, lambda _: None)
        def duplicate(args):
            if args["action"] == "list":
                return {"items": [item(), item()], "next": None}
            return detail()
        with self.assertRaisesRegex(ValueError, "duplicate"):
            recovery.scan(duplicate, lambda _: None)

    def test_main_loads_plugin_before_disabled_gate(self):
        with patch.dict(os.environ, {"QINTOPIA_PMS_RECOVERY_PRODUCTION_ENABLE": "0"}):
            with self.assertRaisesRegex(ValueError, "recovery_disabled"):
                recovery.main()

    def test_official_webhook_202_only_accepts_internal_hint(self):
        seen = []
        fixture_key = "simulated-independent-webhook-secret-key"
        class Response:
            status = 202
            def read(self, _):
                return json.dumps({"delivery_id": WORK, "status": "accepted"}).encode()
        class Connection:
            def __init__(self, host, port, timeout):
                self.assertion = (host, port, timeout)
            def request(self, method, path, body, headers):
                seen.append((self.assertion, method, path, json.loads(body), body, headers))
            def getresponse(self):
                return Response()
            def close(self):
                pass
        with patch.object(recovery.http.client, "HTTPConnection", Connection), \
                patch.object(recovery.time, "time", return_value=1780000000):
            self.assertIsNone(recovery.send_internal_hint(WORK, wake={
                "host": "127.0.0.1", "port": 18765, "secret": fixture_key}))
        where, method, path, payload, raw, headers = seen[0]
        self.assertEqual(where, ("127.0.0.1", 18765, 2))
        self.assertEqual((method, path), ("POST", "/webhooks/anan-workitem-wake"))
        self.assertEqual(payload, {"schema_version": 1, "work_item_id": WORK})
        self.assertEqual(headers["X-Request-ID"], WORK)
        self.assertEqual(headers["X-Webhook-Signature-V2"], hmac.new(
            fixture_key.encode(), b"1780000000." + raw, hashlib.sha256).hexdigest())
        self.assertNotIn("group", raw.decode())


if __name__ == "__main__":
    unittest.main()
