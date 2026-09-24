#!/usr/bin/env python3
"""One host-bound local Agent tool call. No Hermes, LLM, database or channel client."""
from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import types
from typing import Any
from uuid import UUID

ROOT = Path(__file__).resolve().parents[3]
MAX_REQUEST_BYTES = 65536
AGENT_OPERATIONS = {
    "anan": ("request_card", "prepare"),
    "huabaosi": ("render_card",),
    "erhua": ("forward_review", "forward"),
}
EMPTY_ARGUMENTS = {"type": "object", "properties": {}, "required": [], "additionalProperties": False}
REFERENCE = {"type": "string", "format": "uuid"}
HASH = {"type": "string", "pattern": r"[0-9a-f]{64}"}
PHASE = {"type": "string", "enum": ["formal", "preview"]}


def object_schema(properties):
    return {"type": "object", "properties": properties, "required": list(properties), "additionalProperties": False}


def text_schema(maximum):
    return {"type": "string", "minLength": 1, "maxLength": maximum}


def canonical(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def validate(value, schema):
    kind = schema["type"]
    if kind == "object":
        if not isinstance(value, dict) or set(value) != set(schema["required"]):
            raise ValueError("invalid_fields")
        for key, child in schema["properties"].items():
            validate(value[key], child)
    elif kind == "string":
        if not isinstance(value, str) or not schema.get("minLength", 1) <= len(value) <= schema.get("maxLength", 8192):
            raise ValueError("invalid_text")
        if any(ord(character) < 32 and character != "\n" for character in value):
            raise ValueError("invalid_text")
        if "enum" in schema and value not in schema["enum"]:
            raise ValueError("invalid_enum")
        if schema.get("format") == "uuid":
            try:
                if str(UUID(value)) != value:
                    raise ValueError("invalid_reference")
            except (ValueError, AttributeError) as error:
                raise ValueError("invalid_reference") from error
        if "pattern" in schema and re.fullmatch(schema["pattern"], value) is None:
            raise ValueError("invalid_hash")
    elif kind == "integer":
        if type(value) is not int or not schema.get("minimum", 0) <= value <= schema.get("maximum", 2**53 - 1):
            raise ValueError("invalid_version")
    elif kind == "array":
        if not isinstance(value, list) or not schema.get("minItems", 0) <= len(value) <= schema.get("maxItems", 100):
            raise ValueError("invalid_list")
        for item in value:
            validate(item, schema["items"])
        if schema.get("uniqueItems") and len(set(value)) != len(value):
            raise ValueError("duplicate_part")
    else:
        raise ValueError("unsupported_schema")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate_field")
        result[key] = value
    return result


def reject_nonfinite(_):
    raise ValueError("invalid_json")


class LocalToolHost:
    """The host supplies task context; registered tools expose no model arguments."""

    def __init__(self, request):
        self.agent = request["agent"]
        self.operation = request["operation"]
        self._trusted_context = copy.deepcopy(request["trusted_context"])
        self.tools = {}

    def register_tool(self, *, name, schema, handler):
        if name in self.tools or schema != EMPTY_ARGUMENTS or not callable(handler):
            raise ValueError("invalid_tool_registration")
        self.tools[name] = (schema, handler)

    def trusted_input(self, *, agent, operation, schema):
        if agent != self.agent or operation != self.operation:
            raise ValueError("agent_tool_denied")
        result = copy.deepcopy(self._trusted_context["input"])
        validate(result, schema)
        return result

    def run(self, name, arguments):
        if name not in self.tools:
            raise ValueError("unknown_tool")
        schema, handler = self.tools[name]
        validate(arguments, schema)
        return handler(arguments)


def execute(raw: bytes) -> dict[str, Any]:
    if not raw or len(raw) > MAX_REQUEST_BYTES:
        raise ValueError("input_too_large")
    request = json.loads(raw, object_pairs_hook=unique_object, parse_constant=reject_nonfinite)
    expected = {"schema_version", "call_id", "agent", "operation", "trusted_context", "arguments"}
    if not isinstance(request, dict) or set(request) != expected:
        raise ValueError("invalid_envelope")
    if type(request["schema_version"]) is not int or request["schema_version"] != 1:
        raise ValueError("unsupported_version")
    validate(request["call_id"], REFERENCE)
    agent = request["agent"]
    if not isinstance(agent, str) or agent not in AGENT_OPERATIONS:
        raise ValueError("unknown_agent")
    if request["operation"] not in AGENT_OPERATIONS[agent]:
        raise ValueError("agent_tool_denied")
    context = request["trusted_context"]
    if not isinstance(context, dict) or set(context) != {"task_ref", "target_agent", "capability_key", "source_type", "input"}:
        raise ValueError("invalid_task_context")
    validate(context["task_ref"], REFERENCE)
    if context["target_agent"] != agent or context["capability_key"] != "resident_welcome.coordinate" or context["source_type"] != "resident_welcome":
        raise ValueError("trusted_welcome_task_required")
    if not isinstance(context["input"], dict):
        raise ValueError("invalid_task_input")
    validate(request["arguments"], EMPTY_ARGUMENTS)
    plugin_path = ROOT / "fixtures/agents" / agent / "welcome_runtime.py"
    source = plugin_path.read_bytes()
    # Execute exactly the bytes whose digest is returned. A loader reopening the
    # path or using cached bytecode would weaken the source identity evidence.
    plugin = types.ModuleType(f"qintopia_welcome_{agent}")
    plugin.__file__ = str(plugin_path)
    exec(compile(source, str(plugin_path), "exec"), plugin.__dict__)
    if plugin.PLUGIN_ID != f"qintopia-welcome-{agent}" or plugin.PLUGIN_VERSION != "0.1.0":
        raise ValueError("plugin_identity_mismatch")
    host = LocalToolHost(request)
    plugin.register(host)
    expected_tools = {f"qintopia_welcome_{operation}" for operation in AGENT_OPERATIONS[agent]}
    if set(host.tools) != expected_tools:
        raise ValueError("plugin_tool_mismatch")
    tool_name = f"qintopia_welcome_{request['operation']}"
    output = host.run(tool_name, request["arguments"])
    if not isinstance(output, dict):
        raise ValueError("invalid_tool_output")
    return {"schema_version": 1, "call_id": request["call_id"], "task_ref": context["task_ref"],
            "agent": agent, "operation": request["operation"], "runtime": "local_scripted_agent_runtime",
            "plugin_id": plugin.PLUGIN_ID, "plugin_version": plugin.PLUGIN_VERSION, "tool_name": tool_name,
            "request_sha256": sha256(raw), "output_sha256": sha256(canonical(output)),
            "plugin_source_sha256": sha256(source), "output": output}


def deny_external_effects(event, _arguments):
    if event.startswith(("socket.", "subprocess.", "os.exec", "os.spawn")) or event in {"os.system", "os.fork", "os.forkpty", "os.posix_spawn"}:
        raise PermissionError("external_effect_forbidden")


def main():
    # No credentials or application config survive into the fixed plugin host. The
    # audited plugins read only their code/fonts; the host exposes no DB/channel API.
    for key in list(os.environ):
        if key not in {"LANG", "LC_ALL", "PYTHONUNBUFFERED", "PYTHONDONTWRITEBYTECODE", "QINTOPIA_WELCOME_TEST_ARTIFACT"}:
            del os.environ[key]
    sys.dont_write_bytecode = True
    sys.addaudithook(deny_external_effects)
    try:
        response = execute(sys.stdin.buffer.read(MAX_REQUEST_BYTES + 1))
        sys.stdout.buffer.write(canonical(response))
    except Exception as error:
        code = str(error) if isinstance(error, (ValueError, PermissionError)) and re.fullmatch(r"[a-z_]{1,80}", str(error)) else "local_agent_runtime_failed"
        sys.stdout.buffer.write(canonical({"ok": False, "error": {"code": code}}))
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
