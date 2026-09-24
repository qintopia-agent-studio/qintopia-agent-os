import importlib.util
import json
from pathlib import Path
import socket
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

spec = importlib.util.spec_from_file_location("pms_client", Path(__file__).parents[1] / "client.py")
pms = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pms)
TOKEN = "synthetic_pms_test_credential"


class ClientTests(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self.routes = {}
        self.me = {"propertyAccess": {"property_a": "WRITE", "property_b": "WRITE"},
                   "allowedActions": {"property_a": ["CREATE_ORDER", "CHECK_IN"]},
                   "propertyCommandGrants": {"property_a": ["CREATE_ORDER", "CHECK_IN"]}}
        owner = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_GET(self): self.respond()
            def do_POST(self): self.respond()
            def respond(self):
                payload = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                owner.calls.append((self.command, self.path, dict(self.headers), json.loads(payload) if payload else None))
                if self.path == "/api/v1/me":
                    status, value = 200, owner.me
                else:
                    status, value = owner.routes.get((self.command, urlsplit(self.path).path), (500, {"error": TOKEN}))
                if status == 0:
                    self.connection.shutdown(socket.SHUT_RDWR)
                    self.connection.close()
                    return
                self.send_response(status)
                if status == 302:
                    self.send_header("Location", "http://example.invalid/leak")
                self.end_headers()
                self.wfile.write(value if isinstance(value, bytes) else json.dumps(value).encode())
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.base = f"http://127.0.0.1:{self.server.server_port}"
        self.client = pms.Client(self.base, TOKEN, local_enabled=True)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def receipt(self, state="EXECUTED"):
        return {"receiptId": "receipt_1", "commandId": "command_1", "executionStatus": state,
                "businessCommitted": state == "EXECUTED", "resourceRefs": ["order_1"]}

    def preview(self):
        return {"previewId": "preview_1", "propertyId": "property_a", "commandType": "CREATE_ORDER", "effectHash": "a" * 64}

    def test_preview_rejection_is_not_an_unknown_business_commit(self):
        self.routes["POST", "/api/v1/command-previews"] = 409, {"code":"INVALID_ORDER_STATE","retryable":False,"message":"private details"}
        with self.assertRaises(pms.PmsError) as raised:
            self.client.preview("CHECK_IN",{"propertyId":"property_a","orderId":"order_1"},key="preview_rejected",correlation="preview_rejected")
        self.assertEqual(raised.exception.code,"pms_preview_rejected")
        self.assertFalse(raised.exception.outcome_unknown)
        self.assertNotIn("private",str(raised.exception))

    def test_configuration_never_accepts_remote_url_or_secret_repr(self):
        for url in ["https://production.example", "http://localhost:4100", "http://127.0.0.1:4100/api", "http://x:y@127.0.0.1:4100", "http://127.0.0.1:4100?token=bad"]:
            with self.assertRaises(pms.PmsError): pms.Client(url, TOKEN, local_enabled=True)
        with self.assertRaises(pms.PmsError): pms.Client(self.base, TOKEN)
        self.assertNotIn(TOKEN, repr(self.client))

    def test_query_routes_filters_scope_and_current_token_grants(self):
        self.routes["GET", "/api/v1/properties/property_a/availability"] = 200, {"propertyId": "property_a", "units": []}
        self.client.read("availability", "property_a", filters={"arrivalDate": "2026-10-01", "departureDate": "2026-10-03", "unitKind": "ROOM"})
        self.assertEqual(parse_qs(urlsplit(self.calls[-1][1]).query)["unitKind"], ["ROOM"])
        self.me["propertyAccess"] = {}
        with self.assertRaisesRegex(pms.PmsError, "pms_property_denied"):
            self.client.read("availability", "property_a")
        self.assertEqual(len(self.calls), 3)

    def test_payment_head_uses_read_grant_and_fixed_route(self):
        self.routes["GET", "/api/v1/external-payment-events/head"] = 200, {"schemaVersion": "pms.payments.v1", "propertyId": "property_a", "headCursor": "42"}
        self.assertEqual(self.client.payment_head("property_a")["headCursor"], "42")
        self.assertEqual(self.calls[-1][1], "/api/v1/external-payment-events/head?propertyId=property_a")
        self.me["propertyAccess"] = {}
        with self.assertRaisesRegex(pms.PmsError, "pms_property_denied"):
            self.client.payment_head("property_a")
        self.assertEqual(self.calls[-1][1], "/api/v1/me")

    def test_order_resource_cannot_cross_property_even_with_multi_property_token(self):
        self.routes["GET", "/api/v1/orders/order_foreign"] = 200, {"order": {"property_id": "property_b"}}
        with self.assertRaisesRegex(pms.PmsError, "pms_property_denied"):
            self.client.read("order", "property_a", resource="order_foreign")
        with self.assertRaises(pms.PmsError):
            self.client.read("order", "property_a", resource="../members")

    def test_unsupported_command_and_revocation_do_not_post(self):
        for command in ["ISSUE_TOKEN", "RECORD_REFUND"]:
            with self.assertRaisesRegex(pms.PmsError, "unsupported_command"):
                self.client.preview(command, {"propertyId": "property_a"}, key="preview_key", correlation="correlation_1")
        self.me["propertyCommandGrants"]["property_a"] = []
        with self.assertRaisesRegex(pms.PmsError, "pms_command_denied"):
            self.client.confirm(self.preview(), {"code": "CREATE_STANDARD_ORDER", "note": ""}, key="confirm_key", correlation="correlation_1")
        self.assertFalse(any(c[0] == "POST" for c in self.calls))

    def test_confirm_uses_exact_effect_and_durable_not_executed_is_result(self):
        self.routes["POST", "/api/v1/command-previews/preview_1/confirm"] = 409, self.receipt("NOT_EXECUTED")
        result = self.client.confirm(self.preview(), {"code": "CREATE_STANDARD_ORDER", "note": ""}, key="confirm_key", correlation="correlation_1")
        self.assertEqual(result["executionStatus"], "NOT_EXECUTED")
        self.assertEqual(self.calls[-1][3]["expectedEffectHash"], "a" * 64)
        self.assertEqual(self.calls[-1][2]["Idempotency-Key"], "confirm_key")

    def test_lost_response_is_unknown_without_retry_then_resolve_original_key(self):
        self.routes["POST", "/api/v1/command-previews/preview_1/confirm"] = 0, {}
        with self.assertRaises(pms.PmsError) as failure:
            self.client.confirm(self.preview(), {"code": "CREATE_STANDARD_ORDER", "note": ""}, key="original_confirm", correlation="correlation_1")
        self.assertTrue(failure.exception.outcome_unknown)
        self.assertEqual(sum(c[0] == "POST" for c in self.calls), 1)
        self.routes["GET", "/api/v1/command-results"] = 200, {"executionStatus": "UNKNOWN", "businessCommitted": False}
        self.routes["POST", "/api/v1/command-results/resolve"] = 200, self.receipt()
        result = self.client.recover("property_a", "CREATE_ORDER", "original_confirm", resolve_key="resolve_key", correlation="correlation_1")
        self.assertTrue(result["businessCommitted"])
        self.assertEqual(self.calls[-1][3]["idempotencyKey"], "original_confirm")
        self.assertEqual(self.calls[-1][2]["Idempotency-Key"], "resolve_key")

    def test_redirect_duplicate_json_and_body_errors_do_not_leak(self):
        for status, body in [(302, {}), (500, {"error": TOKEN}), (200, b'{"x":1,"x":2}')]:
            self.routes["GET", "/api/v1/orders"] = status, body
            before = len(self.calls)
            with self.assertRaises(pms.PmsError) as failure: self.client.read("orders", "property_a")
            self.assertNotIn(TOKEN, str(failure.exception))
            self.assertEqual(len(self.calls) - before, 2)

    def test_integer_funds_are_not_coerced_and_large_response_is_rejected(self):
        self.routes["POST", "/api/v1/command-previews"] = 200, {"preview": self.preview()}
        self.client.preview("CREATE_ORDER", {"propertyId": "property_a", "targetCurrentContractAmountMinor": 12300}, key="preview_key", correlation="correlation_1")
        self.assertIs(type(self.calls[-1][3]["input"]["targetCurrentContractAmountMinor"]), int)
        self.routes["GET", "/api/v1/orders"] = 200, b' ' * (pms.MAX_BYTES + 1)
        with self.assertRaisesRegex(pms.PmsError, "invalid_pms_response"): self.client.read("orders", "property_a")

    def test_disclosure_strips_nested_private_fields(self):
        self.assertEqual(pms.redact({"order": {"phone": "private", "identity_card_number": "private", "id": "order_1"}, "tokenSecret": TOKEN}), {"order": {"id": "order_1"}})


if __name__ == "__main__": unittest.main()
