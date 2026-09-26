from __future__ import annotations

import importlib.util
import json
import os
import socket
import tempfile
import threading
import unittest
from unittest.mock import patch
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

    def test_welcome_failure_retries_fresh_readback_before_handoff(self):
        calls = []
        class Client:
            def read(inner, record):
                calls.append("GET")
                return self.observe()
        attempts = [0]
        def host(arguments):
            action = arguments["action"]
            calls.append(action)
            if action == "open":
                return {"read_token": "current-token"}
            if action == "save":
                return {"status": "duplicate", "application": "persisted-application"}
            self.assertEqual(arguments, {"action": "reconcile_welcome", "record": "recSyntheticOne"})
            attempts[0] += 1
            if attempts[0] == 1:
                raise ValueError("temporary_handoff_failure")
            return {"status": "awaiting_reliable_stay_link"}
        with self.assertRaises(ValueError):
            adapter.synchronize("recSyntheticOne", Client(), host)
        result = adapter.synchronize("recSyntheticOne", Client(), host)
        self.assertEqual(result["application"], "persisted-application")
        self.assertEqual(result["welcome"]["status"], "awaiting_reliable_stay_link")
        self.assertEqual(calls, ["open", "GET", "save", "reconcile_welcome"] * 2)

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

    def test_candidate_projection_uses_same_normalized_read_after_committed_save(self):
        calls = []
        fixture = self
        class Client:
            def read(inner, record, *, include_fields=False):
                calls.append("GET")
                return adapter.projection(fixture.raw, record, fixture.config, include_fields=include_fields)
        def host(arguments):
            calls.append(arguments["action"])
            if arguments["action"] == "open": return {"read_token": "token"}
            if arguments["action"] == "save":
                self.assertNotIn("fields", arguments["observation"])
                return {"status": "accepted", "application": "saved-application"}
            return {"status": "awaiting_reliable_stay_link"}
        def project(application, fields):
            calls.append("projection")
            self.assertEqual(application, "saved-application")
            self.assertEqual(fields["phone"], "synthetic-contact")
            self.assertNotIn("arrival", fields)
            self.assertNotIn("照片", fields)
            return {"stored": True, "identity_confirmed": False}
        result = adapter.synchronize("recSyntheticOne", Client(), host, project_candidates=project)
        self.assertEqual(calls, ["open", "GET", "save", "projection", "reconcile_welcome"])
        self.assertEqual(result["candidate_projection"], {"stored": True, "identity_confirmed": False})
        self.assertNotIn("synthetic-contact", json.dumps(result))

    def test_candidate_projection_failure_preserves_saved_source_and_dispatch(self):
        fixture = self
        class Client:
            def read(inner, record, *, include_fields=False):
                return adapter.projection(fixture.raw, record, fixture.config, include_fields=include_fields)
        def host(a):
            if a["action"] == "open": return {"read_token": "token"}
            if a["action"] == "save": return {"status": "accepted", "application": "saved", "work_items": ["anan", "silaoshi"]}
            raise ValueError("unknown_handoff")
        def fail(*_): raise ValueError("unknown_projection")
        result = adapter.synchronize("recSyntheticOne", Client(), host, project_candidates=fail)
        self.assertEqual(result["status"], "accepted")
        self.assertEqual(result["work_items"], ["anan", "silaoshi"])
        self.assertIsNone(result["candidate_projection"]["stored"])
        self.assertEqual(result["welcome"]["status"], "handoff_unconfirmed")

    def test_candidate_projection_skips_withdrawn_or_unconsented_source(self):
        fixture = self
        class Client:
            def read(inner, record, *, include_fields=False):
                return adapter.projection(fixture.raw, record, fixture.config, include_fields=include_fields)
        def host(a):
            if a["action"] == "open": return {"read_token": "token"}
            if a["action"] == "save": return {"status": "accepted", "application": "saved"}
            return {"status": "awaiting_reliable_stay_link"}
        def forbidden(*_): self.fail("ineligible candidate must not be uploaded")
        self.raw["data"]["record"]["fields"]["展示"] = "未同意"
        result = adapter.synchronize("recSyntheticOne", Client(), host, project_candidates=forbidden)
        self.assertEqual(result["candidate_projection"]["status"], "not_eligible")
        self.raw["data"]["record"]["fields"]["展示"] = "同意"
        self.raw["data"]["record"]["fields"]["状态"] = "已撤回"
        self.assertFalse(adapter.synchronize("recSyntheticOne", Client(), host, project_candidates=forbidden)["candidate_projection"]["stored"])

    def test_candidate_field_constraints_do_not_rewrite_accepted_values(self):
        values = adapter.projection(self.raw, "recSyntheticOne", self.config, include_fields=True)["fields"]
        self.assertIs(adapter.candidate_fields(values), values)
        for key, invalid in [("name", "长" * 121), ("phone", "1" * 41), ("interests", "x\x00y")]:
            with self.assertRaises(ValueError):
                adapter.candidate_fields({**values, key: invalid})

    def test_remote_source_and_unconfigured_local_source_are_rejected(self):
        for url, enabled in [("https://open.feishu.cn", True), ("http://127.0.0.1:1234", False),
                             ("http://localhost:1234", True), ("http://127.0.0.1:1234?url=x", True)]:
            with self.assertRaises(ValueError):
                adapter.LocalApplicationClient({**self.config, "base_url": url}, "synthetic-test-token", enabled=enabled)

    def production_config(self):
        return {**self.config, "resource_alias": "resident-application",
            "app_id": "synthetic_app_id", "app_secret": "synthetic-app-secret-1234",
            "foundation_host_token": "synthetic-host-token-" + "x" * 32}

    def test_production_mode_requires_both_gates_and_no_local_gate(self):
        keys = {key: "0" for key in ("QINTOPIA_APPLICATION_LOCAL_ENABLE",
            "QINTOPIA_FOUNDATION_LOCAL_ENABLE", "QINTOPIA_APPLICATION_PRODUCTION_ENABLE",
            "QINTOPIA_FOUNDATION_PRODUCTION_ENABLE")}
        with patch.dict(os.environ, keys, clear=True):
            self.assertEqual(adapter.application_mode(), "disabled")
            os.environ["QINTOPIA_APPLICATION_PRODUCTION_ENABLE"] = "1"
            self.assertEqual(adapter.application_mode(), "disabled")
            os.environ["QINTOPIA_FOUNDATION_PRODUCTION_ENABLE"] = "1"
            self.assertEqual(adapter.application_mode(), "production")
            os.environ["QINTOPIA_APPLICATION_LOCAL_ENABLE"] = "1"
            self.assertEqual(adapter.application_mode(), "disabled")

    def test_private_config_rejects_unsafe_path_mode_link_and_schema(self):
        # Production validates every ancestor. Linux /tmp is deliberately
        # world-writable, so use the existing private-credential fixture pattern.
        workspace = Path(__file__).resolve().parents[3] / ".local-workspace"
        workspace.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="application-config-", dir=workspace) as directory, \
                patch.dict(os.environ, {}, clear=True):
            root = Path(directory).resolve()
            path = root / "application.json"
            path.write_text(json.dumps(self.production_config()))
            path.chmod(0o600)
            self.assertEqual(adapter.load_production_config(path, trusted_root=root)["resource_alias"],
                             "resident-application")
            unsafe = root / "writable-parent"
            unsafe.mkdir()
            unsafe.chmod(0o777)
            unsafe_file = unsafe / "application.json"
            unsafe_file.write_text(json.dumps(self.production_config()))
            unsafe_file.chmod(0o600)
            with self.assertRaises(ValueError):
                adapter.load_production_config(unsafe_file, trusted_root=root)
            with self.assertRaises(ValueError):
                adapter.load_production_config(path, trusted_root=root / "different")
            alias = root / "alias.json"
            alias.symlink_to(path)
            with self.assertRaises(ValueError):
                adapter.load_production_config(alias, trusted_root=root)
            hardlink = root / "hardlink.json"
            os.link(path, hardlink)
            with self.assertRaises(ValueError):
                adapter.load_production_config(path, trusted_root=root)
            hardlink.unlink()
            path.chmod(0o644)
            with self.assertRaises(ValueError):
                adapter.load_production_config(path, trusted_root=root)
            path.chmod(0o600)
            for change in ({"extra_url": "https://example.invalid"}, {"fields": {"phone": "电话"}}):
                path.write_text(json.dumps({**self.production_config(), **change}))
                with self.assertRaises(ValueError):
                    adapter.load_production_config(path, trusted_root=root)
            with patch.dict(os.environ, {"QINTOPIA_FOUNDATION_HOST_TOKEN": "model-visible"}):
                with self.assertRaises(ValueError):
                    adapter.load_production_config(path, trusted_root=root)

    def test_production_https_token_and_single_record_get_fail_closed(self):
        requests = []
        responses = [(200, {"code": 0, "tenant_access_token": "synthetic-tenant-token-1234"}),
                     (200, self.raw)]
        class Connection:
            def __init__(inner, host, port, *, timeout, context):
                self.assertEqual((host, port, timeout), ("open.feishu.cn", 443, 10))
                self.assertIsNotNone(context)
            def request(inner, method, path, *, body=None, headers):
                requests.append((method, path, body, headers))
            def getresponse(inner):
                status, payload = responses.pop(0)
                class Response:
                    def read(self, *_): return json.dumps(payload).encode()
                response = Response()
                response.status = status
                return response
            def close(inner): pass
        client = adapter.ProductionApplicationClient(self.production_config())
        with patch.object(adapter.http.client, "HTTPSConnection", Connection):
            result = client.read("recSyntheticOne")
            self.assertTrue(result["valid"])
            self.assertEqual([(method, path) for method, path, *_ in requests], [
                ("POST", "/open-apis/auth/v3/tenant_access_token/internal"),
                ("GET", "/open-apis/bitable/v1/apps/synthetic_base/tables/synthetic_table/records/recSyntheticOne")])
            self.assertNotIn("Authorization", requests[0][3])
            self.assertEqual(requests[1][3]["Authorization"], "Bearer synthetic-tenant-token-1234")
            self.assertNotIn("synthetic-contact", json.dumps(result))
            for source in [(404, self.raw), (403, self.raw), (302, self.raw),
                           (200, {**self.raw, "code": 1}), (200, {"code": 0}),
                           (200, {"code": 0, "data": {"record": {"record_id": "recSyntheticOne",
                               "fields": {"姓名": "x" * adapter.MAX_BYTES}}}})]:
                responses[:] = [(200, {"code": 0, "tenant_access_token": "synthetic-tenant-token-1234"}), source]
                with self.assertRaises(ValueError):
                    client.read("recSyntheticOne")
            responses[:] = [(200, {"code": 1, "tenant_access_token": "synthetic-tenant-token-1234"})]
            with self.assertRaises(ValueError):
                client.read("recSyntheticOne")

    def test_production_readback_uses_original_host_order_and_fixed_context(self):
        calls = []
        config = self.production_config()
        fixture = self
        class Client:
            def __init__(inner, value):
                fixture.assertIs(value, config)
            def read(inner, record, *, include_fields):
                calls.append("GET")
                return adapter.projection(fixture.raw, record, fixture.config, include_fields=include_fields)
        def transport(request):
            self.assertEqual(request["agent"], "anan")
            self.assertEqual(request["operation"], "person_foundation_ingress")
            self.assertEqual(request["trusted_context"], {"gateway_id": "synthetic-gateway",
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""})
            if request["tool"] == "welcome_source_projection":
                calls.append("projection")
                self.assertEqual(request["arguments"]["fields"]["phone"], "synthetic-contact")
                return {"ok": True, "result": {"stored": True, "identity_confirmed": False}}
            action = request["arguments"]["action"]
            calls.append(action)
            self.assertEqual(request["arguments"]["resource_alias"], "resident-application")
            if action == "open": return {"ok": True, "result": {"read_token": "current-token"}}
            if action == "save":
                self.assertNotIn("phone", json.dumps(request["arguments"]["observation"]))
                return {"ok": True, "result": {"status": "accepted", "application": "application-id"}}
            return {"ok": True, "result": {"status": "awaiting_reliable_stay_link"}}
        env = {"QINTOPIA_APPLICATION_LOCAL_ENABLE": "0", "QINTOPIA_FOUNDATION_LOCAL_ENABLE": "0",
            "QINTOPIA_APPLICATION_PRODUCTION_ENABLE": "1", "QINTOPIA_FOUNDATION_PRODUCTION_ENABLE": "1",
            "QINTOPIA_APPLICATION_PRODUCTION_CONFIG": "/etc/qintopia/application.json",
            "QINTOPIA_APPLICATION_RESOURCE_ALIAS": "resident-application",
            "QINTOPIA_APPLICATION_BINDING": "synthetic-binding",
            "QINTOPIA_FOUNDATION_GATEWAY_ID": "synthetic-gateway"}
        with patch.dict(os.environ, env, clear=True), \
                patch.object(adapter.production, "require", side_effect=lambda **_: calls.append("isolation")) as guard, \
                patch.object(adapter, "load_production_config", side_effect=lambda _: (calls.append("config"), config)[1]), \
                patch.object(adapter, "ProductionApplicationClient", Client), \
                patch.object(adapter, "production_transport", return_value=transport):
            result = adapter.readback_one("resident-application", "recSyntheticOne")
        guard.assert_called_once_with(private_paths=(env["QINTOPIA_APPLICATION_PRODUCTION_CONFIG"],))
        self.assertEqual(calls, ["isolation", "config", "open", "GET", "save", "projection", "reconcile_welcome"])
        self.assertEqual(result["status"], "accepted")
        self.assertNotIn("synthetic-contact", json.dumps(result))

    def test_production_host_without_isolation_never_opens_config_or_connects(self):
        env = {"QINTOPIA_APPLICATION_PRODUCTION_ENABLE": "1", "QINTOPIA_FOUNDATION_PRODUCTION_ENABLE": "1",
               "QINTOPIA_PMS_PRODUCTION_ENABLE": "1",
               "QINTOPIA_APPLICATION_PRODUCTION_CONFIG": "/etc/qintopia/application.json"}
        with patch.dict(os.environ, env, clear=True), \
                patch.object(adapter, "load_production_config") as load, \
                patch.object(adapter, "ProductionApplicationClient") as client, \
                patch.object(adapter, "production_transport") as transport:
            with self.assertRaisesRegex(ValueError, "pms_isolation_unavailable"):
                adapter.readback_one("resident-application", "recSyntheticOne")
        load.assert_not_called()
        client.assert_not_called()
        transport.assert_not_called()

    def test_production_host_transport_uses_private_token_without_returning_it(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "foundation.sock"
            listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            listener.bind(str(path))
            listener.listen(1)
            received = []
            def serve():
                connection, _ = listener.accept()
                with connection:
                    received.append(json.loads(connection.recv(4096)))
                    connection.sendall(b'{"ok":true,"result":{"status":"accepted"}}\n')
            thread = threading.Thread(target=serve)
            thread.start()
            try:
                with patch.dict(os.environ, {"QINTOPIA_FOUNDATION_SOCKET": str(path)}):
                    result = adapter.production_transport(self.production_config())({"tool": "pms_application_intake"})
            finally:
                thread.join(5)
                listener.close()
            self.assertEqual(received[0]["token"], self.production_config()["foundation_host_token"])
            self.assertEqual(result, {"ok": True, "result": {"status": "accepted"}})
            self.assertNotIn("synthetic-host-token", json.dumps(result))


if __name__ == "__main__":
    unittest.main()
