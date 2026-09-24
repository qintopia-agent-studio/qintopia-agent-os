"""Actual signed Bridge Unix ingress -> local HTTP readback -> real PG host broker.

Launched only by the explicit isolated Rust fixture. No production Feishu or sends.
"""
from __future__ import annotations

import importlib.util
import json
import os
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

root = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("application_bridge",
    root / "workflows/silaoshi-daily-ops/bin/script_action_bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
record = os.environ["ANAN_APPLICATION_TEST_RECORD"]
fields = {"姓名": "模拟住客", "昵称": "模拟昵称", "电话": "synthetic-contact",
          "展示": "同意", "状态": "有效", "兴趣": "阅读"}
http_status = [200]
reads = []


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        reads.append(self.path)
        expected = "/open-apis/bitable/v1/apps/synthetic_base/tables/synthetic_table/records/" + record
        self.send_response(http_status[0] if self.path == expected else 404)
        self.end_headers()
        self.wfile.write(json.dumps({"code": 0, "data": {"record": {
            "record_id": record, "fields": fields}}}).encode())

    def log_message(self, *_):
        pass


def until(predicate):
    for _ in range(150):
        if predicate():
            return
        time.sleep(0.05)
    raise AssertionError("local application journey timeout")


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    with tempfile.TemporaryDirectory(prefix="anan-app-", dir="/tmp") as directory:
        work = Path(directory)
        secret = work / "secret"
        secret.write_text("synthetic-bridge-secret-" * 3)
        secret.chmod(0o600)
        cfg = {"socket_path": str(work / "bridge.sock"), "database_path": str(work / "bridge.sqlite3"),
               "secret_file": str(secret), "action": {"name": "silaoshi-base-notify",
                    "command": ["/definitely-not-invoked"], "script_sha256": "0" * 64},
               "application_intake": {"local_only": True, "resource_alias": "resident-application",
                                      "record_path": ["record_id"]}}
        config = work / "bridge.json"
        config.write_text(json.dumps(cfg))
        source = work / "source.json"
        source.write_text(json.dumps({"base_url": f"http://127.0.0.1:{server.server_port}",
            "base_token": "synthetic_base", "table_id": "synthetic_table",
            "resource_alias": "resident-application", "fields": {"name": "姓名", "nickname": "昵称",
                "phone": "电话", "consent": "展示", "status": "状态", "interests": "兴趣"},
            "consent_value": "同意", "withdrawn_value": "已撤回"}))
        source.chmod(0o600)
        env = {**os.environ, "QINTOPIA_APPLICATION_LOCAL_CONFIG": str(source),
               "QINTOPIA_APPLICATION_LOCAL_API_TOKEN": "synthetic-application-token"}
        process = subprocess.Popen([sys.executable, str(root / "workflows/silaoshi-daily-ops/bin/script_action_bridge.py"),
                                    "serve", "--config", str(config)], env=env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            until(lambda: Path(cfg["socket_path"]).exists() or process.poll() is not None)
            assert process.poll() is None, "bridge did not start"

            def send(*, tamper=False):
                envelope = bridge.signed_envelope({"record_id": record, "person_id": "not-authority",
                    "approved": True, "withdrawn": True}, cfg)
                if tamper:
                    envelope["signature"] = "0" * 64
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                    connection.settimeout(5)
                    connection.connect(cfg["socket_path"])
                    connection.sendall(bridge.canonical_json(envelope) + b"\n")
                    return json.loads(connection.recv(8192))

            def state():
                with sqlite3.connect(cfg["database_path"]) as db:
                    return db.execute("SELECT requested_version,completed_version,last_error_code FROM application_wakes").fetchone()

            def wake(version):
                assert send()["status"] == "accepted"
                until(lambda: state() is not None and state()[1] == version)

            wake(1)
            fields["兴趣"] = "散步"
            wake(2)
            fields["电话"] = "synthetic-new-contact"
            wake(3)
            http_status[0] = 404
            assert send()["status"] == "accepted"
            until(lambda: state()[2] == "readback_pending")
            assert state()[:2] == (4, 3)
            http_status[0] = 200
            wake(5)
            fields["状态"] = "已撤回"
            wake(6)
            assert send(tamper=True)["status"] == "rejected"
            assert state()[:2] == (6, 6)
            with sqlite3.connect(cfg["database_path"]) as db:
                assert db.execute("SELECT count(*) FROM jobs").fetchone()[0] == 0
            assert len(reads) == 6
            print(json.dumps({"signed_unix_ingress": True, "http_readbacks": len(reads),
                "repeat_record_callback": True, "unreadable_stayed_pending": True,
                "legacy_actions": 0, "source_withdrawal_read": True}))
        finally:
            process.terminate()
            process.wait(timeout=10)
finally:
    server.shutdown()
    server.server_close()
    thread.join()
