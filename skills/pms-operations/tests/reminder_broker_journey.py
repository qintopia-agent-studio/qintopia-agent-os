"""Real Unix broker + PG + plugin client; PMS HTTP and outgoing channel are simulated."""
import json
import os
from pathlib import Path
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util

root = Path(__file__).resolve().parents[1]
def load(name, file):
    spec = importlib.util.spec_from_file_location(name, root / file)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module
plugin = load("reminder_broker_plugin", "__init__.py")
host = load("reminder_broker_host", "reminder_host.py")
work = os.environ["ANAN_REMINDER_WORK"]
request = {"operation": "person_foundation_ingress", "schema_version": 1, "agent": "anan",
    "tool": "pms_reminder", "arguments": {"action": "context", "work": work},
    "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"], "platform": "host",
                        "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""}}
assert plugin.transport(request, host=False)["ok"] is False
assert plugin.transport({**request, "agent": "erhua"}, host=True)["ok"] is False
assert plugin.transport({**request, "trusted_context": {**request["trusted_context"], "gateway_id": "wrong"}}, host=True)["ok"] is False
assert plugin.transport({**request, "operation": "person_foundation_tool"}, host=False)["ok"] is False

def call(arguments):
    reply = plugin.transport({**request, "arguments": arguments}, host=True)
    assert reply["ok"], reply.get("error")
    return reply["result"]

order = {"order": {"id": "synthetic_order", "property_id": "property_a", "version": 1, "status": "RESERVED", "arrival_date": "2026-01-01"},
         "amounts": {"currentContractAmount": {"currency": "CNY", "minorUnits": 12000}, "netRecordedCollection": {"currency": "CNY", "minorUnits": 0}}}
reads = []
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        reads.append(self.path)
        if self.path == "/api/v1/me":
            value = {"propertyAccess": {"property_a": "READ"}, "propertyCommandGrants": {}, "allowedActions": {}}
        elif self.path == "/api/v1/orders/synthetic_order": value = order
        else:
            self.send_error(404); return
        self.send_response(200); self.end_headers(); self.wfile.write(json.dumps(value).encode())
    def do_POST(self): self.send_error(405)
    def log_message(self, *_): pass
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
try:
    pms = plugin.client.Client(f"http://127.0.0.1:{server.server_port}", "synthetic-read-token", local_enabled=True)
    with tempfile.TemporaryDirectory() as directory:
        adapter = host.LocalRecordingAdapter(directory, enabled=True, profile="anan")
        def lose_settle(arguments):
            if arguments["action"] == "settle": raise OSError("simulated_lost_ack")
            return call(arguments)
        first = host.run_once(pms, lose_settle, adapter)
        assert first["results"] == [{"status": "incomplete"}], first
        context = call({"action": "context", "work": work})
        assert context["state"]["phase"] == "unknown"
        assert len(list(Path(directory).iterdir())) == 1
        count = len(reads)
        second = host.run_once(pms, call, adapter)
        assert second["results"] == [{"status": "sent", "simulated": True}], second
        assert len(reads) == count, "receipt recovery must not re-read or resend"
        assert host.run_once(pms, call, adapter)["results"] == [{"status": "waiting"}]
        order["order"]["status"] = "CHECKED_IN"; order["order"]["version"] = 2
        order["amounts"]["netRecordedCollection"]["minorUnits"] = 12000
        assert host.run_once(pms, call, adapter)["results"] == [{"status": "stopped"}]
        assert len(list(Path(directory).iterdir())) == 1
        print(json.dumps({"real_unix_broker": True, "real_pg": True, "pms_http_simulated": True,
                          "channel_simulated": True, "messages": 1, "completed_stop": True}))
finally:
    server.shutdown(); server.server_close(); thread.join()
