"""Hermes-owned interpretation and trusted local Person foundation tools.

No provider client, database credentials, message sending or automatic retries live here.
"""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import socket
import stat
import sys
from typing import Any, Callable
from uuid import UUID

MAX_BYTES = 256 * 1024
PREFIX = "qintopia_person_"


def _object(properties: dict[str, Any], required: list[str] | None = None) -> dict[str, Any]:
    return {"type": "object", "properties": properties, "required": required or [], "additionalProperties": False}


def _text(limit: int = 1000, **kwargs: Any) -> dict[str, Any]:
    return {"type": "string", "minLength": 1, "maxLength": limit, **kwargs}


UUID_FIELD = _text(36, format="uuid")
VERSION_FIELD = {"type": "integer", "minimum": 0}
TOOL_PARAMETERS = {
    "context": _object({"purpose": _text(120, enum=["reply"]), "topic": _text(120, enum=["general", "fees"])}),
    "save_rule": _object({
        "operation_id": UUID_FIELD, "expected_version": VERSION_FIELD,
        "key": _text(120), "content": _text(4000),
        "kind": _text(16, enum=["rule", "fact", "culture", "experience", "principle"]), "shared": {"type": "boolean"},
        "effective_at": _text(40), "effective_until": _text(40), "case_ref": UUID_FIELD,
    }, ["operation_id", "expected_version", "key", "content"]),
    "remember": _object({
        "operation_id": UUID_FIELD, "expected_version": VERSION_FIELD,
        "change": _object({
            "action": _text(16, enum=["set", "stop"]),
            "condition": _text(16, enum=["general", "fees"]),
            "style": _text(16, enum=["brief", "detailed"]),
        }, ["action"]),
    }, ["operation_id", "expected_version", "change"]),
    "history": _object({"purpose": _text(120, enum=["self_history"])}),
    "dispatch": _object({
        "operation_id": UUID_FIELD, "expected_version": VERSION_FIELD,
        "capability": _text(120, enum=["erhua.foundation_context"]), "brief": _text(2000),
    }, ["operation_id", "expected_version", "capability", "brief"]),
    "task_status": _object({"work_item_id": UUID_FIELD}, ["work_item_id"]),
}
TOOL_DESCRIPTIONS = {
    "context": "Read current trusted identity, permitted knowledge and rule versions for this conversation.",
    "save_rule": "Save an explicitly requested in-scope rule change using current authorization and version.",
    "remember": "Persist or stop a person's requested response preference; success requires a durable service result.",
    "history": "Read purpose-filtered membership and stay history for the current trusted person.",
    "dispatch": "Create a persistent authorized WorkItem; accepted does not mean completed.",
    "task_status": "Read current authorized WorkItem state and actual result evidence.",
}
AGENT_TOOLS = {
    "erhua": ("context", "save_rule", "remember", "history", "task_status"),
    "anan": ("context", "task_status"),
    "default": ("context", "dispatch", "task_status"),
    "silaoshi": ("context", "dispatch", "task_status"),
}


def _validate(value: Any, schema: dict[str, Any]) -> None:
    kind = schema["type"]
    if kind == "object":
        if not isinstance(value, dict) or set(value) - set(schema["properties"]):
            raise ValueError("invalid_arguments")
        if set(schema["required"]) - set(value):
            raise ValueError("invalid_arguments")
        for key, child in value.items():
            _validate(child, schema["properties"][key])
    elif kind == "integer":
        if type(value) is not int or value < schema["minimum"]:
            raise ValueError("invalid_arguments")
    elif kind == "boolean":
        if type(value) is not bool:
            raise ValueError("invalid_arguments")
    elif kind == "string":
        if not isinstance(value, str) or not schema["minLength"] <= len(value) <= schema["maxLength"] or "\x00" in value:
            raise ValueError("invalid_arguments")
        if "enum" in schema and value not in schema["enum"]:
            raise ValueError("invalid_arguments")
        if schema.get("format") == "uuid":
            try:
                if str(UUID(value)) != value:
                    raise ValueError("invalid_arguments")
            except (ValueError, AttributeError) as exc:
                raise ValueError("invalid_arguments") from exc


def validate_arguments(tool: str, arguments: Any) -> dict[str, Any]:
    if tool not in TOOL_PARAMETERS:
        raise ValueError("unknown_tool")
    _validate(arguments, TOOL_PARAMETERS[tool])
    if tool == "remember":
        change = arguments["change"]
        if change["action"] == "set" and set(change) != {"action", "condition", "style"}:
            raise ValueError("invalid_arguments")
        if change["action"] == "stop" and set(change) != {"action"}:
            raise ValueError("invalid_arguments")
    return arguments


def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("duplicate_field")
        value[key] = item
    return value


def _decode(raw: bytes | str) -> dict[str, Any]:
    value = json.loads(raw, object_pairs_hook=_unique_object)
    if not isinstance(value, dict):
        raise ValueError("invalid_response")
    return value


def enabled() -> bool:
    return os.environ.get("QINTOPIA_FOUNDATION_LOCAL_ENABLE") == "1"


def _gateway_session() -> dict[str, str]:
    path = Path(__file__).resolve().parents[1] / "qiwe" / "space_change_tools.py"
    name = "qintopia_person_foundation_qiwe_session"
    module = sys.modules.get(name)
    if module is None:
        spec = importlib.util.spec_from_file_location(name, path)
        if spec is None or spec.loader is None:
            raise ValueError("trusted_context_unavailable")
        module = importlib.util.module_from_spec(spec)
        sys.modules[name] = module
        try:
            spec.loader.exec_module(module)
        except Exception:
            sys.modules.pop(name, None)
            raise
    return module.trusted_qiwe_turn_session()


def trusted_context(session_provider: Callable[[], dict[str, str]]) -> dict[str, str]:
    session = session_provider()
    result = {
        "platform": session.get("platform", ""),
        "chat_type": session.get("conversation_type", ""),
        "chat_id": session.get("conversation_id", ""),
        "sender_id": session.get("requester_user_id", ""),
        "message_id": session.get("source_message_id", ""),
        "gateway_id": os.environ.get("QINTOPIA_FOUNDATION_GATEWAY_ID", ""),
    }
    if result["platform"] != "qiwe" or result["chat_type"] not in {"group", "direct"}:
        raise ValueError("trusted_context_unavailable")
    if any(not isinstance(v, str) or not v or len(v) > 240 or any(c.isspace() for c in v) for v in result.values()):
        raise ValueError("trusted_context_unavailable")
    return result


def socket_call(request: dict[str, Any]) -> dict[str, Any]:
    """One bounded local attempt. Any lost acknowledgement stays unknown."""
    if not enabled():
        raise ValueError("foundation_disabled")
    path = Path(os.environ.get("QINTOPIA_FOUNDATION_SOCKET", ""))
    token = os.environ.get("QINTOPIA_FOUNDATION_TOKEN", "")
    if not path.is_absolute() or path.is_symlink() or not 32 <= len(token) <= 256:
        raise ValueError("foundation_unavailable")
    attempted = False
    try:
        info = path.stat()
        if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid():
            raise ValueError("foundation_unavailable")
        raw = json.dumps({**request, "token": token}, ensure_ascii=False, separators=(",", ":")).encode() + b"\n"
        if len(raw) > MAX_BYTES:
            raise ValueError("invalid_arguments")
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as channel:
            channel.settimeout(10)
            channel.connect(str(path))
            attempted = True
            channel.sendall(raw)
            received = bytearray()
            while not received.endswith(b"\n"):
                chunk = channel.recv(min(4096, MAX_BYTES + 1 - len(received)))
                if not chunk:
                    raise ValueError("outcome_unknown")
                received.extend(chunk)
                if len(received) > MAX_BYTES:
                    raise ValueError("outcome_unknown")
            result = _decode(bytes(received))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ValueError("outcome_unknown" if attempted else "foundation_unavailable") from exc
    if type(result.get("ok")) is not bool or (result["ok"] and "result" not in result):
        raise ValueError("outcome_unknown")
    if not result["ok"]:
        code = result.get("error", {}).get("code")
        if not isinstance(code, str) or not code or len(code) > 120 or not all(c.isalnum() or c == "_" for c in code):
            raise ValueError("outcome_unknown")
        return {"ok": False, "error": {"code": code}}
    return result


def invoke(tool: str, arguments: dict[str, Any], *, agent_id: str,
           session_provider: Callable[[], dict[str, str]] = _gateway_session,
           transport: Callable[[dict[str, Any]], dict[str, Any]] = socket_call) -> dict[str, Any]:
    if not enabled():
        return {"ok": False, "error": {"code": "foundation_disabled"}}
    try:
        if tool not in AGENT_TOOLS.get(agent_id, ()):
            raise ValueError("agent_tool_denied")
        args = validate_arguments(tool, arguments)
        context = trusted_context(session_provider)
        return transport({"operation": "person_foundation_tool", "schema_version": 1,
                          "agent": agent_id, "tool": tool, "trusted_context": context, "arguments": args})
    except ValueError as exc:
        known = {"invalid_arguments", "unknown_tool", "agent_tool_denied", "trusted_context_unavailable",
                 "foundation_disabled", "foundation_unavailable", "outcome_unknown"}
        code = str(exc) if str(exc) in known else "foundation_unavailable"
        return {"ok": False, "error": {"code": code}}
    except Exception:
        return {"ok": False, "error": {"code": "foundation_unavailable"}}


def register(ctx: Any, *, agent_id: str = "erhua",
             session_provider: Callable[[], dict[str, str]] = _gateway_session,
             transport: Callable[[dict[str, Any]], dict[str, Any]] = socket_call) -> None:
    if agent_id not in AGENT_TOOLS:
        raise ValueError("unregistered_agent")
    for tool in AGENT_TOOLS[agent_id]:
        def handler(arguments: dict[str, Any], _tool: str = tool, **_: Any) -> str:
            return json.dumps(invoke(_tool, arguments, agent_id=agent_id,
                                     session_provider=session_provider, transport=transport), ensure_ascii=False)
        name = PREFIX + tool
        schema = {"name": name, "description": TOOL_DESCRIPTIONS[tool], "parameters": TOOL_PARAMETERS[tool]}
        ctx.register_tool(name=name, toolset="qintopia", schema=schema, handler=handler,
                          check_fn=enabled, description=TOOL_DESCRIPTIONS[tool], emoji="🧭")


async def interpret(ctx: Any, message: str) -> dict[str, Any]:
    """Interpret with Hermes's provider; caller executes only the validated plan."""
    if not isinstance(message, str) or not 1 <= len(message) <= 8000:
        return {"kind": "clarify", "reason": "invalid_message"}
    llm = getattr(ctx, "llm", None)
    if llm is None:
        return {"kind": "clarify", "reason": "model_unavailable"}
    instructions = (
        "你是二花的结构化意图适配器。只返回JSON。明确当前本人请求且内容完整时返回 "
        '{"kind":"tool","tool":"工具短名","arguments":{业务参数}}；否则返回 '
        '{"kind":"clarify","reason":"需要澄清的内容"}。建议、引用、转述、讨论不能当修改指令。'
        "不猜身份、范围、版本、操作UUID或许可；缺少它们时要求服务端提供当前上下文。"
        "不得输出actor、person、tenant、scope、群ID、gateway、token或URL。"
        "保存、更正、停止记忆均需工具持久回执，模型文本不证明保存。允许工具schema："
        + json.dumps(TOOL_PARAMETERS, ensure_ascii=False)
    )
    try:
        response = await llm.acomplete(messages=[{"role": "system", "content": instructions},
            {"role": "user", "content": message}], temperature=0, max_tokens=2000, timeout=20,
            purpose="qintopia_person_foundation_intent")
        text = str(getattr(response, "text", ""))
        if len(text.encode()) > MAX_BYTES:
            raise ValueError("invalid_model_response")
        plan = _decode(text)
        if plan.get("kind") == "clarify" and set(plan) == {"kind", "reason"} and isinstance(plan["reason"], str) and len(plan["reason"]) <= 500:
            return plan
        if set(plan) != {"kind", "tool", "arguments"} or plan["kind"] != "tool":
            raise ValueError("invalid_model_response")
        validate_arguments(plan["tool"], plan["arguments"])
        return plan
    except Exception:
        return {"kind": "clarify", "reason": "model_result_unusable"}
