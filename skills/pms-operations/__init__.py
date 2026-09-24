"""Anan-only PMS tools. Internal execution keys and human authority stay off tool schemas."""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import socket
import stat
import sys


def _module(name, filename):
    key = "qintopia_pms_" + name
    if key not in sys.modules:
        spec = importlib.util.spec_from_file_location(key, Path(__file__).with_name(filename))
        module = importlib.util.module_from_spec(spec)
        sys.modules[key] = module
        spec.loader.exec_module(module)
    return sys.modules[key]


client = _module("client", "client.py")
host = _module("host", "host.py")
CATALOG = json.loads(Path(__file__).with_name("operations.json").read_text())["operations"]


def enabled():
    return os.environ.get("QINTOPIA_PMS_LOCAL_ENABLE") == "1" and os.environ.get("QINTOPIA_FOUNDATION_LOCAL_ENABLE") == "1"


def transport(request, *, host=False):
    if not enabled():
        raise ValueError("pms_disabled")
    path = Path(os.environ.get("QINTOPIA_FOUNDATION_SOCKET", ""))
    token = os.environ.get("QINTOPIA_FOUNDATION_HOST_TOKEN" if host else "QINTOPIA_FOUNDATION_TOKEN", "")
    if not path.is_absolute() or path.is_symlink() or not 32 <= len(token) <= 256:
        raise ValueError("foundation_unavailable")
    info = path.stat()
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid():
        raise ValueError("foundation_unavailable")
    raw = json.dumps({**request, "token": token}, ensure_ascii=False, allow_nan=False).encode() + b"\n"
    if len(raw) > client.MAX_BYTES:
        raise ValueError("invalid_arguments")
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(10)
        connection.connect(str(path))
        connection.sendall(raw)
        received = bytearray()
        while not received.endswith(b"\n"):
            chunk = connection.recv(min(4096, client.MAX_BYTES + 1 - len(received)))
            if not chunk:
                raise ValueError("outcome_unknown")
            received.extend(chunk)
            if len(received) > client.MAX_BYTES:
                raise ValueError("outcome_unknown")
    return client.decode(received)


# Output uses positive field selection: free text, notes, source bodies and credentials are omitted.
PUBLIC_FIELDS = set("schemaVersion events eventId eventType sequence action work_item phase operation version method transactionReference pricing pricingDecision pricingBasis policyBaseAmount targetCurrentContractAmount differenceFromPolicy manualAdjustmentMinor differenceExceedsThreshold cashLines cashRemainder coverageSet serviceDate minorUnits inventoryUnit stayType unitKind bookingChannelCode roomId buildingCode roomTypeCode occupancyCapacity unit_code effectiveDate newArrivalDate newDepartureDate newInventoryUnitId settlement currentStatus previousStatus newStatus preview result readback previewId commandType propertyId effectHash effect expiresAt receiptId commandId executionStatus businessCommitted resourceRefs factRefs committedAt order orders quote quoteId totalAmountMinor amountMinor currentContractAmount currentContractAmountMinor collectionDifference netRecordedCollection arrivalDate departureDate inventoryUnitId unitKind units id code name status nickname fullName primaryGuest members memberId property_id inventory_unit_id arrival_date departure_date current_contract_amount_minor net_recorded_collection_minor collection_difference_minor items kind reference occurredAt orderId enabled lastSyncedAt synchronizationError hasMore nextBeforeId nextCursor businessDate available capacity currency pricingPolicyVersionId nights guestCount totals stay stays occupants billId confirmation_hint confirmation_reused pms_reversed replayed linked source_summary".split())


SAFE_ERRORS = {"payment_readback_required", "payment_effect_mismatch","invalid_arguments", "pms_disabled", "pms_property_denied", "pms_command_denied",
    "pms_unavailable", "pms_outcome_unknown", "pms_preview_rejected", "pms_preview_expired", "pms_request_rejected",
    "human_confirmation_required", "business_action_terminal", "business_reconcile_first",
    "business_operation_denied", "business_authority_denied", "business_authority_changed",
    "business_binding_changed", "business_conversation_mismatch", "business_not_awaiting_confirmation",
    "business_agent_not_registered", "trusted_message_evidence_required", "foundation_unavailable",
    "human_manual_reference_required", "business_manual_reference_mismatch", "manual_effect_not_observed",
    "manual_effect_verification_required", "business_handoff_required", "agent_tool_denied",
    "trusted_context_unavailable", "unsupported_command"}
GROUP_PRIVATE = {"primaryGuest", "members", "member", "occupants", "fullName", "nickname"}


def public(value, *, group=False):
    if isinstance(value, dict):
        return {k: public(v, group=group) for k, v in value.items() if k in PUBLIC_FIELDS and not (group and k in GROUP_PRIVATE)}
    if isinstance(value, list):
        return [public(v, group=group) for v in value[:100]]
    return value


class Operations:
    def __init__(self, pms, broker=transport, context=host.session_context):
        self.pms, self.broker, self.context = pms, broker, context

    def call(self, name, args):
        result = self.broker({"operation": "person_foundation_tool", "schema_version": 1,
            "agent": "anan", "tool": name, "trusted_context": self.context(), "arguments": args})
        if not result.get("ok"):
            # Do not reflect arbitrary broker errors into the model.
            code = result.get("error", {}).get("code", "foundation_unavailable")
            raise ValueError(code if code in SAFE_ERRORS else "foundation_unavailable")
        return result["result"]

    def read(self, args):
        operation = "pms.read." + args["query"]
        authority = self.call("pms_authorize", {"binding": args["binding"], "operation": operation})
        return public(self.pms.read(args["query"], authority["property"], resource=args.get("resource"), filters=args.get("filters")))

    def _finish(self, action, claim, receipt, input=None):
        readback = None
        if receipt.get("executionStatus") == "EXECUTED":
            payload = receipt.get("result") or {}
            order = payload.get("order") or payload
            order_id = order.get("orderId") or order.get("id") or (input or {}).get("orderId")
            property_id = order.get("propertyId", order.get("property_id")) or (input or {}).get("propertyId")
            if order_id and property_id:
                try:
                    readback = public(self.pms.read("order", property_id, resource=order_id))
                except client.PmsError:
                    # The committed receipt remains valid; missing readback is not a reason to replay.
                    readback = None
        return self.call("pms_save_result", {"action": action, "claim": claim, "result": public(receipt), "readback": readback})

    def prepare(self, args):
        operation = args["operation"]
        if operation not in CATALOG or "command" not in CATALOG[operation]:
            raise ValueError("unsupported_command")
        args = dict(args)
        if args.get("work_item"):
            event = self.call("pms_event_context", {k: args[k] for k in ("binding", "operation", "work_item")})
            if event and event.get("bill_id"):
                current = self.pms.read("payments", event["property"], filters={"billId": event["bill_id"], "kind": "COLLECTION", "status": "ALL", "limit": 1})
                items = current.get("items", [])
                if len(items) != 1 or items[0].get("id") != event["bill_id"]:
                    raise ValueError("payment_readback_required")
                payment = items[0]
                if (payment.get("status") != "AVAILABLE" or payment.get("kind") != "COLLECTION"
                        or args["input"].get("method") != "WECOM"
                        or payment.get("reference") != args["input"].get("transactionReference")
                        or payment.get("amountMinor") != args["input"].get("amountMinor")):
                    raise ValueError("payment_effect_mismatch")
                args["source_payment"] = {k: payment[k] for k in ("id", "status", "kind", "reference", "amountMinor")}
        started = self.call("pms_start", args)
        action = started["action"]
        if started.get("replayed"):
            status = self.call("pms_status", {"action": action})
            if status["phase"] != "draft":
                return status
        return self._preview_action(action)

    def _preview_action(self, action):
        claimed = self.call("pms_claim_preview", {"action": action})
        command = CATALOG[claimed["operation"]]["command"]
        try:
            if command == "CREATE_QUOTE":
                result = self.pms.quote(claimed["input"], key=claimed["execution_key"], correlation=claimed["correlation"])
                return self._finish(action, claimed["claim"], result["receipt"], claimed["input"])
            result = self.pms.preview(command, claimed["input"], key=claimed["preview_key"], correlation=claimed["correlation"])
            preview = result["preview"]
            preview["propertyId"] = claimed["input"]["propertyId"]
            return self.call("pms_save_preview", {"action": action, "claim": claimed["claim"], "preview": preview})
        except client.PmsError as error:
            if error.code == "pms_preview_rejected":
                return self.call("pms_reject_preview", {"action": action, "claim": claimed["claim"]})
            # Do not retry a lost request here. The durable previewing phase retains its exact key.
            return {"action": action, "phase": "unknown"}

    def execute(self, args):
        claimed = self.call("pms_claim_execute", {"action": args["action"]})
        try:
            result = self.pms.confirm(claimed["preview"], claimed["reason"], key=claimed["execution_key"], correlation=claimed["correlation"])
        except client.PmsError:
            result = {"executionStatus": "UNKNOWN", "businessCommitted": False}
        return self._finish(args["action"], claimed["claim"], result, claimed["input"])

    def resume(self, args):
        self.call("pms_resume", args)
        return self._preview_action(args["action"])

    def recover(self, args):
        status = self.call("pms_status", args)
        if status["phase"] == "draft":
            return self._preview_action(args["action"])
        c = self.call("pms_recovery", args)
        command = CATALOG[c["operation"]]["command"]
        if c["phase"] == "previewing" and command != "CREATE_QUOTE":
            try:
                result = self.pms.preview(command, c["input"], key=c["preview_key"], correlation=c["correlation"])
            except client.PmsError as error:
                if error.code == "pms_preview_rejected":
                    return self.call("pms_reject_preview", {"action": c["action"], "claim": c["claim"]})
                raise
            preview = result["preview"]
            preview["propertyId"] = c["property"]
            return self.call("pms_save_preview", {"action": c["action"], "claim": c["claim"], "preview": preview})
        result = self.pms.recover(c["property"], command, c["execution_key"], resolve_key=c["resolution_key"], correlation=c["correlation"])
        return self._finish(c["action"], c["claim"], result, c["input"])

    def reconcile(self, args):
        context = self.call("pms_manual_context", args)
        observed = self.pms.read("order", context["property"], resource=context["order_ref"])
        return self.call("pms_save_manual", {"action": args["action"], "readback": public(observed)})

    def link(self, args):
        context = self.call("pms_link_context", {"action": args["action"]})
        current = self.pms.read("order", context["property"], resource=context["order_ref"])
        return self.call("pms_link", {**args, "readback": public(current)})

    def invoke(self, name, args):
        validate(name, args)
        if name == "read": result = self.read(args)
        elif name == "prepare": result = self.prepare(args)
        elif name == "execute": result = self.execute(args)
        elif name == "recover": result = self.recover(args)
        elif name == "resume": result = self.resume(args)
        elif name == "reconcile": result = self.reconcile(args)
        elif name == "link": result = self.link(args)
        else: result = self.call("pms_" + name, args)
        return {"ok": True, "result": public(result, group=self.context().get("chat_type") == "group")}


def obj(properties, required):
    return {"type": "object", "properties": properties, "required": required, "additionalProperties": False}


TEXT = {"type": "string", "minLength": 1, "maxLength": 160}
ACTION = obj({"action": TEXT}, ["action"])
SCHEMAS = {
    "read": obj({"binding": TEXT, "query": {"type": "string", "enum": [k[9:] for k in CATALOG if k.startswith("pms.read.")]},
                 "resource": TEXT, "filters": {"type": "object"}}, ["binding", "query"]),
    "prepare": obj({"binding": TEXT, "operation": {"type": "string", "enum": [k for k,v in CATALOG.items() if "command" in v]},
                    "input": {"type": "object"}, "reason": obj({"code": TEXT, "note": {"type": "string", "maxLength": 2000}}, ["code", "note"]),
                    "work_item": TEXT}, ["binding", "operation", "input", "reason"]),
    "link": obj({"action": TEXT, "work_item": TEXT}, ["action", "work_item"]),
    **{key: ACTION for key in ("execute", "recover", "status", "pause", "resume", "cancel", "handoff", "reconcile")},
}


def validate(name, args):
    if name not in SCHEMAS or not isinstance(args, dict): raise ValueError("invalid_arguments")
    schema = SCHEMAS[name]
    if set(args) - set(schema["properties"]) or set(schema["required"]) - set(args): raise ValueError("invalid_arguments")
    def shape(value, spec):
        kind = spec.get("type")
        if kind == "object":
            if not isinstance(value, dict): raise ValueError("invalid_arguments")
            if "properties" in spec:
                if set(spec.get("required", [])) - set(value) or set(value) - set(spec["properties"]):
                    raise ValueError("invalid_arguments")
                for key, child in value.items(): shape(child, spec["properties"][key])
        elif kind == "string":
            if not isinstance(value, str) or not spec.get("minLength", 0) <= len(value) <= spec.get("maxLength", 64000):
                raise ValueError("invalid_arguments")
        if "enum" in spec and value not in spec["enum"]: raise ValueError("invalid_arguments")
    shape(args, schema)
    raw = json.dumps(args, allow_nan=False)
    if len(raw.encode()) > 64000: raise ValueError("invalid_arguments")
    def walk(value):
        if isinstance(value, dict):
            for key, child in value.items():
                if key.lower() in {"approved", "approval", "actor", "person", "token", "url", "propertyid", "idempotencykey", "confirmation", "confirmby"}:
                    raise ValueError("invalid_arguments")
                walk(child)
        elif isinstance(value, list):
            for child in value: walk(child)
    walk(args)


def register(ctx):
    def hook(event, **_):
        if not enabled(): return None
        try: return host.capture(event, transport)
        except Exception: return {"action": "skip", "reason": "business_evidence_unavailable"}
    ctx.register_hook("pre_gateway_dispatch", hook)
    for name, schema in SCHEMAS.items():
        def handler(arguments, _name=name, **_):
            if not enabled(): return json.dumps({"ok": False, "error": {"code": "pms_disabled"}})
            try:
                pms = client.Client(os.environ.get("GREENPMS_BASE_URL", ""), os.environ.get("GREENPMS_API_TOKEN", ""), local_enabled=True)
                result = Operations(pms).invoke(_name, arguments)
            except (ValueError, client.PmsError) as exc:
                code = getattr(exc, "code", str(exc))
                result = {"ok": False, "error": {"code": code if code in SAFE_ERRORS else "pms_operation_unavailable"}}
            except Exception:
                result = {"ok": False, "error": {"code": "pms_operation_unavailable"}}
            return json.dumps(result, ensure_ascii=False)
        tool = "qintopia_pms_" + name
        ctx.register_tool(name=tool, toolset="qintopia", schema={"name": tool,
            "description": "受控客房" + name + "；需要当前人员精确授权，写入确认来自可信人类消息，未知结果先恢复。", "parameters": schema},
            handler=handler, check_fn=enabled, description="岸岸受控 PMS 办理", emoji="🏠")
