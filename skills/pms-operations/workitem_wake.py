"""Bounded, read-only WorkItem wake from the official Generic webhook."""
from __future__ import annotations

from contextvars import copy_context
import hmac
import os
from pathlib import Path
import uuid


ROUTE = "anan-workitem-wake"
USER_ID = f"webhook:{ROUTE}"
CHAT_PREFIX = f"{USER_ID}:"
TOOL = "qintopia_workitem_read"
PROMPT = "只读回查当前待办。"
DENIED = {"action": "block", "message": "Webhook tool unavailable"}
SKIP = {"action": "skip", "reason": "workitem_wake_rejected"}
FIELDS = ("work_item_id", "kind", "status", "readback_required", "contact_status",
          "requires_human_confirmation")


def _value(value):
    return getattr(value, "value", value)


def _uuid(value):
    if not isinstance(value, str) or len(value) != 36:
        raise ValueError("invalid_uuid")
    parsed = uuid.UUID(value)
    if str(parsed) != value:
        raise ValueError("invalid_uuid")
    return value


def _enabled(production):
    return (os.environ.get("QINTOPIA_PMS_WORKITEM_WAKE_ENABLE") == "1"
            and production.mode() == "production")


def _tool_search_off():
    from tools.tool_search import load_config_readonly
    return load_config_readonly().enabled == "off"


def _private_route_ready():
    """Inspect only the presence of a managed literal; never return or log it."""
    from hermes_cli.managed_scope import get_managed_dir, load_managed_config, load_managed_env
    from hermes_constants import get_hermes_home
    from agent.secret_scope import load_env_file

    if get_managed_dir() is None or "ANAN_WORKITEM_WEBHOOK_SECRET" in os.environ:
        return False
    config = load_managed_config()
    webhook = ((config.get("platforms") or {}).get("webhook") or {})
    extra = webhook.get("extra") or {}
    route = (extra.get("routes") or {}).get(ROUTE) or {}
    secret = route.get("secret")
    other_secrets = [extra.get("secret"), *(
        other.get("secret") for name, other in (extra.get("routes") or {}).items()
        if name != ROUTE and isinstance(other, dict)
    )]
    if (extra.get("host") != "127.0.0.1" or route.get("profile", "default") != "default"
            or route.get("deliver") != "log" or route.get("enabled", True) is not True
            or not isinstance(secret, str) or not 32 <= len(secret) <= 512
            or "${" in secret or secret == "INSECURE_NO_AUTH" or "\n" in secret
            or secret in other_secrets
            or any(key in route for key in ("script", "cron_job", "coalesce", "deliver_only",
                                             "deliver_extra", "skills"))):
        return False
    profile_env = Path(get_hermes_home()) / ".env"
    profile_values = load_env_file(profile_env) if profile_env.is_file() else {}
    return not any(
        hmac.compare_digest(secret, value)
        for value in (*os.environ.values(), *load_managed_env().values(), *profile_values.values())
        if isinstance(value, str)
    )


def available(production):
    try:
        from hermes_cli.profiles import get_active_profile_name
        return (_enabled(production) and get_active_profile_name() == "anan"
                and _private_route_ready() and _tool_search_off())
    except Exception:
        return False


def _valid_event(event, body):
    source = event.source
    if (event.internal is not False or _value(source.platform) != "webhook"
            or source.chat_type != "webhook" or source.user_id != USER_ID
            or source.profile not in (None, "anan") or source.message_id is not None
            or source.is_bot is not False or getattr(source, "profile_route_rejected", False)
            or _value(event.message_type) != "text" or not isinstance(body, dict)
            or set(body) != {"schema_version", "work_item_id"}
            or type(body["schema_version"]) is not int or body["schema_version"] != 1
            or event.prompt_response is not None or event.reply_to_message_id is not None
            or event.reply_to_text is not None or event.reply_to_author_id is not None
            or event.reply_to_author_name is not None or event.reply_to_is_own_message is not False
            or event.ledger_message_id is not None or event.reply_anchor_override is not None
            or event.media_urls or event.media_types or event.media_text_inlined
            or event.auto_skill is not None or event.channel_prompt is not None
            or event.channel_context is not None or event.metadata):
        raise ValueError("invalid_event")
    work_item = _uuid(body["work_item_id"])
    if source.chat_id != CHAT_PREFIX + work_item or event.message_id != work_item:
        raise ValueError("invalid_source")
    return work_item


def inbound(event, *, production):
    """Called by the official pre_gateway_dispatch hook before auth and control."""
    try:
        source = event.source
        if _value(source.platform) != "webhook":
            return None
        body = event.raw_message
        event.allow_gateway_control = False
        event.text = PROMPT
        event.raw_message = None
        if not available(production):
            return SKIP
        work_item = _valid_event(event, body)
        production.require()
        source.profile = "anan"
        source.message_id = work_item
        return None
    except Exception:
        return SKIP


def _session():
    """Require bound task-local Hermes ContextVars, never the env fallback."""
    from gateway import session_context as sc

    bound = copy_context()
    variables = sc._VAR_MAP

    def current(name):
        variable = variables.get(name)
        if variable is None or variable not in bound or not isinstance(bound[variable], str):
            raise ValueError("wake_context_unavailable")
        return bound[variable]

    context = {key: current("HERMES_SESSION_" + key.upper()) for key in
               ("platform", "chat_type", "user_id", "chat_id", "message_id", "profile")}
    if (context["platform"] != "webhook" or context["chat_type"] != "webhook"
            or context["user_id"] != USER_ID or context["profile"] != "anan"
            or not context["chat_id"].startswith(CHAT_PREFIX)):
        raise ValueError("wake_context_unavailable")
    work_item = _uuid(context["message_id"])
    if context["chat_id"] != CHAT_PREFIX + work_item:
        raise ValueError("wake_context_unavailable")
    return work_item


def guard(tool_name, args, *, production, **_):
    """No webhook tool except the one verified read, including bridge wrappers."""
    try:
        from gateway import session_context as sc
        bound = copy_context()
        platform_var = sc._VAR_MAP["HERMES_SESSION_PLATFORM"]
        platform = bound.get(platform_var, "")
    except Exception:
        platform = ""
    if platform != "webhook":
        return DENIED if os.environ.get("HERMES_SESSION_PLATFORM") == "webhook" else None
    if tool_name != TOOL or type(args) is not dict or args or not available(production):
        return DENIED
    try:
        _session()
        production.require()
        return None
    except Exception:
        return DENIED


def _projection(result, work_item):
    if (not isinstance(result, dict) or result.get("work_item_id") != work_item
            or result.get("kind") != "payment" or not set(FIELDS) <= result.keys()):
        raise ValueError("invalid_result")
    projected = {"work_item_id": work_item}
    for name in FIELDS[1:]:
        value = result[name]
        if name in {"readback_required", "requires_human_confirmation"}:
            if type(value) is not bool:
                raise ValueError("invalid_result")
        elif not isinstance(value, str) or len(value) > 64 or any(c.isspace() for c in value):
            raise ValueError("invalid_result")
        projected[name] = value
    if not projected.get("kind") or not projected.get("status"):
        raise ValueError("invalid_result")
    return projected


def read(arguments, *, transport, production):
    """Return only a bounded projection; broker owns binding/scope validation."""
    try:
        if type(arguments) is not dict or arguments or not available(production):
            raise ValueError("invalid_arguments")
        work_item = _session()
        production.require()
        binding = _uuid(os.environ.get("QINTOPIA_PMS_EVENT_BINDING"))
        gateway = os.environ.get("QINTOPIA_FOUNDATION_GATEWAY_ID", "")
        if not gateway or len(gateway) > 240 or any(c.isspace() for c in gateway):
            raise ValueError("invalid_gateway")
        response = transport({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_workitem_read",
            "trusted_context": {"gateway_id": gateway, "platform": "host", "chat_type": "",
                                "chat_id": "", "sender_id": "", "message_id": ""},
            "arguments": {"binding": binding, "work_item": work_item}}, host=True)
        if not isinstance(response, dict) or response.get("ok") is not True:
            raise ValueError("broker_unavailable")
        result = _projection(response.get("result"), work_item)
        return {"ok": True, "result": result}
    except Exception:
        return {"ok": False, "error": {"code": "workitem_wake_unavailable"}}
