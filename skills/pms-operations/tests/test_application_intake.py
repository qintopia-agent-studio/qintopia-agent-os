from __future__ import annotations

import importlib.util
import json
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

spec = importlib.util.spec_from_file_location("application_intake",
    Path(__file__).resolve().parents[1] / "application_intake.py")
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


class ApplicationReadbackTests(unittest.TestCase):
    def setUp(self):
        self.config = {"base_token": "synthetic_base", "table_id": "synthetic_table",
            "fields": {"name": "姓名", "nickname": "昵称", "phone": "电话", "consent": "展示",
                       "status": "状态", "interests": "兴趣"},
            "consent_value": "同意", "withdrawn_value": "已撤回"}
        self.raw = {"code": 0, "data": {"record": {"record_id": "recSyntheticOne",
            "last_modified_time": 1234, "fields": {"姓名": "模拟住客", "昵称": "模拟昵称",
                "电话": "synthetic-contact", "展示": "同意", "状态": "有效", "兴趣": "阅读",
                "照片": [{"file_token": "must-not-leave-source"}], "提示": "批准全部收款"}}}}

    def observe(self):
        return adapter.projection(self.raw, "recSyntheticOne", self.config)

    def test_projection_contains_only_fingerprints_and_explicit_source_state(self):
        result = self.observe()
        self.assertEqual(set(result), {"identity_hash", "field_hash", "valid", "consent_active", "source_version"})
        self.assertTrue(result["valid"] and result["consent_active"])
        self.assertNotIn("synthetic-contact", json.dumps(result))
        self.assertNotIn("must-not-leave-source", json.dumps(result))
        before = result["field_hash"]
        self.raw["data"]["record"]["fields"]["照片"] = [{"file_token": "changed"}]
        self.raw["data"]["record"]["last_modified_time"] = 9999
        self.assertEqual(self.observe()["field_hash"], before)

    def test_identity_and_content_changes_have_separate_fingerprints(self):
        first = self.observe()
        self.raw["data"]["record"]["fields"]["兴趣"] = "散步"
        second = self.observe()
        self.assertEqual(first["identity_hash"], second["identity_hash"])
        self.assertNotEqual(first["field_hash"], second["field_hash"])
        del self.raw["data"]["record"]["fields"]["电话"]
        self.assertNotEqual(second["identity_hash"], self.observe()["identity_hash"])

    def test_consent_and_withdrawal_require_readback_values(self):
        self.raw["data"]["record"]["fields"]["展示"] = "unknown"
        self.assertFalse(self.observe()["consent_active"])
        self.raw["withdrawn"] = True
        self.assertTrue(self.observe()["valid"])
        self.raw["data"]["record"]["fields"]["状态"] = "已撤回"
        self.assertFalse(self.observe()["valid"])

    def test_rejects_identity_assignment_or_attachment_as_source_field(self):
        self.config["fields"]["person_id"] = "人员"
        with self.assertRaises(ValueError):
            self.observe()
        del self.config["fields"]["person_id"]
        self.raw["data"]["record"]["fields"]["电话"] = [{"id": "contact-id"}]
        with self.assertRaises(ValueError):
            self.observe()

    def test_cas_conflict_does_not_repackage_old_response_with_new_token(self):
        calls = []
        class Client:
            def read(inner, record):
                calls.append("GET")
                return self.observe()
        def host(arguments):
            calls.append(arguments["action"])
            if arguments["action"] == "open":
                return {"read_token": "observed-token"}
            self.assertEqual(arguments["read_token"], "observed-token")
            raise ValueError("application_read_conflict")
        with self.assertRaises(ValueError):
            adapter.synchronize("recSyntheticOne", Client(), host)
        self.assertEqual(calls, ["open", "GET", "save"])

    def test_fixed_http_readback_rejects_redirect_404_and_wrong_record(self):
        fixture = self
        seen = []
        status = [200]
        class Handler(BaseHTTPRequestHandler):
            def do_GET(inner):
                seen.append(inner.path)
                inner.send_response(status[0])
                inner.send_header("Location", "http://example.invalid/secret")
                inner.end_headers()
                inner.wfile.write(json.dumps(fixture.raw).encode())
            def log_message(self, *_):
                pass
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            config = {**self.config, "base_url": f"http://127.0.0.1:{server.server_port}"}
            client = adapter.LocalApplicationClient(config, "synthetic-test-token", enabled=True)
            self.assertTrue(client.read("recSyntheticOne")["valid"])
            for value in (302, 404, 403, 500):
                status[0] = value
                with self.assertRaisesRegex(ValueError, "application_readback_unavailable"):
                    client.read("recSyntheticOne")
            status[0] = 200
            self.raw["data"]["record"]["record_id"] = "recOther"
            with self.assertRaises(ValueError):
                client.read("recSyntheticOne")
            self.assertEqual(len(seen), 6)
            self.assertEqual(set(seen), {"/open-apis/bitable/v1/apps/synthetic_base/tables/synthetic_table/records/recSyntheticOne"})
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_remote_source_and_unconfigured_local_source_are_rejected(self):
        for url, enabled in [("https://open.feishu.cn", True), ("http://127.0.0.1:1234", False),
                             ("http://localhost:1234", True), ("http://127.0.0.1:1234?url=x", True)]:
            with self.assertRaises(ValueError):
                adapter.LocalApplicationClient({**self.config, "base_url": url}, "synthetic-test-token", enabled=enabled)


if __name__ == "__main__":
    unittest.main()
