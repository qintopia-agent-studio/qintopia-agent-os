"""Host-only bounded payment catch-up. Not registered as a model tool.

The broker owns source scope, initial baseline and checkpoint. A lost acknowledgement
is recovered by reading durable state on the next run; no finance action is issued.
"""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import socket
import stat


def synchronize(client, host_call, *, max_pages=10):
    if type(max_pages) is not int or not 1 <= max_pages <= 100:
        raise ValueError("invalid_page_budget")
    context = host_call({"action": "context"})
    if context["state"] is None:
        head = client.payment_head(context["propertyId"])
        if (head.get("schemaVersion") != context["schemaVersion"]
                or head.get("propertyId") != context["propertyId"]):
            raise ValueError("payment_source_mismatch")
        # Source and binding version come from the trusted broker, never PMS/model input.
        host_call({"action": "open", "head": {**head,
            "sourceInstance": context["sourceInstance"], "bindingVersion": context["bindingVersion"]}})
    else:
        host_call({"action": "open"})
    for _ in range(max_pages):
        # Re-read each iteration so uncertain earlier writes cannot rewind the cursor.
        context = host_call({"action": "context"})
        cursor = context["state"]["cursor"]
        page = client.read("payment_events", context["propertyId"], filters={"cursor": cursor, "limit": 100})
        result = host_call({"action": "page", "expected": cursor, "page": page})
        if not page["events"]:
            return {"cursor": result["cursor"], "caught_up": True}
    return {"cursor": result["cursor"], "caught_up": False}


def _production_host(plugin):
    plugin.production.require()
    from hermes_constants import get_hermes_home

    credentials = plugin.credentials.load(
        os.environ["QINTOPIA_PMS_CREDENTIALS_FILE"], profile_home=str(get_hermes_home()))
    path = Path(os.environ.get("QINTOPIA_FOUNDATION_SOCKET", ""))
    token = credentials["QINTOPIA_FOUNDATION_HOST_TOKEN"]
    if not path.is_absolute() or path.is_symlink():
        raise ValueError("foundation_unavailable")
    info = path.stat()
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid():
        raise ValueError("foundation_unavailable")

    def transport(request):
        raw = json.dumps({**request, "token": token}, ensure_ascii=False, allow_nan=False).encode() + b"\n"
        if len(raw) > plugin.client.MAX_BYTES:
            raise ValueError("invalid_arguments")
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(10)
            connection.connect(str(path))
            connection.sendall(raw)
            received = bytearray()
            while not received.endswith(b"\n"):
                chunk = connection.recv(min(4096, plugin.client.MAX_BYTES + 1 - len(received)))
                if not chunk or len(received) + len(chunk) > plugin.client.MAX_BYTES:
                    raise ValueError("outcome_unknown")
                received.extend(chunk)
        return plugin.client.decode(received)

    pms = plugin.client.Client(plugin.client.PRODUCTION_ORIGIN,
                               credentials["GREENPMS_API_TOKEN"], production_enabled=True)
    return pms, transport


def main():
    spec = importlib.util.spec_from_file_location("qintopia_pms_feed_plugin", Path(__file__).with_name("__init__.py"))
    plugin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin)
    mode = plugin.production.mode()
    if mode == "local" and os.environ.get("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE") == "1":
        client = plugin.client.Client(os.environ["GREENPMS_BASE_URL"], os.environ["GREENPMS_API_TOKEN"], local_enabled=True)

        def transport(request):
            return plugin.transport(request, host=True)
    elif mode == "production" and os.environ.get("QINTOPIA_PMS_EVENTS_PRODUCTION_ENABLE") == "1":
        client, transport = _production_host(plugin)
    else:
        raise ValueError("payment_feed_disabled")

    def host_call(arguments):
        response = transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_payment_feed", "arguments": arguments,
            "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"],
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""}})
        if not response.get("ok"):
            raise ValueError("payment_feed_rejected")
        return response["result"]

    print(json.dumps(synchronize(client, host_call), separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Neither credentials, source payloads, nor database diagnostics enter logs.
        raise SystemExit("payment_feed_incomplete; inspect durable state before retry") from None
