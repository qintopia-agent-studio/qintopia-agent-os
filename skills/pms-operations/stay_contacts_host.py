"""Private local PMS contact reads; never registered as a model tool.

The broker owns the eligible pool, evidence generations, matching and confirmation.
Only the fixed PMS client sees raw orders. No private payload is logged or returned.
"""
from __future__ import annotations

import copy
import importlib.util
import os
from pathlib import Path
import re
import uuid


def require(condition):
    if not condition:
        raise ValueError("stay_contacts_invalid_response")


def reference(value):
    require(isinstance(value, str) and bool(value) and len(value) <= 160
            and all(c.isascii() and (c.isalnum() or c in "-_") for c in value))
    return value


def uuid_reference(value):
    require(isinstance(value, str))
    try:
        uuid.UUID(value)
    except ValueError:
        raise ValueError("stay_contacts_invalid_reference") from None
    return value


def project_order(raw, read):
    require(isinstance(raw, dict) and isinstance(raw.get("order"), dict))
    order = raw["order"]
    require(order.get("id") == read["order_id"]
            and order.get("property_id") == read["property_id"])
    require(type(order.get("version")) is int and order["version"] >= 0)
    require(isinstance(raw.get("occupants"), list))
    occupants, seen = [], set()
    for item in raw["occupants"]:
        require(isinstance(item, dict) and "phone" in item)
        occupant = reference(item.get("id"))
        require(occupant not in seen and (item["phone"] is None or isinstance(item["phone"], str)))
        seen.add(occupant)
        occupants.append({"id": occupant, "phone": item["phone"]})
    # Version and complete occupant membership are compared against current projection
    # by the broker. Do not turn a newer PMS version into an HTTP failure here.
    return {"id": order["id"], "property_id": order["property_id"],
            "version": order["version"], "occupants": occupants}


def public_status(result, work_item, *, local_only):
    require(isinstance(result, dict) and result.get("work_item") == work_item)
    uuid_reference(result.get("application"))
    require(result.get("status") in {"pending", "complete", "incomplete", "awaiting_source_sync", "stale"}
            and type(result.get("scan_complete")) is bool and result.get("local_only") is local_only)
    for key in ("pool_count", "orders_total", "orders_done", "failed_count"):
        require(type(result.get(key)) is int and 0 <= result[key] <= 200)
    return {key: result[key] for key in ("work_item", "application", "status", "pool_count",
            "orders_total", "orders_done", "failed_count", "scan_complete", "local_only")}


class StayContactsHost:
    def __init__(self, client, broker, *, local_enabled=False, production_enabled=False):
        if (type(local_enabled) is not bool or type(production_enabled) is not bool
                or local_enabled == production_enabled):
            raise ValueError("stay_contacts_mode_required")
        self.client, self.broker, self.local_only = client, broker, local_enabled

    def refresh_contacts(self, work_item, presentation):
        uuid_reference(presentation)
        return self.synchronize(work_item, presentation=presentation)

    def synchronize(self, work_item, *, presentation=None):
        """Read every broker-issued order once, then consult durable aggregate status."""
        try:
            uuid_reference(work_item)
            request = {"action": "open", "work_item": work_item}
            if presentation is not None:
                request.update(refresh=True, presentation=uuid_reference(presentation))
            opened = self.broker(request)
            public_status(opened, work_item, local_only=self.local_only)
            reads = opened.get("reads")
            require(isinstance(reads, list) and len(reads) <= 200)
            seen, tokens = set(), set()
            # Validate the whole read plan before making any external request.
            for read in reads:
                require(isinstance(read, dict) and set(read) == {
                    "read_token", "order_id", "property_id", "order_revision"})
                key = (reference(read["property_id"]), reference(read["order_id"]))
                token = uuid_reference(read["read_token"])
                require(key not in seen and token not in tokens)
                require(isinstance(read["order_revision"], str) and bool(read["order_revision"])
                        and read["order_revision"].isascii() and read["order_revision"].isdigit())
                seen.add(key)
                tokens.add(token)
            for read in reads:
                args = {"work_item": work_item, "read_token": read["read_token"]}
                try:
                    raw = self.client.read("order", read["property_id"], resource=read["order_id"])
                    order = project_order(raw, read)
                except Exception as error:
                    reason = "read_unavailable" if getattr(error, "code", None) == "pms_unavailable" else "read_failed"
                    try:
                        self.broker({"action": "failed", **args, "reason": reason})
                    except Exception:
                        pass  # An uncertain acknowledgement is resolved only by status.
                    continue
                try:
                    self.broker({"action": "save", **args, "order": order})
                except Exception:
                    # A lost save acknowledgement is not a failed HTTP read. Never
                    # overwrite it with failed or resend the private payload here.
                    pass
            return public_status(self.broker({"action": "status", "work_item": work_item}),
                                 work_item, local_only=self.local_only)
        except Exception:
            raise ValueError("stay_contacts_incomplete") from None


def load_plugin():
    spec = importlib.util.spec_from_file_location("pms_stay_contacts_plugin", Path(__file__).with_name("__init__.py"))
    plugin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin)
    return plugin


def from_environment(trusted_context):
    """Capture the original authenticated context; caller is a private host only."""
    plugin = load_plugin()
    mode = plugin.production.mode()
    application_local = os.environ.get("QINTOPIA_APPLICATION_LOCAL_ENABLE") == "1"
    application_production = os.environ.get("QINTOPIA_APPLICATION_PRODUCTION_ENABLE") == "1"
    if (not plugin.enabled() or mode not in {"local", "production"}
            or application_local == application_production):
        raise ValueError("stay_contacts_mode_required")
    if (mode == "local") != application_local:
        raise ValueError("stay_contacts_mode_required")
    context = copy.deepcopy(trusted_context)
    if not isinstance(context, dict) or context.get("gateway_id") != os.environ.get("QINTOPIA_FOUNDATION_GATEWAY_ID") or not context.get("gateway_id"):
        raise ValueError("stay_contacts_gateway_required")
    if mode == "production":
        uuid_reference(os.environ.get("QINTOPIA_APPLICATION_BINDING"))
        alias = os.environ.get("QINTOPIA_APPLICATION_RESOURCE_ALIAS")
        if not isinstance(alias, str) or not re.fullmatch(r"[a-z][a-z0-9_-]{0,79}", alias):
            raise ValueError("stay_contacts_mode_required")
        plugin.production.require()
        credentials = plugin.credential_values()
        client = plugin.client.Client(plugin.client.PRODUCTION_ORIGIN,
                                      credentials["GREENPMS_API_TOKEN"], production_enabled=True)
    else:
        client = plugin.client.Client(os.environ["GREENPMS_BASE_URL"], os.environ["GREENPMS_API_TOKEN"], local_enabled=True)

    def broker(arguments):
        response = plugin.transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "welcome_stay_contacts", "arguments": arguments,
            "trusted_context": copy.deepcopy(context)}, host=True)
        if response.get("ok") is not True:
            raise ValueError("stay_contacts_broker_rejected")
        return response["result"]

    return StayContactsHost(client, broker, local_enabled=mode == "local",
                            production_enabled=mode == "production")
