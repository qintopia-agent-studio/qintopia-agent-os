"""HTTP contract tests. The broker is scripted; these do not prove PG matching."""
from __future__ import annotations
import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import socket
import threading
import tempfile
import unittest
from unittest.mock import patch
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).resolve().parents[1] / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


host = load("stay_contacts_host", "stay_contacts_host.py")
pms = load("stay_contacts_client", "client.py")


class StayContactsTests(unittest.TestCase):
    def setUp(self):
        self.work, self.application, self.presentation = [str(uuid.uuid4()) for _ in range(3)]
        self.http, self.calls, self.routes = [], [], {}
        self.allowed = True
        owner = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_GET(self):
                owner.http.append(self.path)
                if self.path == "/api/v1/me":
                    status, value = 200, {"propertyAccess": {"property_a": "READ"} if owner.allowed else {},
                                          "allowedActions": {}, "propertyCommandGrants": {}}
                else:
                    status, value = owner.routes.get(self.path, (500, {"private": "unexpected_route"}))
                if status == 0:
                    self.connection.shutdown(socket.SHUT_RDWR)
                    self.connection.close()
                    return
                self.send_response(status)
                if status == 302:
                    self.send_header("Location", "http://example.invalid/private")
                self.end_headers()
                self.wfile.write(json.dumps(value).encode())
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.client = pms.Client(f"http://127.0.0.1:{self.server.server_port}", "synthetic_contact_credential", local_enabled=True)
        self.reads = []
        self.result = self.summary("complete", True)
        self.save_error = False
        self.status_error = False

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def summary(self, status, complete):
        return {"work_item": self.work, "application": self.application, "status": status,
                "pool_count": 0, "orders_total": 0, "orders_done": 0, "failed_count": 0,
                "scan_complete": complete, "local_only": True}

    def add_order(self, index=0):
        order_id = f"order_{index}"
        read = {"order_id": order_id, "property_id": "property_a", "read_token": str(uuid.uuid4()), "order_revision": "3"}
        raw = {"order": {"id": order_id, "property_id": "property_a", "version": 3, "private": "excluded"},
               "occupants": [{"id": f"occupant_{index}", "phone": " 138-0000-0000 ", "identityCardNumber": "excluded"},
                             {"id": f"other_{index}", "phone": None, "history": {"phone": "never-use"}}],
               "primaryGuest": {"phone": "never-use"}, "history": "excluded"}
        self.reads.append(read)
        self.routes[f"/api/v1/orders/{order_id}"] = 200, raw
        return raw

    def broker(self, args):
        self.calls.append(copy.deepcopy(args))
        if args["action"] == "open":
            return {**self.summary("pending", False), "reads": self.reads}
        if args["action"] == "save":
            if self.save_error:
                raise RuntimeError("private broker diagnostic")
            return {"stored": True}
        if args["action"] == "failed":
            return {"stored": True}
        if args["action"] == "status":
            if self.status_error:
                raise RuntimeError("private broker diagnostic")
            return self.result
        raise AssertionError("unexpected_broker_action")

    def run_host(self, *, refresh=False):
        runner = host.StayContactsHost(self.client, self.broker, local_enabled=True)
        return runner.refresh_contacts(self.work, self.presentation) if refresh else runner.synchronize(self.work)

    def test_full_pool_over_twenty_and_private_projection(self):
        for index in range(23): self.add_order(index)
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            result = self.run_host()
        saves = [a for a in self.calls if a["action"] == "save"]
        self.assertEqual(len(saves), 23)
        self.assertEqual(len(self.http), 46)
        self.assertEqual(self.http[:2], ["/api/v1/me", "/api/v1/orders/order_0"])
        self.assertEqual(saves[0]["order"], {"id": "order_0", "property_id": "property_a", "version": 3,
            "occupants": [{"id": "occupant_0", "phone": " 138-0000-0000 "}, {"id": "other_0", "phone": None}]})
        self.assertNotIn("phone", json.dumps(result))
        self.assertEqual(output.getvalue(), "")

    def test_refresh_binds_original_presentation_and_performs_another_get(self):
        self.add_order()
        self.run_host()
        self.run_host(refresh=True)
        opens = [a for a in self.calls if a["action"] == "open"]
        self.assertEqual(opens[1], {"action": "open", "work_item": self.work, "refresh": True, "presentation": self.presentation})
        self.assertEqual(self.http.count("/api/v1/orders/order_0"), 2)

    def test_malformed_orders_never_become_missing_phone(self):
        original = self.add_order()
        variants = []
        for version in (True, 3.0, "3", -1):
            raw = copy.deepcopy(original)
            raw["order"]["version"] = version
            variants.append(raw)
        for occupants in (None, [{"id": "occupant_0"}], [{"id": "occupant_0", "phone": 123}],
                          [{"id": "duplicate", "phone": None}, {"id": "duplicate", "phone": None}]):
            variants.append({**original, "occupants": occupants})
        variants.append({**original, "order": {**original["order"], "id": "wrong"}})
        variants.append({**original, "order": {**original["order"], "property_id": "wrong"}})
        for raw in variants:
            with self.subTest(raw=variants.index(raw)):
                self.calls.clear()
                self.routes["/api/v1/orders/order_0"] = 200, raw
                self.result = self.summary("incomplete", False)
                self.assertEqual(self.run_host()["status"], "incomplete")
                self.assertFalse(any(a["action"] == "save" for a in self.calls))
                self.assertEqual(self.calls[-2]["reason"], "read_failed")

    def test_read_failure_unavailable_redirect_and_revocation(self):
        self.add_order()
        for status, reason in [(404, "read_failed"), (302, "read_failed"), (0, "read_unavailable")]:
            with self.subTest(status=status):
                self.calls.clear()
                self.routes["/api/v1/orders/order_0"] = status, {"phone": "private"}
                self.result = self.summary("incomplete", False)
                self.run_host()
                self.assertEqual(self.calls[-2]["reason"], reason)
                self.assertFalse(any(a["action"] == "save" for a in self.calls))
        self.allowed = False
        self.http.clear()
        self.run_host()
        self.assertEqual(self.http, ["/api/v1/me"])

    def test_lost_save_ack_reads_status_without_retry_or_failed(self):
        self.add_order()
        self.save_error = True
        self.assertEqual(self.run_host()["status"], "complete")
        self.assertEqual([a["action"] for a in self.calls], ["open", "save", "status"])
        self.status_error = True
        with self.assertRaisesRegex(ValueError, "^stay_contacts_incomplete$"):
            self.run_host()

    def test_empty_pool_and_newer_revision_defer_to_durable_status(self):
        self.result = self.summary("incomplete", False)
        self.assertFalse(self.run_host()["scan_complete"])
        self.assertEqual(self.http, [])
        self.add_order()["order"]["version"] = 4
        self.result = self.summary("awaiting_source_sync", False)
        self.assertEqual(self.run_host()["status"], "awaiting_source_sync")
        self.assertEqual(self.calls[-2]["order"]["version"], 4)

    def test_invalid_pool_rejected_before_http(self):
        self.add_order()
        valid = copy.deepcopy(self.reads)
        for reads in (valid * 2, valid * 201, [{**valid[0], "url": "http://example.invalid"}]):
            self.reads = reads
            with self.assertRaisesRegex(ValueError, "stay_contacts_incomplete"):
                self.run_host()
            self.assertEqual(self.http, [])

    def test_status_whitelist_and_local_gate(self):
        self.result.update(phone="private", read_token="private", welcome_contact_read_v1={"private": True})
        result = self.run_host()
        self.assertNotIn("private", json.dumps(result))
        with self.assertRaisesRegex(ValueError, "local_enable_required"):
            host.StayContactsHost(self.client, self.broker)
        with self.assertRaises(ValueError):
            host.StayContactsHost(self.client, self.broker, local_enabled=True).refresh_contacts(self.work, None)

    def test_one_failed_order_does_not_skip_the_rest_or_claim_complete(self):
        self.add_order(0)
        self.add_order(1)
        self.routes["/api/v1/orders/order_0"] = 404, {"error": "private"}
        self.result = self.summary("incomplete", False)
        result = self.run_host()
        self.assertEqual([a["action"] for a in self.calls], ["open", "failed", "save", "status"])
        self.assertEqual(self.calls[2]["order"]["id"], "order_1")
        self.assertEqual(result["status"], "incomplete")
        self.assertFalse(result["scan_complete"])

    def test_environment_factory_uses_host_token_and_original_context_over_socket(self):
        self.add_order()
        captured, errors = [], []
        with tempfile.TemporaryDirectory(prefix="contacts-", dir="/tmp") as directory:
            path = str(Path(directory) / "broker.sock")
            listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            listener.bind(path)
            listener.listen()
            listener.settimeout(5)
            def serve():
                try:
                    for _ in range(3):
                        connection, _ = listener.accept()
                        with connection, connection.makefile("rb") as stream:
                            request = json.loads(stream.readline())
                            captured.append(request)
                            result = self.broker(request["arguments"])
                            connection.sendall(json.dumps({"ok": True, "result": result}).encode() + b"\n")
                except Exception as error:
                    errors.append(type(error).__name__)
            server = threading.Thread(target=serve)
            server.start()
            context = {"gateway_id": "synthetic-gateway", "platform": "wecom", "chat_type": "group",
                       "chat_id": "synthetic-group", "sender_id": "synthetic-person", "message_id": "original-confirmation"}
            expected_context = copy.deepcopy(context)
            try:
                with patch.dict(os.environ, {"QINTOPIA_PMS_LOCAL_ENABLE": "1", "QINTOPIA_FOUNDATION_LOCAL_ENABLE": "1",
                    "QINTOPIA_APPLICATION_LOCAL_ENABLE": "1", "QINTOPIA_FOUNDATION_SOCKET": path,
                    "QINTOPIA_FOUNDATION_GATEWAY_ID": "synthetic-gateway",
                    "QINTOPIA_FOUNDATION_HOST_TOKEN": "synthetic-host-token-" + "x" * 32,
                    "QINTOPIA_FOUNDATION_TOKEN": "synthetic-model-token-" + "y" * 32,
                    "GREENPMS_BASE_URL": f"http://127.0.0.1:{self.server.server_port}",
                    "GREENPMS_API_TOKEN": "synthetic_contact_credential"}):
                    runner = host.from_environment(context)
                    context["message_id"] = "later-message-must-not-replace-original"
                    self.assertEqual(runner.refresh_contacts(self.work, self.presentation)["status"], "complete")
            finally:
                server.join(6)
                listener.close()
            self.assertFalse(server.is_alive())
            self.assertEqual(errors, [])
            self.assertEqual(len(captured), 3)
            for request in captured:
                self.assertEqual(request["trusted_context"], expected_context)
                self.assertEqual(request["token"], "synthetic-host-token-" + "x" * 32)
                self.assertEqual(request["tool"], "welcome_stay_contacts")
                self.assertEqual(request["operation"], "person_foundation_ingress")
                self.assertEqual(request["agent"], "anan")
                self.assertEqual(request["schema_version"], 1)
