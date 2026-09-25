"""Real broker/PG + private contact host; PMS HTTP and channel are simulated."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

root = Path(os.environ["ANAN_CONTACTS_REPO"])
def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, root / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

plugin = load("contacts_broker_plugin", "skills/pms-operations/__init__.py")
contacts = load("contacts_broker_host", "skills/pms-operations/stay_contacts_host.py")
welcome = load("contacts_welcome_host", "skills/person-foundation/welcome_host.py")
fixture = json.loads(Path(os.environ["ANAN_CONTACTS_FIXTURE"]).read_text())
mode = sys.argv[1]
orders = fixture["orders"]
reads = []
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_GET(self):
        reads.append(self.path)
        if self.path == "/api/v1/me":
            value = {"propertyAccess": {order["property_id"]: "READ" for order in orders},
                     "allowedActions": {}, "propertyCommandGrants": {}}
        else:
            matches = [order for order in orders if self.path == "/api/v1/orders/" + order["id"]]
            if len(matches) != 1:
                self.send_error(404)
                return
            order = matches[0]
            value = {"order": {key: order[key] for key in ("id", "property_id", "version")},
                     "occupants": order["occupants"]}
        self.send_response(200)
        self.end_headers()
        self.wfile.write(json.dumps(value).encode())
    def do_POST(self): self.send_error(405)

server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    os.environ["GREENPMS_BASE_URL"] = f"http://127.0.0.1:{server.server_port}"
    os.environ["GREENPMS_API_TOKEN"] = "synthetic-private-read-token"
    runner = contacts.from_environment(fixture["context"])
    if mode == "prime":
        probe = plugin.transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "welcome_stay_contacts",
            "arguments": {"action": "status", "work_item": fixture["application_work"]},
            "trusted_context": fixture["context"]}, host=True)
        assert probe.get("ok") is True, probe.get("error", {}).get("code")
        state = runner.synchronize(fixture["application_work"])
        assert state["status"] == "complete" and state["scan_complete"] is True
        assert len(reads) == len(orders) * 2
    else:
        def broker(arguments):
            response = plugin.transport({"operation": "person_foundation_ingress", "schema_version": 1,
                "agent": "anan", "tool": "welcome_group_host", "arguments": arguments,
                "trusted_context": fixture["context"]}, host=True)
            if response.get("ok") is not True:
                raise ValueError(response["error"]["code"])
            return response["result"]
        group = welcome.WelcomeHost(broker, welcome.SimulatedTransport(), local_enabled=True)
        if mode in {"changed", "cross_application"}:
            if mode == "cross_application":
                try:
                    group.callback()
                    raise AssertionError("other_application_relation_bypassed_refresh")
                except ValueError as error:
                    assert str(error) == "contact_confirmation_refresh_required", str(error)
            try:
                group.callback(refresh_contacts=runner.refresh_contacts)
                raise AssertionError("changed_contact_basis_was_accepted")
            except ValueError as error:
                assert str(error) == "contact_confirmation_basis_changed", str(error)
            assert len(reads) == len(orders) * 2
        else:
            result = group.callback(refresh_contacts=runner.refresh_contacts)
            assert result["identity_confirmed"] is True
            count = len(reads)
            assert count == (0 if mode in {"manual", "existing"} else len(orders) * 2)
            assert group.callback(refresh_contacts=runner.refresh_contacts) == result
            assert len(reads) == count, "replayed_confirmation_must_not_refresh"
    print(json.dumps({"mode": mode, "real_unix_broker": True, "real_pg": True,
                      "pms_http_simulated": True, "channel_simulated": True, "http_reads": len(reads)}))
finally:
    server.shutdown()
    server.server_close()
    thread.join()
