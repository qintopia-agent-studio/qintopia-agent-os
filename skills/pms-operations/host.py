"""Official Hermes WeCom ingress evidence. No environment fallback for turn identity.

pre_gateway_dispatch runs before pairing; the shared service independently requires a
verified Person and an active scoped gateway. Internal/bot events cannot confirm work.
"""
from __future__ import annotations

import os
import logging


logger = logging.getLogger(__name__)


def _value(value):
    return getattr(value, "value", value)


def validate(context):
    if (context.get("platform") != "wecom" or context.get("chat_type") not in {"direct", "group"}
            or any(not isinstance(v, str) or not v or len(v) > 240 or any(c.isspace() for c in v) for v in context.values())):
        raise ValueError("trusted_context_unavailable")
    return context


def active_profile(explicit):
    # Hermes leaves source.profile empty outside multiplex routing. Use its official
    # process profile accessor, never a model argument or a fabricated session.
    if explicit:
        return explicit
    from hermes_cli.profiles import get_active_profile_name
    return get_active_profile_name()


def session_context():
    from contextvars import copy_context
    from gateway import session_context as sc
    # get_session_env falls back to process environment even after its global
    # engaged latch is set. Read only the official ContextVars in THIS task.
    variables = getattr(sc, "_VAR_MAP", {})
    bound = copy_context()
    def current(name):
        variable = variables.get(name)
        if variable is None or variable not in bound or not isinstance(bound[variable], str):
            raise ValueError("trusted_context_unavailable")
        return bound[variable]
    if active_profile(current("HERMES_SESSION_PROFILE")) != "anan":
        raise ValueError("agent_tool_denied")
    chat_type = current("HERMES_SESSION_CHAT_TYPE")
    return validate({"platform": current("HERMES_SESSION_PLATFORM"),
        "chat_type": "direct" if chat_type == "dm" else chat_type,
        "chat_id": current("HERMES_SESSION_CHAT_ID"),
        "sender_id": current("HERMES_SESSION_USER_ID"),
        "message_id": current("HERMES_SESSION_MESSAGE_ID"),
        "gateway_id": os.environ.get("QINTOPIA_FOUNDATION_GATEWAY_ID", "")})


def capture(event, transport):
    source = getattr(event, "source", None)
    if (source is None or _value(getattr(source, "platform", "")) != "wecom"
            or getattr(event, "internal", True) or getattr(source, "is_bot", True)
            or active_profile(getattr(source, "profile", None)) != "anan"):
        return None
    kind = source.chat_type
    context = validate({"platform": "wecom", "chat_type": "direct" if kind == "dm" else kind,
        "chat_id": source.chat_id, "sender_id": source.user_id,
        "message_id": event.message_id, "gateway_id": os.environ.get("QINTOPIA_FOUNDATION_GATEWAY_ID", "")})
    text = getattr(event, "text", None)
    if not isinstance(text, str) or len(text.encode()) > 16000:
        logger.warning("business_evidence_unavailable")
        return None
    response = transport({"operation": "person_foundation_ingress", "schema_version": 1,
        "agent": "anan", "tool": "pms_capture", "trusted_context": context, "arguments": {"text": text}}, host=True)
    if not response.get("ok"):
        logger.warning("business_evidence_unavailable")
        return None
    return None
