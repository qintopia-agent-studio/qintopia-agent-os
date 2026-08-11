"""Dedicated Xiaoman activity skill boundary."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
from typing import Any


TOOL_NAMES = [
    "qintopia_xiaoman_activity_record_get",
    "qintopia_xiaoman_activity_list_by_date",
    "qintopia_xiaoman_activity_plan_table_probe",
    "qintopia_xiaoman_activity_announcement_prepare",
    "qintopia_xiaoman_activity_text_group_message_request_prepare",
    "qintopia_xiaoman_weekly_poster_workflow_prepare",
    "qintopia_xiaoman_public_reply_rewrite",
    "qintopia_xiaoman_activity_status_update",
    "qintopia_xiaoman_activity_gap_update",
    "qintopia_xiaoman_activity_phase_update",
    "qintopia_xiaoman_activity_feishu_field_update",
    "qintopia_xiaoman_activity_handoff_create",
    "qintopia_xiaoman_activity_promotion_review_draft",
    "qintopia_xiaoman_activity_material_summary",
]

SCHEMA_NAMES = {
    "qintopia_xiaoman_activity_record_get": "QINTOPIA_XIAOMAN_ACTIVITY_RECORD_GET_SCHEMA",
    "qintopia_xiaoman_activity_list_by_date": "QINTOPIA_XIAOMAN_ACTIVITY_LIST_BY_DATE_SCHEMA",
    "qintopia_xiaoman_activity_plan_table_probe": "QINTOPIA_XIAOMAN_ACTIVITY_PLAN_TABLE_PROBE_SCHEMA",
    "qintopia_xiaoman_activity_announcement_prepare": "QINTOPIA_XIAOMAN_ACTIVITY_ANNOUNCEMENT_PREPARE_SCHEMA",
    "qintopia_xiaoman_activity_text_group_message_request_prepare": "QINTOPIA_XIAOMAN_ACTIVITY_TEXT_GROUP_MESSAGE_REQUEST_PREPARE_SCHEMA",
    "qintopia_xiaoman_weekly_poster_workflow_prepare": "QINTOPIA_XIAOMAN_WEEKLY_POSTER_WORKFLOW_PREPARE_SCHEMA",
    "qintopia_xiaoman_public_reply_rewrite": "QINTOPIA_XIAOMAN_PUBLIC_REPLY_REWRITE_SCHEMA",
    "qintopia_xiaoman_activity_status_update": "QINTOPIA_XIAOMAN_ACTIVITY_STATUS_UPDATE_SCHEMA",
    "qintopia_xiaoman_activity_gap_update": "QINTOPIA_XIAOMAN_ACTIVITY_GAP_UPDATE_SCHEMA",
    "qintopia_xiaoman_activity_phase_update": "QINTOPIA_XIAOMAN_ACTIVITY_PHASE_UPDATE_SCHEMA",
    "qintopia_xiaoman_activity_feishu_field_update": "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_FIELD_UPDATE_SCHEMA",
    "qintopia_xiaoman_activity_handoff_create": "QINTOPIA_XIAOMAN_ACTIVITY_HANDOFF_CREATE_SCHEMA",
    "qintopia_xiaoman_activity_promotion_review_draft": "QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_REVIEW_DRAFT_SCHEMA",
    "qintopia_xiaoman_activity_material_summary": "QINTOPIA_XIAOMAN_ACTIVITY_MATERIAL_SUMMARY_SCHEMA",
}

EMOJIS = {
    "qintopia_xiaoman_activity_record_get": "📋",
    "qintopia_xiaoman_activity_list_by_date": "📅",
    "qintopia_xiaoman_activity_plan_table_probe": "📅",
    "qintopia_xiaoman_activity_announcement_prepare": "📣",
    "qintopia_xiaoman_activity_text_group_message_request_prepare": "📣",
    "qintopia_xiaoman_weekly_poster_workflow_prepare": "🖼️",
    "qintopia_xiaoman_public_reply_rewrite": "🧹",
    "qintopia_xiaoman_activity_status_update": "🧭",
    "qintopia_xiaoman_activity_gap_update": "🧩",
    "qintopia_xiaoman_activity_phase_update": "🪜",
    "qintopia_xiaoman_activity_feishu_field_update": "📝",
    "qintopia_xiaoman_activity_handoff_create": "🤝",
    "qintopia_xiaoman_activity_promotion_review_draft": "📣",
    "qintopia_xiaoman_activity_material_summary": "🧾",
}

_LEGACY_PLUGIN = None


def _legacy_plugin():
    global _LEGACY_PLUGIN
    if _LEGACY_PLUGIN is not None:
        return _LEGACY_PLUGIN

    plugin_path = (
        Path(__file__).resolve().parents[1]
        / "qintopia-tools"
        / "variants"
        / "xiaoman"
        / "__init__.py"
    )
    spec = importlib.util.spec_from_file_location(
        "qintopia_tools_xiaoman_activity_legacy", plugin_path
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("xiaoman activity legacy plugin cannot be loaded")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    _LEGACY_PLUGIN = module
    return module


def _schema(tool_name: str) -> dict[str, Any]:
    return getattr(_legacy_plugin(), SCHEMA_NAMES[tool_name])


def _handler(tool_name: str):
    return getattr(_legacy_plugin(), f"handle_{tool_name}")


def _json_error(skill_name: str, error: str) -> str:
    return json.dumps(
        {
            "success": False,
            "skill": skill_name,
            "safe_answer_mode": "runtime_package_missing",
            "error": error,
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )


def check_xiaoman_activity_requirements() -> bool:
    try:
        return bool(_legacy_plugin().check_xiaoman_activity_requirements())
    except Exception:
        return False


def __getattr__(name: str):
    if name in SCHEMA_NAMES.values() or name.startswith("handle_qintopia_xiaoman_"):
        return getattr(_legacy_plugin(), name)
    raise AttributeError(name)


def register(ctx) -> None:
    for tool_name in TOOL_NAMES:
        try:
            schema = _schema(tool_name)
            handler = _handler(tool_name)
        except Exception:
            schema = {
                "description": f"{tool_name} is unavailable because the legacy Xiaoman activity bridge failed to load.",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": True},
            }
            handler = lambda args, _tool_name=tool_name, **_: _json_error(
                _tool_name, "xiaoman activity legacy bridge unavailable"
            )
        ctx.register_tool(
            name=tool_name,
            toolset="qintopia",
            schema=schema,
            handler=handler,
            check_fn=check_xiaoman_activity_requirements,
            description=schema["description"],
            emoji=EMOJIS[tool_name],
        )
