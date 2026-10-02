"""Bounded host recovery of the existing read-only WorkItem webhook hint."""
from __future__ import annotations

import hashlib
import hmac
import http.client
import importlib.util
import json
import os
from pathlib import Path
import time
import uuid


MAX_PAGES = 10
PAGE_SIZE = 100
MAX_RESPONSE = 4096


def _uuid(value):
    if not isinstance(value, str) or str(uuid.UUID(value)) != value:
        raise ValueError("recovery_invalid_work_item")
    return value


def _current_target(detail):
    target = detail.get("target")
    if not isinstance(target, dict):
        return False
    if "current" in target:
        return (target["current"] is True and bool(target.get("binding_id"))
                and bool(target.get("group_binding_id")) and bool(target.get("config_version")))
    return (type(detail.get("binding_version")) is int
            and type(detail.get("configuration_version")) is int
            and type(target.get("binding_version")) is int
            and target.get("platform") == "wecom"
            and all(isinstance(target.get(key), str) and target[key]
                    for key in ("binding_id", "conversation_id", "chat_id")))


def scan(host_call, hint, *, max_pages=MAX_PAGES):
    """A hint starts only a read-only internal turn; it is never a human receipt."""
    if type(max_pages) is not int or not 1 <= max_pages <= MAX_PAGES:
        raise ValueError("recovery_invalid_budget")
    after = None
    cursors = set()
    seen = set()
    counts = {"hint_accepted": 0, "held": 0, "incomplete": 0}
    for _ in range(max_pages):
        arguments = {"action": "list", "limit": PAGE_SIZE}
        if after is not None:
            arguments["after"] = after
        page = host_call(arguments)
        if (not isinstance(page, dict) or not isinstance(page.get("items"), list)
                or len(page["items"]) > PAGE_SIZE):
            raise ValueError("recovery_invalid_page")
        for item in page["items"]:
            if not isinstance(item, dict) or item.get("kind") != "payment":
                counts["held"] += 1
                continue
            work = _uuid(item.get("work_item"))
            if work in seen:
                raise ValueError("recovery_duplicate_work_item")
            seen.add(work)
            if (item.get("status") != "awaiting_review"
                    or item.get("delivery_state", "pending") != "pending"):
                counts["held"] += 1
                continue
            detail = host_call({"action": "detail", "work_item": work})
            if (not isinstance(detail, dict) or detail.get("work_item") != work
                    or detail.get("kind") != "payment" or detail.get("status") != "awaiting_review"):
                raise ValueError("recovery_invalid_detail")
            sending = detail.get("send")
            if (detail.get("contact_required") is not True or not _current_target(detail)
                    or not isinstance(sending, dict)
                    or sending.get("key") or sending.get("receipt")
                    or sending.get("phase") not in (None, "pending")):
                counts["held"] += 1
                continue
            try:
                hint(work)
            except Exception:
                counts["incomplete"] += 1
            else:
                counts["hint_accepted"] += 1
        after = page.get("next")
        if after is None:
            return {**counts, "more": False}
        if not isinstance(after, str) or not after or len(after) > 512 or after in cursors:
            raise ValueError("recovery_invalid_cursor")
        cursors.add(after)
    return {**counts, "more": True}


def send_internal_hint(work, *, wake):
    """Use the same fixed official Generic webhook protocol as the Sidecar hint."""
    _uuid(work)
    if (not isinstance(wake, dict) or wake.get("host") != "127.0.0.1"
            or type(wake.get("port")) is not int or not 1 <= wake["port"] <= 65535
            or not isinstance(wake.get("secret"), str)
            or not 32 <= len(wake["secret"]) <= 512):
        raise ValueError("recovery_webhook_unavailable")
    body = json.dumps({"schema_version": 1, "work_item_id": work}, separators=(",", ":")).encode()
    timestamp = str(int(time.time()))
    signature = hmac.new(wake["secret"].encode(), timestamp.encode() + b"." + body,
                         hashlib.sha256).hexdigest()
    connection = http.client.HTTPConnection("127.0.0.1", wake["port"], timeout=2)
    try:
        connection.request("POST", "/webhooks/anan-workitem-wake", body=body, headers={
            "Content-Type": "application/json", "X-Request-ID": work,
            "X-Webhook-Timestamp": timestamp, "X-Webhook-Signature-V2": signature})
        response = connection.getresponse()
        raw = response.read(MAX_RESPONSE + 1)
        if response.status not in (200, 202) or len(raw) > MAX_RESPONSE:
            raise ValueError("recovery_webhook_unavailable")
        result = json.loads(raw)
        if (result.get("delivery_id") != work or (response.status, result.get("status"))
                not in ((202, "accepted"), (200, "duplicate"))):
            raise ValueError("recovery_webhook_unavailable")
    except Exception:
        raise ValueError("recovery_webhook_unavailable") from None
    finally:
        connection.close()


def main():
    spec = importlib.util.spec_from_file_location(
        "qintopia_pms_recovery_plugin", Path(__file__).with_name("__init__.py"))
    plugin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin)
    if (os.environ.get("QINTOPIA_PMS_RECOVERY_PRODUCTION_ENABLE") != "1"
            or plugin.production.mode() != "production"):
        raise ValueError("recovery_disabled")
    plugin.production.require()
    if not plugin.workitem_wake.available(plugin.production):
        raise ValueError("recovery_webhook_unavailable")
    from hermes_cli.managed_scope import load_managed_config
    extra = load_managed_config()["platforms"]["webhook"]["extra"]
    route = extra["routes"][plugin.workitem_wake.ROUTE]
    wake = {"host": extra["host"], "port": extra["port"], "secret": route["secret"]}

    def host_call(arguments):
        response = plugin.transport({"operation": "person_foundation_ingress",
            "schema_version": 1, "agent": "anan", "tool": "pms_workitem_recovery",
            "trusted_context": {"gateway_id": os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"],
                "platform": "host", "chat_type": "", "chat_id": "", "sender_id": "",
                "message_id": ""}, "arguments": arguments}, host=True)
        if not isinstance(response, dict) or response.get("ok") is not True:
            raise ValueError("recovery_broker_unavailable")
        return response["result"]

    print(json.dumps(scan(host_call, lambda work: send_internal_hint(work, wake=wake)),
                     separators=(",", ":")))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        raise SystemExit("recovery_incomplete; inspect durable WorkItem before retry") from None
