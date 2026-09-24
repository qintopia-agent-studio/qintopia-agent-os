"""Single bounded cron invocation; only an explicit local recording channel is available.

The broker owns plans and attempt identity. Never retry a send after an uncertain
outcome. A later invocation may only recover the exact adapter receipt.
"""
from __future__ import annotations
import importlib.util
import json
import os
from pathlib import Path
import uuid


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class LocalRecordingAdapter:
    """A simulated external boundary, never a production or fallback channel."""
    def __init__(self, directory, *, enabled=False, profile=None):
        path = Path(directory)
        if not enabled or profile != "anan" or not path.is_absolute() or path.is_symlink():
            raise ValueError("local_reminder_channel_required")
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
        if path.stat().st_mode & 0o077:
            raise ValueError("private_reminder_channel_required")
        self.directory = path

    def _path(self, claim):
        return self.directory / (str(uuid.UUID(claim)) + ".json")

    def lookup(self, claim):
        path = self._path(claim)
        if not path.exists():
            return None
        if path.is_symlink() or path.stat().st_size > 16384:
            raise ValueError("invalid_reminder_receipt")
        return json.loads(path.read_text())["receipt"]

    def send(self, request):
        plan = request["plan"]
        if request["status"] != "send" or plan["profile"] != "anan" or plan["simulated"] is not True:
            raise ValueError("local_reminder_channel_required")
        receipt = {"claim": request["claim"], "profile": "anan", "simulated": True,
                   "destination": plan["destination"], "signature": plan["signature"],
                   "message_id": "simulated-" + str(uuid.uuid4())}
        # Exclusive creation makes duplicate attempts visible, never an implicit resend.
        with self._path(request["claim"]).open("x", encoding="utf-8") as stream:
            os.chmod(stream.name, 0o600)
            json.dump({"receipt": receipt, "text": request["text"]}, stream, ensure_ascii=False)
            stream.flush()
            os.fsync(stream.fileno())
        return receipt


def read_observation(context, pms, application):
    applications = []
    for source in context["applications"]:
        if application is None or source["resource_alias"] != application.config["resource_alias"]:
            raise ValueError("reminder_application_source_unavailable")
        observed = application.read(source["record"], include_fields=True)
        applications.append({**observed, "record": source["record"],
                             "resource_alias": source["resource_alias"]})
    order = None
    if context.get("order"):
        raw = pms.read("order", context["property"], resource=context["order"])
        order = {"order": {k: raw["order"].get(k) for k in
                           ("id", "property_id", "version", "status", "arrival_date")},
                 "amounts": raw.get("amounts")}
    return {"applications": applications, "order": order}


def run_once(pms, host_call, adapter, *, application=None, after=None):
    page = host_call({"action": "list", "after": after})
    results = []
    for work in page["works"]:
        def call(action, **args):
            return host_call({"action": action, "work": work, **args})
        try:
            context = call("context")
            state = context.get("state") or {}
            if state.get("phase") == "unknown":
                receipt = adapter.lookup(state["claim"])
                results.append(call("settle", claim=state["claim"], receipt=receipt)
                               if receipt else {"status": "unknown"})
                continue
            if state.get("phase") == "claimed":
                # A claim has no send permission until validate commits UNKNOWN.
                # Resume that exact pre-send claim; never create another one.
                claimed = {"claim": state["claim"]}
            else:
                observed = read_observation(context, pms, application)
                plan = call("observe", observation=observed)
                if plan["status"] != "due":
                    results.append({"status": plan["status"]})
                    continue
                claimed = call("claim", observation=observed, signature=plan["signature"])
                if claimed["status"] != "claimed":
                    results.append({"status": claimed["status"]})
                    continue
            # Read facts a second time immediately before the broker's one-shot send gate.
            current = call("context")
            request = call("validate", claim=claimed["claim"],
                           observation=read_observation(current, pms, application))
            if request["status"] != "send":
                results.append({"status": request["status"]})
                continue
            receipt = adapter.send(request)
            results.append(call("settle", claim=claimed["claim"], receipt=receipt))
        except Exception:
            # No payload, credential or private source data in logs. The next wakeup
            # inspects persisted state; it does not infer a failed send from this error.
            results.append({"status": "incomplete"})
    return {"results": results, "next": page["next"], "simulated": True}


def main():
    plugin = load("pms_reminder_plugin", "__init__.py")
    if not plugin.enabled() or os.environ.get("QINTOPIA_PMS_REMINDERS_LOCAL_ENABLE") != "1":
        raise ValueError("reminder_disabled")
    adapter = LocalRecordingAdapter(os.environ["QINTOPIA_PMS_REMINDER_SIMULATED_OUTBOX"],
                                    enabled=True, profile="anan")
    pms = plugin.client.Client(os.environ["GREENPMS_BASE_URL"], os.environ["GREENPMS_API_TOKEN"], local_enabled=True)
    application = None
    if os.environ.get("QINTOPIA_APPLICATION_LOCAL_ENABLE") == "1":
        source = load("pms_reminder_application", "application_intake.py")
        config = source.decode(Path(os.environ["QINTOPIA_APPLICATION_LOCAL_CONFIG"]).read_bytes())
        application = source.LocalApplicationClient(config, os.environ["QINTOPIA_APPLICATION_LOCAL_API_TOKEN"], enabled=True)

    def host_call(arguments):
        response = plugin.transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_reminder", "arguments": arguments,
            "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"],
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""}}, host=True)
        if response.get("ok") is not True:
            raise ValueError("reminder_broker_rejected")
        return response["result"]
    print(json.dumps(run_once(pms, host_call, adapter, application=application), separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        raise SystemExit("reminder_incomplete; inspect durable state before retry") from None
