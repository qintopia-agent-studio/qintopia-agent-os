"""Host-only bounded payment catch-up. Not registered as a model tool.

The broker owns source scope, initial baseline and checkpoint. A lost acknowledgement
is recovered by reading durable state on the next run; no finance action is issued.
"""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path


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


def main():
    spec = importlib.util.spec_from_file_location("qintopia_pms_feed_plugin", Path(__file__).with_name("__init__.py"))
    plugin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin)
    if not plugin.enabled() or os.environ.get("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE") != "1":
        raise ValueError("payment_feed_disabled")
    client = plugin.client.Client(os.environ["GREENPMS_BASE_URL"], os.environ["GREENPMS_API_TOKEN"], local_enabled=True)

    def host_call(arguments):
        response = plugin.transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_payment_feed", "arguments": arguments,
            "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"],
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""}}, host=True)
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
