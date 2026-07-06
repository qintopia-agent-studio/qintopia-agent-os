"""Qintopia Hermes-native knowledge, GIS, and complaint workflow tools.

Knowledge and GIS tools are read-only. Complaint tools are narrow write-capable
wrappers for Erhua's controlled complaint/service-recovery workflow; they create
or update only complaint_intake cards and leave dispatch with 大总管/default.
"""

from __future__ import annotations

import asyncio
import hashlib
import importlib
import importlib.util
import json
import logging
import os
import re
import shlex
import sys
import textwrap
from concurrent.futures import ThreadPoolExecutor, TimeoutError
from datetime import datetime, timezone
from threading import Lock
from urllib import error as urlerror
from urllib import request as urlrequest
from pathlib import Path
from typing import Any
from urllib.parse import quote, urlencode


DEFAULT_INDEX_DIR = Path("/home/ubuntu/.hermes/qintopia-knowledge/indexes")
INDEX_FILES = {
    "Public": "public.jsonl",
    "Internal": "internal.jsonl",
    "Member-scoped": "member-scoped.jsonl",
}
DEFAULT_KB_LIMIT = 5
MAX_KB_LIMIT = 10
DEFAULT_GIS_LIMIT = 3
DEFAULT_DIFY_KB_BASE_URL = "https://qintopia.cn/remote/v1"
DEFAULT_DIFY_LIMIT = 20
MAX_DIFY_LIMIT = 100
DEFAULT_DIFY_TIMEOUT_SECONDS = 10
DEFAULT_MESSAGE_STORE_LIMIT = 20
MAX_MESSAGE_STORE_LIMIT = 50
DEFAULT_MESSAGE_STORE_EMBEDDING_TIMEOUT_SECONDS = 10
DEFAULT_MESSAGE_STORE_SEMANTIC_CANDIDATE_LIMIT = 40
MAX_MESSAGE_STORE_SEMANTIC_CANDIDATE_LIMIT = 100
DEFAULT_QINTOPIA_WEATHER_LOCATION = "108.5876,33.9996"
DEFAULT_QINTOPIA_WEATHER_LOCATION_NAME = "秦托邦"
DEFAULT_QINTOPIA_WEATHER_QWEATHER_CITY = "鄠邑区"
DEFAULT_QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS = 12
DEFAULT_OPEN_METEO_TIMEOUT_SECONDS = 8
QINTOPIA_WEATHER_TOOL = "qintopia_weather_lookup"
QINTOPIA_TENANT = "qintopia"
COMPLAINT_TASK_TYPE = "complaint_intake"
COMPLAINT_OWNER_PROFILE = "default"
COMPLAINT_BOARD = "default"
SALES_OWNER_PROFILE = "xiaoqin"
SALES_BOARD = "default"
SALES_TASK_TYPES = {
    "sales_lead": {"status": "triage", "label": "商务线索"},
    "demo_request": {"status": "triage", "label": "演示请求"},
    "proposal": {"status": "todo", "label": "方案提案"},
    "external_disclosure_review": {"status": "review", "label": "对外披露复核"},
}
PUBLIC_AGENT_OS_BASELINES = [
    "Qintopia Agent OS 是面向组织协作场景的 Agent 工作系统。",
    "系统可以把客户需求、知识检索、方案草拟、演示准备、任务流转和交接沉淀到可追踪流程中。",
    "系统强调可控披露、任务可追踪、人工审批和多角色协作。",
    "报价、合同、正式交付范围、排期、SLA 和客户案例细节需要团队负责人确认。",
]
XIAOQIN_SAFE_FOLLOWUP_MESSAGE = "我先帮您记录下来，稍后由团队同事继续跟进确认。"
DAILY_DIGEST_PUBLISH_TOOL = "qintopia_daily_digest_publish"
XIAOMAN_ACTIVITY_TOOL_NAMES = [
    "qintopia_xiaoman_activity_record_get",
    "qintopia_xiaoman_activity_list_by_date",
    "qintopia_xiaoman_activity_status_update",
    "qintopia_xiaoman_activity_gap_update",
    "qintopia_xiaoman_activity_handoff_create",
    "qintopia_xiaoman_activity_material_summary",
]
XIAOMAN_ACTIVITY_TABLE_ROLES = ["activity_plan", "activity_occurrence"]
XIAOMAN_ACTIVITY_HANDOFF_TYPES = [
    "visual_asset_request",
    "ops_followup",
    "member_notice",
    "human_confirmation",
    "activity_recap",
]
XIAOMAN_ACTIVITY_HANDOFF_TARGETS = ["huabaosi", "silaoshi", "erhua", "default"]
QWEATHER_ALLOWED_MCP_TOOLS = {
    "get_weather_now",
    "get_hourly_weather",
    "get_minutely_5m",
    "get_warning",
    "get_air_quality",
}
QWEATHER_FORBIDDEN_TOOL_PATTERNS = {
    "cyclone",
    "typhoon",
    "tropical",
    "storm_track",
    "ocean",
    "marine",
    "tide",
    "tidal",
    "ocean_current",
    "tidal_current",
    "wave",
    "seawater",
    "solar",
    "radiation",
    "poi",
    "station",
    "台风",
    "热带气旋",
    "海洋",
    "潮汐",
    "潮流",
    "浪高",
    "海温",
    "太阳辐射",
    "兴趣点",
    "监测站",
}
QWEATHER_IMPORT_LOCK = Lock()


_OPERATIONS_INTAKE_PLUGIN = None


def _skill_plugin_candidates(skill_name: str) -> list[Path]:
    current = Path(__file__).resolve()
    candidates: list[Path] = []

    skills_dir = os.getenv("QINTOPIA_AGENT_OS_SKILLS_DIR")
    if skills_dir:
        candidates.append(Path(skills_dir) / skill_name / "__init__.py")

    release_dir = os.getenv("QINTOPIA_AGENT_OS_RELEASE_DIR")
    if release_dir:
        candidates.append(Path(release_dir) / "skills" / skill_name / "__init__.py")

    monorepo_dir = os.getenv("QINTOPIA_AGENT_OS_MONOREPO_DIR")
    if monorepo_dir:
        candidates.append(Path(monorepo_dir) / "skills" / skill_name / "__init__.py")

    for parent in current.parents:
        candidates.append(parent / "skills" / skill_name / "__init__.py")
        if parent.name in {"skills", "plugins"}:
            candidates.append(parent / skill_name / "__init__.py")

    deduped: list[Path] = []
    seen: set[str] = set()
    for candidate in candidates:
        key = str(candidate)
        if key not in seen:
            deduped.append(candidate)
            seen.add(key)
    return deduped


def _load_skill_plugin(skill_name: str, module_name: str):
    checked_paths = _skill_plugin_candidates(skill_name)
    for plugin_path in checked_paths:
        if not plugin_path.exists():
            continue
        spec = importlib.util.spec_from_file_location(module_name, plugin_path)
        if not spec or not spec.loader:
            continue
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    checked = ", ".join(str(path) for path in checked_paths)
    raise RuntimeError(f"Cannot load {skill_name} skill. Checked paths: {checked}")


def _operations_intake_plugin():
    global _OPERATIONS_INTAKE_PLUGIN
    if _OPERATIONS_INTAKE_PLUGIN is not None:
        return _OPERATIONS_INTAKE_PLUGIN
    _OPERATIONS_INTAKE_PLUGIN = _load_skill_plugin("operations-intake", "operations_intake_plugin")
    return _OPERATIONS_INTAKE_PLUGIN


def _configured_operations_intake_plugin():
    plugin = _operations_intake_plugin()
    plugin.configure_runtime(
        kanban_create_complaint=_kanban_create_complaint,
        kanban_add_complaint_comment=_kanban_add_complaint_comment,
        kanban_create_sales_task=_kanban_create_sales_task,
        kb_search_handler=handle_qintopia_kb_search,
    )
    return plugin


QINTOPIA_KB_SEARCH_SCHEMA = {
    "description": (
        "Search Qintopia approved knowledge snapshot indexes. Defaults to "
        "Public-only. Member-scoped content requires an explicit scoped request."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Natural language query or keywords.",
            },
            "information_classes": {
                "type": "array",
                "items": {"type": "string", "enum": ["Public", "Internal", "Member-scoped"]},
                "description": "Allowed information classes. Defaults to Public only.",
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_KB_LIMIT,
                "description": "Maximum result count.",
            },
            "caller": {
                "type": "string",
                "description": "Calling profile id, e.g. erhua or wenyuange.",
            },
            "task_id": {
                "type": "string",
                "description": "Optional Kanban task id for audit context.",
            },
            "purpose": {
                "type": "string",
                "description": "Why this search is needed.",
            },
            "allow_member_scoped": {
                "type": "boolean",
                "description": "Must be true to search Member-scoped records.",
            },
        },
        "required": ["query"],
        "additionalProperties": False,
    },
}


QINTOPIA_GIS_LOCATION_LOOKUP_SCHEMA = {
    "description": (
        "Look up a Qintopia public GIS location, such as QinTopia buildings, "
        "from the WenYuanGe Public GIS snapshot. Returns structured coordinates "
        "for a channel adapter to send as a location card."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Location query, e.g. 1 栋, 秦托邦A栋, or 秦托邦社区.",
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 5,
                "description": "Maximum candidate count.",
            },
            "caller": {
                "type": "string",
                "description": "Calling profile id, e.g. erhua.",
            },
            "task_id": {
                "type": "string",
                "description": "Optional Kanban task id for audit context.",
            },
        },
        "required": ["query"],
        "additionalProperties": False,
    },
}


QINTOPIA_WEATHER_LOOKUP_SCHEMA = {
    "description": (
        "Look up Qintopia weather through a narrow QWeather MCP wrapper. "
        "It is fixed to Qintopia coordinates, uses Open-Meteo only as a limited "
        "fallback, and never exposes typhoon, ocean, solar-radiation, POI, or "
        "arbitrary-city weather capabilities."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Original member weather question.",
            },
            "intent": {
                "type": "string",
                "enum": ["current", "umbrella", "thunderstorm", "warning", "air_quality", "general"],
                "description": "Weather intent. Defaults to general.",
            },
            "hours": {
                "type": "integer",
                "minimum": 1,
                "maximum": 24,
                "description": "Forecast horizon in hours. Defaults to 24 and is capped at 24.",
            },
        },
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_DATASET_LIST_SCHEMA = {
    "description": (
        "List Dify Knowledge datasets through the configured Knowledge Service API. "
        "If QINTOPIA_DIFY_ALLOWED_DATASET_IDS is set, results are filtered to that allowlist."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "page": {"type": "integer", "minimum": 1, "description": "Page number."},
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_DIFY_LIMIT,
                "description": "Maximum datasets to return.",
            },
            "keyword": {"type": "string", "description": "Optional dataset title keyword."},
        },
        "required": ["purpose"],
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_DATASET_GET_SCHEMA = {
    "description": "Get read-only metadata for one allowed Dify Knowledge dataset.",
    "parameters": {
        "type": "object",
        "properties": {
            "dataset_id": {"type": "string", "description": "Dify dataset id."},
        },
        "required": ["dataset_id"],
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_KNOWLEDGE_RETRIEVE_SCHEMA = {
    "description": (
        "Retrieve matching chunks from one allowed Dify Knowledge dataset. "
        "This is a read operation even though Dify exposes it as POST."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "dataset_id": {"type": "string", "description": "Dify dataset id."},
            "query": {"type": "string", "description": "Natural language query."},
            "top_k": {
                "type": "integer",
                "minimum": 1,
                "maximum": 10,
                "description": "Maximum retrieved chunks. Defaults to 5.",
            },
            "search_method": {
                "type": "string",
                "enum": ["semantic_search", "full_text_search", "hybrid_search"],
                "description": "Dify retrieval mode. Defaults to semantic_search.",
            },
            "score_threshold_enabled": {
                "type": "boolean",
                "description": "Whether Dify should apply a score threshold.",
            },
            "score_threshold": {
                "type": "number",
                "minimum": 0,
                "maximum": 1,
                "description": "Score threshold when score_threshold_enabled is true.",
            },
            "reranking_enable": {
                "type": "boolean",
                "description": "Whether Dify should rerank results. Defaults to false.",
            },
        },
        "required": ["dataset_id", "query"],
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_DOCUMENT_LIST_SCHEMA = {
    "description": "List documents in one allowed Dify Knowledge dataset.",
    "parameters": {
        "type": "object",
        "properties": {
            "dataset_id": {"type": "string", "description": "Dify dataset id."},
            "page": {"type": "integer", "minimum": 1},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_DIFY_LIMIT},
            "keyword": {"type": "string", "description": "Optional document keyword."},
        },
        "required": ["dataset_id"],
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_DOCUMENT_GET_SCHEMA = {
    "description": "Get read-only metadata/details for one Dify Knowledge document.",
    "parameters": {
        "type": "object",
        "properties": {
            "dataset_id": {"type": "string", "description": "Dify dataset id."},
            "document_id": {"type": "string", "description": "Dify document id."},
        },
        "required": ["dataset_id", "document_id"],
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_INDEXING_STATUS_GET_SCHEMA = {
    "description": "Get indexing status for a Dify document creation/update batch.",
    "parameters": {
        "type": "object",
        "properties": {
            "dataset_id": {"type": "string", "description": "Dify dataset id."},
            "batch": {"type": "string", "description": "Batch id returned by Dify document APIs."},
        },
        "required": ["dataset_id", "batch"],
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_SEGMENT_LIST_SCHEMA = {
    "description": "List chunks/segments for one Dify Knowledge document.",
    "parameters": {
        "type": "object",
        "properties": {
            "dataset_id": {"type": "string", "description": "Dify dataset id."},
            "document_id": {"type": "string", "description": "Dify document id."},
            "page": {"type": "integer", "minimum": 1},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_DIFY_LIMIT},
            "keyword": {"type": "string", "description": "Optional segment keyword."},
            "status": {"type": "string", "description": "Optional Dify segment status filter."},
        },
        "required": ["dataset_id", "document_id"],
        "additionalProperties": False,
    },
}


QINTOPIA_DIFY_SEGMENT_GET_SCHEMA = {
    "description": "Get one chunk/segment from a Dify Knowledge document.",
    "parameters": {
        "type": "object",
        "properties": {
            "dataset_id": {"type": "string", "description": "Dify dataset id."},
            "document_id": {"type": "string", "description": "Dify document id."},
            "segment_id": {"type": "string", "description": "Dify segment id."},
        },
        "required": ["dataset_id", "document_id", "segment_id"],
        "additionalProperties": False,
    },
}


QINTOPIA_WENYUANGE_LOOKUP_SCHEMA = {
    "description": (
        "Synchronously look up Dify-backed knowledge through WenYuanGe guardrails. "
        "Frontline agents use this instead of raw qintopia_dify_* tools."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "User question or search query."},
            "caller_profile": {
                "type": "string",
                "enum": ["erhua", "xiaoqin"],
                "description": "Calling frontline profile.",
            },
            "audience": {
                "type": "string",
                "enum": ["member_reply", "external_customer"],
                "description": "Audience for the filtered answer basis.",
            },
            "purpose": {
                "type": "string",
                "description": "Why the caller needs this knowledge.",
            },
            "top_k": {
                "type": "integer",
                "minimum": 1,
                "maximum": 5,
                "description": "Maximum chunks to inspect. Defaults to 3.",
            },
        },
        "required": ["query", "caller_profile", "audience", "purpose"],
        "additionalProperties": False,
    },
}


QINTOPIA_MESSAGE_STORE_SEARCH_SCHEMA = {
    "description": (
        "Search the Qintopia QiWe message store in Postgres for recent group memory. "
        "This is a WenYuanGe-only read tool for answering time-bounded community "
        "memory questions with source message metadata."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Natural language query or keywords. Empty query is allowed when time filters are enough."},
            "search_mode": {
                "type": "string",
                "enum": ["hybrid", "semantic", "keyword", "recent"],
                "description": "Retrieval mode. Defaults to hybrid: structured filters plus pgvector semantic recall when configured, with keyword/recent fallback.",
            },
            "chat_id": {"type": "string", "description": "Optional QiWe chat/group id filter."},
            "sender_id": {"type": "string", "description": "Optional sender id filter."},
            "chat_type": {
                "type": "string",
                "enum": ["group", "direct"],
                "description": "Optional chat type filter.",
            },
            "message_kind": {"type": "string", "description": "Optional message kind filter, e.g. text."},
            "since": {"type": "string", "description": "Optional lower timestamp bound, ISO 8601."},
            "until": {"type": "string", "description": "Optional upper timestamp bound, ISO 8601."},
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_MESSAGE_STORE_LIMIT,
                "description": "Maximum messages to return. Defaults to 20.",
            },
            "caller": {"type": "string", "description": "Calling profile id. Must be wenyuange."},
            "purpose": {"type": "string", "description": "Why this message search is needed."},
        },
        "additionalProperties": False,
    },
}


QINTOPIA_DAILY_DIGEST_PUBLISH_SCHEMA = {
    "description": (
        "Publish one Xiaoman-owned daily community event radar digest through "
        "the narrow Agent OS publisher boundary. This tool accepts only a "
        "digest_id and never accepts arbitrary Markdown or generic Feishu URLs."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "digest_id": {
                "type": "string",
                "description": "qintopia_agent_os.daily_digests id to publish.",
            },
            "actor_agent": {
                "type": "string",
                "description": "Actor agent requesting publication. Must be xiaoman.",
            },
            "dry_run": {
                "type": "boolean",
                "description": "Preview the publisher command without applying.",
            },
        },
        "required": ["digest_id"],
        "additionalProperties": False,
    },
}


_XIAOMAN_ACTIVITY_COMMON_PROPS = {
    "actor_agent": {
        "type": "string",
        "description": "Actor agent requesting the operation. Must be xiaoman.",
    },
    "dry_run": {
        "type": "boolean",
        "description": "Preview the worker command. Write operations default to dry-run.",
    },
    "idempotency_key": {
        "type": "string",
        "description": "Optional caller-provided idempotency key for the Agent OS worker.",
    },
}


QINTOPIA_XIAOMAN_ACTIVITY_RECORD_GET_SCHEMA = {
    "description": (
        "Get one approved Xiaoman activity plan/occurrence record through the "
        "Agent OS activity worker boundary. This replaces raw lark-base reads "
        "for Xiaoman turns once the worker is enabled."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "record_id": {"type": "string", "description": "Approved activity Base record id."},
            "table_role": {
                "type": "string",
                "enum": XIAOMAN_ACTIVITY_TABLE_ROLES,
                "description": "Whether the record belongs to the activity plan or occurrence table.",
            },
            **_XIAOMAN_ACTIVITY_COMMON_PROPS,
        },
        "required": ["record_id", "table_role"],
        "additionalProperties": False,
    },
}


QINTOPIA_XIAOMAN_ACTIVITY_LIST_BY_DATE_SCHEMA = {
    "description": (
        "List Xiaoman activity records for one local date through the Agent OS "
        "activity worker boundary. The tool accepts a date, not arbitrary Base queries."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "date": {"type": "string", "description": "Local date in YYYY-MM-DD format."},
            "table_role": {
                "type": "string",
                "enum": XIAOMAN_ACTIVITY_TABLE_ROLES,
                "description": "Table role to list. Defaults to activity_plan when omitted.",
            },
            "timezone": {
                "type": "string",
                "description": "IANA timezone. Defaults to Asia/Shanghai.",
            },
            **_XIAOMAN_ACTIVITY_COMMON_PROPS,
        },
        "required": ["date"],
        "additionalProperties": False,
    },
}


QINTOPIA_XIAOMAN_ACTIVITY_STATUS_UPDATE_SCHEMA = {
    "description": (
        "Update Xiaoman-owned activity status fields through the Agent OS "
        "activity worker boundary. It is not a generic Feishu/Base write tool."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "record_id": {"type": "string", "description": "Approved activity Base record id."},
            "table_role": {"type": "string", "enum": XIAOMAN_ACTIVITY_TABLE_ROLES},
            "status": {"type": "string", "description": "New Xiaoman-owned activity status."},
            "status_note": {"type": "string", "description": "Short note explaining the status change."},
            **_XIAOMAN_ACTIVITY_COMMON_PROPS,
        },
        "required": ["record_id", "table_role", "status"],
        "additionalProperties": False,
    },
}


QINTOPIA_XIAOMAN_ACTIVITY_GAP_UPDATE_SCHEMA = {
    "description": (
        "Update Xiaoman-owned activity gap/supplement fields through the Agent OS "
        "activity worker boundary. It cannot update arbitrary Base fields."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "record_id": {"type": "string", "description": "Approved activity Base record id."},
            "table_role": {"type": "string", "enum": XIAOMAN_ACTIVITY_TABLE_ROLES},
            "gap_summary": {"type": "string", "description": "Short summary of missing information or material gaps."},
            "missing_fields": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Optional missing field names from the approved Xiaoman field set.",
            },
            **_XIAOMAN_ACTIVITY_COMMON_PROPS,
        },
        "required": ["record_id", "table_role", "gap_summary"],
        "additionalProperties": False,
    },
}


QINTOPIA_XIAOMAN_ACTIVITY_HANDOFF_CREATE_SCHEMA = {
    "description": (
        "Create a controlled Xiaoman activity handoff request, such as a visual "
        "asset request for Huabaosi. This is the collaboration wrapper path, "
        "not a raw Kanban or prompt handoff."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "source_record_id": {"type": "string", "description": "Source activity plan/occurrence record id."},
            "handoff_type": {"type": "string", "enum": XIAOMAN_ACTIVITY_HANDOFF_TYPES},
            "target_agent": {"type": "string", "enum": XIAOMAN_ACTIVITY_HANDOFF_TARGETS},
            "brief_summary": {"type": "string", "description": "Safe, concise handoff brief."},
            "purpose": {"type": "string", "description": "Why this handoff is needed."},
            "risk_level": {"type": "string", "enum": ["low", "medium", "high"]},
            "source_event_signal_id": {"type": "string"},
            **_XIAOMAN_ACTIVITY_COMMON_PROPS,
        },
        "required": ["source_record_id", "handoff_type", "target_agent", "brief_summary"],
        "additionalProperties": False,
    },
}


QINTOPIA_XIAOMAN_ACTIVITY_MATERIAL_SUMMARY_SCHEMA = {
    "description": (
        "Request a safe activity material summary through the Agent OS activity "
        "worker boundary. It returns a worker command and does not expose raw "
        "private chat, unrestricted files, or generic Feishu reads."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "record_id": {"type": "string", "description": "Approved activity Base record id."},
            "table_role": {"type": "string", "enum": XIAOMAN_ACTIVITY_TABLE_ROLES},
            "material_notes": {"type": "string", "description": "Optional safe notes about available materials."},
            "source_event_signal_id": {"type": "string"},
            **_XIAOMAN_ACTIVITY_COMMON_PROPS,
        },
        "required": ["record_id", "table_role"],
        "additionalProperties": False,
    },
}


QINTOPIA_COMPLAINT_INTAKE_CREATE_SCHEMA = _operations_intake_plugin().QINTOPIA_COMPLAINT_INTAKE_CREATE_SCHEMA

QINTOPIA_COMPLAINT_INTAKE_UPDATE_SCHEMA = _operations_intake_plugin().QINTOPIA_COMPLAINT_INTAKE_UPDATE_SCHEMA

QINTOPIA_COMPLAINT_FOLLOWUP_SEND_SCHEMA = _operations_intake_plugin().QINTOPIA_COMPLAINT_FOLLOWUP_SEND_SCHEMA

QINTOPIA_EXTERNAL_PRODUCT_KB_SEARCH_SCHEMA = _operations_intake_plugin().QINTOPIA_EXTERNAL_PRODUCT_KB_SEARCH_SCHEMA

QINTOPIA_PUBLIC_CASE_SEARCH_SCHEMA = _operations_intake_plugin().QINTOPIA_PUBLIC_CASE_SEARCH_SCHEMA

QINTOPIA_CUSTOMER_CONTEXT_LOOKUP_SCHEMA = _operations_intake_plugin().QINTOPIA_CUSTOMER_CONTEXT_LOOKUP_SCHEMA

QINTOPIA_LEAD_CAPTURE_SCHEMA = _operations_intake_plugin().QINTOPIA_LEAD_CAPTURE_SCHEMA

QINTOPIA_PROPOSAL_OUTLINE_GENERATE_SCHEMA = _operations_intake_plugin().QINTOPIA_PROPOSAL_OUTLINE_GENERATE_SCHEMA

QINTOPIA_DEMO_SCRIPT_GENERATE_SCHEMA = _operations_intake_plugin().QINTOPIA_DEMO_SCRIPT_GENERATE_SCHEMA

QINTOPIA_EXTERNAL_DISCLOSURE_FILTER_SCHEMA = _operations_intake_plugin().QINTOPIA_EXTERNAL_DISCLOSURE_FILTER_SCHEMA

QINTOPIA_CONVERSATION_SUMMARY_SCHEMA = _operations_intake_plugin().QINTOPIA_CONVERSATION_SUMMARY_SCHEMA

def _json(data: dict[str, Any]) -> str:
    return json.dumps(data, ensure_ascii=False, separators=(",", ":"))


def _clean_text(value: Any, *, max_len: int = 1200) -> str:
    cleaned = re.sub(r"\s+", " ", str(value or "")).strip()
    return cleaned[:max_len]


def _body_text(value: Any, *, max_len: int = 4000) -> str:
    cleaned = str(value or "").replace("\r\n", "\n").replace("\r", "\n").strip()
    return cleaned[:max_len]


def _session_env(name: str) -> str:
    try:
        from gateway.session_context import get_session_env

        return _clean_text(get_session_env(name, ""), max_len=4000)
    except Exception:
        return _clean_text(os.getenv(name, ""), max_len=4000)


def _dify_base_url() -> str:
    return (
        _session_env("QINTOPIA_DIFY_KB_BASE_URL")
        or _session_env("DIFY_KB_BASE_URL")
        or DEFAULT_DIFY_KB_BASE_URL
    ).rstrip("/")


def _dify_api_key() -> str:
    return (
        _session_env("QINTOPIA_DIFY_KB_API_KEY")
        or _session_env("DIFY_KB_API_KEY")
        or _session_env("DIFY_KNOWLEDGE_API_KEY")
    )


def _qintopia_profile_id() -> str:
    return (
        _session_env("QINTOPIA_PROFILE_ID")
        or _session_env("HERMES_PROFILE")
        or _session_env("HERMES_PROFILE_ID")
    ).strip()


def _dify_raw_tools_enabled() -> bool:
    return (
        _qintopia_profile_id() == "wenyuange"
        and _session_env("QINTOPIA_DIFY_RAW_TOOLS_ENABLE") == "1"
    )


def _message_store_tools_enabled() -> bool:
    return (
        _qintopia_profile_id() == "wenyuange"
        and _session_env("QINTOPIA_MESSAGE_STORE_ENABLE") == "1"
    )


def _message_store_database_url() -> str:
    return (
        _session_env("QINTOPIA_MESSAGE_STORE_DATABASE_URL")
        or _session_env("QINTOPIA_SIDECAR_DATABASE_URL")
        or _session_env("DATABASE_URL")
    )


def _daily_digest_publish_enabled() -> bool:
    return _session_env("QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE") == "1"


def _daily_digest_publisher_bin() -> str:
    return (
        _session_env("QINTOPIA_DAILY_DIGEST_PUBLISHER_BIN")
        or _session_env("QINTOPIA_AGENTOS_WORKER_BIN")
        or _session_env("QINTOPIA_SIDECAR_BIN")
        or "/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar"
    )


def _xiaoman_activity_wrappers_enabled() -> bool:
    return (
        _qintopia_profile_id() == "xiaoman"
        and _session_env("QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE") == "1"
    )


def _xiaoman_activity_worker_bin() -> str:
    return (
        _session_env("QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN")
        or _session_env("QINTOPIA_AGENTOS_WORKER_BIN")
        or _session_env("QINTOPIA_SIDECAR_BIN")
        or "/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar"
    )


def _xiaoman_activity_fixture_path() -> str:
    return _session_env("QINTOPIA_XIAOMAN_ACTIVITY_FIXTURE_PATH")


def _xiaoman_activity_use_feishu_base() -> bool:
    return _session_env("QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE") == "1"


def _qintopia_weather_location() -> str:
    return _session_env("QINTOPIA_WEATHER_LOCATION") or DEFAULT_QINTOPIA_WEATHER_LOCATION


def _qintopia_weather_location_name() -> str:
    return _session_env("QINTOPIA_WEATHER_LOCATION_NAME") or DEFAULT_QINTOPIA_WEATHER_LOCATION_NAME


def _qintopia_weather_qweather_city() -> str:
    return _session_env("QINTOPIA_WEATHER_QWEATHER_CITY") or DEFAULT_QINTOPIA_WEATHER_QWEATHER_CITY


def _qintopia_weather_mcp_timeout() -> float:
    try:
        raw = float(_session_env("QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS") or DEFAULT_QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS)
    except (TypeError, ValueError):
        raw = DEFAULT_QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS
    return min(max(raw, 2.0), 30.0)


def _qintopia_weather_horizon(args: dict[str, Any]) -> int:
    try:
        raw = int(args.get("hours") or 24)
    except (TypeError, ValueError):
        raw = 24
    return min(max(raw, 1), 24)


def _qintopia_weather_now_iso() -> str:
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


def _qweather_forbidden_tool_names(tool_names: list[str]) -> list[str]:
    forbidden = []
    for name in tool_names:
        lowered = name.lower()
        if any(pattern in lowered or pattern in name for pattern in QWEATHER_FORBIDDEN_TOOL_PATTERNS):
            forbidden.append(name)
    return sorted(set(forbidden))


def _qweather_mcp_call(tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    if tool_name not in QWEATHER_ALLOWED_MCP_TOOLS:
        return {
            "success": False,
            "error": "QWeather MCP tool is not allowlisted for Erhua",
            "tool": tool_name,
            "allowlist": sorted(QWEATHER_ALLOWED_MCP_TOOLS),
        }
    previous_disable_level = logging.root.manager.disable
    try:
        with QWEATHER_IMPORT_LOCK:
            logging.disable(logging.INFO)
            logging.getLogger("hefeng_qweather_mcp").setLevel(logging.ERROR)
            logging.getLogger("httpx").setLevel(logging.WARNING)
            sys.modules.pop("hefeng_qweather_mcp.main", None)
            module = importlib.import_module("hefeng_qweather_mcp.main")
            handler = getattr(module, tool_name)
            executor = ThreadPoolExecutor(max_workers=1)
            future = executor.submit(handler, **arguments)
            try:
                payload = future.result(timeout=_qintopia_weather_mcp_timeout())
            except TimeoutError:
                future.cancel()
                executor.shutdown(wait=False, cancel_futures=True)
                return {
                    "success": False,
                    "error": "QWeather MCP package call timed out",
                    "tool": tool_name,
                    "timeout_seconds": _qintopia_weather_mcp_timeout(),
                }
            finally:
                if future.done():
                    executor.shutdown(wait=False, cancel_futures=True)
        if payload is None:
            return {
                "success": False,
                "error": "QWeather MCP tool returned no data",
                "tool": tool_name,
            }
        return {"success": True, "tool": tool_name, "data": payload}
    except ImportError:
        return {
            "success": False,
            "error": "hefeng-qweather-mcp is not installed in the Hermes Python environment",
            "tool": tool_name,
        }
    except Exception as exc:
        return {
            "success": False,
            "error": "QWeather MCP package call failed",
            "tool": tool_name,
            "detail": _clean_text(exc, max_len=300),
        }
    finally:
        logging.disable(previous_disable_level)


def _qweather_call_bundle(location: str) -> dict[str, dict[str, Any]]:
    calls = {
        "current": ("get_weather_now", {"location": location, "lang": "zh", "unit": "m"}),
        "hourly": ("get_hourly_weather", {"location": location, "hours": "24h", "lang": "zh", "unit": "m"}),
        "minutely": ("get_minutely_5m", {"location": location, "lang": "zh"}),
        "warnings": ("get_warning", {"city": _qintopia_weather_qweather_city()}),
        "air_quality": ("get_air_quality", {"city": _qintopia_weather_qweather_city()}),
    }
    with ThreadPoolExecutor(max_workers=len(calls)) as executor:
        futures = {
            name: executor.submit(_qweather_mcp_call, tool_name, arguments)
            for name, (tool_name, arguments) in calls.items()
        }
        return {name: future.result() for name, future in futures.items()}


def _qweather_data(call: dict[str, Any], key: str) -> Any:
    if not call.get("success") or not isinstance(call.get("data"), dict):
        return None
    return call["data"].get(key)


def _qweather_rainy_hour(hour: dict[str, Any]) -> bool:
    text = _clean_text(hour.get("text"), max_len=80)
    try:
        pop = int(float(hour.get("pop") or 0))
    except (TypeError, ValueError):
        pop = 0
    try:
        precip = float(hour.get("precip") or 0)
    except (TypeError, ValueError):
        precip = 0.0
    return bool(re.search(r"雨|雷|阵雨|降水", text)) or pop >= 40 or precip >= 0.1


def _qweather_thunder_hour(hour: dict[str, Any]) -> bool:
    text = _clean_text(hour.get("text"), max_len=80)
    return bool(re.search(r"雷|雷阵雨|雷暴", text))


def _qweather_precip_minute(item: dict[str, Any]) -> bool:
    try:
        return float(item.get("precip") or 0) > 0
    except (TypeError, ValueError):
        return False


def _time_windows(items: list[dict[str, Any]], predicate, time_key: str, *, max_windows: int = 6) -> list[dict[str, str]]:
    windows = []
    start = ""
    end = ""
    for item in items:
        when = _clean_text(item.get(time_key), max_len=40)
        if not when:
            continue
        if predicate(item):
            if not start:
                start = when
            end = when
        elif start:
            windows.append({"start": start, "end": end})
            start = ""
            end = ""
    if start:
        windows.append({"start": start, "end": end})
    return windows[:max_windows]


def _qweather_current(now: Any) -> dict[str, Any] | None:
    if not isinstance(now, dict):
        return None
    return {
        "obs_time": _clean_text(now.get("obsTime"), max_len=40),
        "text": _clean_text(now.get("text"), max_len=80),
        "temp_c": _clean_text(now.get("temp"), max_len=20),
        "feels_like_c": _clean_text(now.get("feelsLike"), max_len=20),
        "humidity_pct": _clean_text(now.get("humidity"), max_len=20),
        "wind_dir": _clean_text(now.get("windDir"), max_len=40),
        "wind_scale": _clean_text(now.get("windScale"), max_len=20),
        "wind_speed_kmh": _clean_text(now.get("windSpeed"), max_len=20),
        "precip_mm": _clean_text(now.get("precip"), max_len=20),
    }


def _qweather_warnings(items: Any) -> list[dict[str, str]]:
    if not isinstance(items, list):
        return []
    warnings = []
    for item in items:
        if not isinstance(item, dict):
            continue
        warnings.append(
            {
                "title": _clean_text(item.get("title"), max_len=160),
                "type": _clean_text(item.get("typeName"), max_len=80),
                "level": _clean_text(item.get("level"), max_len=40),
                "status": _clean_text(item.get("status"), max_len=40),
                "start_time": _clean_text(item.get("startTime"), max_len=40),
            }
        )
    return warnings[:5]


def _qweather_air_quality(data: Any) -> dict[str, Any] | None:
    if not isinstance(data, dict):
        return None
    now = data.get("now")
    if isinstance(now, dict):
        return {
            "pub_time": _clean_text(now.get("pubTime"), max_len=40),
            "aqi": _clean_text(now.get("aqi"), max_len=20),
            "category": _clean_text(now.get("category"), max_len=80),
            "primary": _clean_text(now.get("primary"), max_len=80),
        }

    indexes = data.get("indexes")
    if not isinstance(indexes, list) or not indexes:
        return None
    primary_index = next((item for item in indexes if item.get("code") == "cn-mee"), indexes[0])
    if not isinstance(primary_index, dict):
        return None
    pollutant = primary_index.get("primaryPollutant")
    if not isinstance(pollutant, dict):
        pollutant = {}
    health = primary_index.get("health")
    advice = health.get("advice") if isinstance(health, dict) else {}
    if not isinstance(advice, dict):
        advice = {}
    return {
        "pub_time": "",
        "aqi": _clean_text(primary_index.get("aqiDisplay") or primary_index.get("aqi"), max_len=20),
        "category": _clean_text(primary_index.get("category"), max_len=80),
        "primary": _clean_text(pollutant.get("name") or pollutant.get("code"), max_len=80),
        "health_advice": _clean_text(advice.get("generalPopulation"), max_len=200),
        "source_city": _qintopia_weather_qweather_city(),
    }


def _qweather_successful(bundle: dict[str, dict[str, Any]]) -> bool:
    return any(call.get("success") for call in bundle.values())


def _qweather_payload(args: dict[str, Any], bundle: dict[str, dict[str, Any]]) -> dict[str, Any]:
    hourly = _qweather_data(bundle.get("hourly", {}), "hourly")
    minutely = _qweather_data(bundle.get("minutely", {}), "minutely")
    if not isinstance(hourly, list):
        hourly = []
    if not isinstance(minutely, list):
        minutely = []

    umbrella_windows = _time_windows(minutely, _qweather_precip_minute, "fxTime", max_windows=8)
    if not umbrella_windows:
        umbrella_windows = _time_windows(hourly[: _qintopia_weather_horizon(args)], _qweather_rainy_hour, "fxTime")
    thunderstorm_windows = _time_windows(hourly[: _qintopia_weather_horizon(args)], _qweather_thunder_hour, "fxTime")

    errors = {
        name: {key: value for key, value in call.items() if key in {"error", "detail", "status", "exit_code", "timeout_seconds"}}
        for name, call in bundle.items()
        if not call.get("success")
    }
    limitations = []
    if "warnings" in errors:
        limitations.append("QWeather warning data unavailable; do not claim no official warning")
    if "air_quality" in errors:
        limitations.append("QWeather air-quality data unavailable")

    payload = {
        "success": True,
        "skill": QINTOPIA_WEATHER_TOOL,
        "source": "qweather_mcp",
        "provider": "QWeather",
        "generated_at": _qintopia_weather_now_iso(),
        "location": {
            "name": _qintopia_weather_location_name(),
            "coordinates": _qintopia_weather_location(),
            "fixed": True,
        },
        "current": _qweather_current(_qweather_data(bundle.get("current", {}), "now")),
        "umbrella_windows": umbrella_windows,
        "thunderstorm_windows": thunderstorm_windows,
        "warnings": _qweather_warnings(_qweather_data(bundle.get("warnings", {}), "warning")),
        "air_quality": _qweather_air_quality(bundle.get("air_quality", {}).get("data")),
        "limitations": limitations,
        "guardrails": {
            "allowed_mcp_tools": sorted(QWEATHER_ALLOWED_MCP_TOOLS),
            "excluded_capabilities": ["tropical_cyclone_typhoon", "ocean_marine", "solar_radiation"],
            "fixed_location_only": True,
        },
    }
    if errors:
        payload["partial_errors"] = errors
    return payload


def _open_meteo_fallback() -> dict[str, Any]:
    lon, lat = [part.strip() for part in _qintopia_weather_location().split(",", 1)]
    params = urlencode(
        {
            "latitude": lat,
            "longitude": lon,
            "current": "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m",
            "hourly": "weather_code,precipitation_probability,precipitation",
            "timezone": "Asia/Shanghai",
            "forecast_days": "1",
        }
    )
    url = f"https://api.open-meteo.com/v1/forecast?{params}"
    request = urlrequest.Request(url, headers={"User-Agent": "qintopia-weather-fallback/1.0"})
    try:
        with urlrequest.urlopen(request, timeout=DEFAULT_OPEN_METEO_TIMEOUT_SECONDS) as response:
            data = json.loads(response.read(1_000_000).decode("utf-8"))
    except Exception as exc:
        return {
            "success": False,
            "skill": QINTOPIA_WEATHER_TOOL,
            "source": "weather_unavailable",
            "generated_at": _qintopia_weather_now_iso(),
            "error": "QWeather MCP failed and Open-Meteo fallback failed",
            "detail": _clean_text(exc, max_len=300),
            "limitations": ["cannot confirm hourly weather now"],
        }

    hourly = data.get("hourly") if isinstance(data.get("hourly"), dict) else {}
    times = hourly.get("time") if isinstance(hourly.get("time"), list) else []
    probs = hourly.get("precipitation_probability") if isinstance(hourly.get("precipitation_probability"), list) else []
    precip = hourly.get("precipitation") if isinstance(hourly.get("precipitation"), list) else []
    rows = []
    for idx, when in enumerate(times[:24]):
        rows.append(
            {
                "time": str(when),
                "precipitation_probability": probs[idx] if idx < len(probs) else 0,
                "precipitation": precip[idx] if idx < len(precip) else 0,
            }
        )

    def rainy(row: dict[str, Any]) -> bool:
        try:
            probability = int(float(row.get("precipitation_probability") or 0))
            amount = float(row.get("precipitation") or 0)
        except (TypeError, ValueError):
            return False
        return probability >= 40 or amount >= 0.1

    current = data.get("current") if isinstance(data.get("current"), dict) else {}
    return {
        "success": True,
        "skill": QINTOPIA_WEATHER_TOOL,
        "source": "open_meteo_fallback",
        "provider": "Open-Meteo",
        "generated_at": _qintopia_weather_now_iso(),
        "location": {
            "name": _qintopia_weather_location_name(),
            "coordinates": _qintopia_weather_location(),
            "fixed": True,
        },
        "current": {
            "time": _clean_text(current.get("time"), max_len=40),
            "temp_c": current.get("temperature_2m"),
            "feels_like_c": current.get("apparent_temperature"),
            "humidity_pct": current.get("relative_humidity_2m"),
            "wind_speed_kmh": current.get("wind_speed_10m"),
        },
        "umbrella_windows": _time_windows(rows, rainy, "time"),
        "thunderstorm_windows": [],
        "warnings": [],
        "air_quality": None,
        "limitations": [
            "Open-Meteo fallback only; no QWeather official warnings",
            "no minute-level precipitation conclusion",
            "no air-quality result",
            "no typhoon, ocean, or solar-radiation data",
        ],
        "guardrails": {
            "excluded_capabilities": ["tropical_cyclone_typhoon", "ocean_marine", "solar_radiation"],
            "fixed_location_only": True,
        },
    }


def _message_store_embedding_url() -> str:
    return (
        _session_env("QINTOPIA_MESSAGE_STORE_EMBEDDING_URL")
        or _session_env("QINTOPIA_EMBEDDING_URL")
        or _session_env("OPENAI_EMBEDDING_BASE_URL")
    ).rstrip("/")


def _message_store_embedding_api_key() -> str:
    return (
        _session_env("QINTOPIA_MESSAGE_STORE_EMBEDDING_API_KEY")
        or _session_env("QINTOPIA_EMBEDDING_API_KEY")
        or _session_env("OPENAI_API_KEY")
    )


def _message_store_embedding_model() -> str:
    return (
        _session_env("QINTOPIA_MESSAGE_STORE_EMBEDDING_MODEL")
        or _session_env("QINTOPIA_EMBEDDING_MODEL")
        or "text-embedding-3-small"
    )


def _message_store_embedding_db_model() -> str:
    return _session_env("QINTOPIA_MESSAGE_STORE_EMBEDDING_DB_MODEL") or _message_store_embedding_model()


def _dify_allowed_dataset_ids() -> set[str]:
    raw = _session_env("QINTOPIA_DIFY_ALLOWED_DATASET_IDS") or _session_env("DIFY_ALLOWED_DATASET_IDS")
    return {item.strip() for item in raw.split(",") if item.strip()}


def _dify_lookup_dataset_id() -> str:
    explicit = _session_env("QINTOPIA_DIFY_LOOKUP_DATASET_ID")
    if explicit:
        return explicit
    allowed = sorted(_dify_allowed_dataset_ids())
    return allowed[0] if len(allowed) == 1 else ""


def _dify_limit(args: dict[str, Any], *, default: int = DEFAULT_DIFY_LIMIT, maximum: int = MAX_DIFY_LIMIT) -> int:
    try:
        raw = int(args.get("limit") or default)
    except (TypeError, ValueError):
        raw = default
    return min(max(raw, 1), maximum)


def _dify_page(args: dict[str, Any]) -> int:
    try:
        raw = int(args.get("page") or 1)
    except (TypeError, ValueError):
        raw = 1
    return max(raw, 1)


def _dify_dataset_id(args: dict[str, Any]) -> str:
    return _clean_text(args.get("dataset_id"), max_len=120)


def _dify_document_id(args: dict[str, Any]) -> str:
    return _clean_text(args.get("document_id"), max_len=120)


def _dify_segment_id(args: dict[str, Any]) -> str:
    return _clean_text(args.get("segment_id"), max_len=120)


def _dify_dataset_allowed(dataset_id: str) -> bool:
    allowed = _dify_allowed_dataset_ids()
    return not allowed or dataset_id in allowed


def _dify_readiness_error() -> dict[str, Any] | None:
    if not _dify_base_url():
        return {"success": False, "error": "Dify Knowledge base URL is not configured"}
    if not _dify_api_key():
        return {
            "success": False,
            "error": "Dify Knowledge Service API key is not configured",
            "required_env": [
                "QINTOPIA_DIFY_KB_API_KEY",
                "DIFY_KB_API_KEY",
                "DIFY_KNOWLEDGE_API_KEY",
            ],
        }
    return None


def _dify_dataset_denied(dataset_id: str) -> dict[str, Any] | None:
    if not dataset_id:
        return {"success": False, "error": "dataset_id is required"}
    if not _dify_dataset_allowed(dataset_id):
        return {
            "success": False,
            "error": "dataset_id is not in the configured allowlist",
            "dataset_id": dataset_id,
            "allowlist_env": "QINTOPIA_DIFY_ALLOWED_DATASET_IDS",
        }
    return None


def _dify_endpoint(path: str, params: dict[str, Any] | None = None) -> str:
    query_params = {
        key: value
        for key, value in (params or {}).items()
        if value not in ("", None, [], {})
    }
    url = f"{_dify_base_url()}/{path.lstrip('/')}"
    if query_params:
        url = f"{url}?{urlencode(query_params)}"
    return url


def _dify_parse_json(raw: bytes) -> Any:
    if not raw:
        return None
    text = raw.decode("utf-8", errors="replace")
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return {"raw": text[:4000]}


def _dify_request(
    method: str,
    path: str,
    *,
    params: dict[str, Any] | None = None,
    body: dict[str, Any] | None = None,
) -> dict[str, Any]:
    readiness = _dify_readiness_error()
    if readiness:
        return readiness

    data = None
    headers = {
        "Authorization": f"Bearer {_dify_api_key()}",
        "Content-Type": "application/json",
        "Accept": "application/json",
    }
    if body is not None:
        data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    request = urlrequest.Request(
        _dify_endpoint(path, params),
        data=data,
        headers=headers,
        method=method.upper(),
    )
    try:
        with urlrequest.urlopen(request, timeout=DEFAULT_DIFY_TIMEOUT_SECONDS) as response:
            payload = _dify_parse_json(response.read(2_000_000))
            return {
                "success": True,
                "status": response.status,
                "data": payload,
            }
    except urlerror.HTTPError as exc:
        return {
            "success": False,
            "status": exc.code,
            "error": "Dify Knowledge API returned an HTTP error",
            "data": _dify_parse_json(exc.read(200_000)),
        }
    except urlerror.URLError as exc:
        return {
            "success": False,
            "error": "Dify Knowledge API connection failed",
            "detail": _clean_text(getattr(exc, "reason", exc), max_len=500),
        }
    except TimeoutError:
        return {"success": False, "error": "Dify Knowledge API request timed out"}
    except Exception as exc:
        return {
            "success": False,
            "error": "Dify Knowledge API request failed",
            "detail": _clean_text(exc, max_len=500),
        }


def _dify_tool_payload(skill: str, operation: str, response: dict[str, Any], **meta: Any) -> str:
    payload: dict[str, Any] = {
        "success": bool(response.get("success")),
        "skill": skill,
        "operation": operation,
        "source": "dify_knowledge_api",
        "base_url": _dify_base_url(),
        "read_only": True,
        **meta,
    }
    if "status" in response:
        payload["status"] = response["status"]
    if response.get("success"):
        payload["data"] = response.get("data")
    else:
        payload["error"] = response.get("error") or "Dify Knowledge API request failed"
        if "detail" in response:
            payload["detail"] = response["detail"]
        if "data" in response:
            payload["data"] = response["data"]
    return _json(payload)


def _filter_dify_dataset_list(data: Any) -> Any:
    allowed = _dify_allowed_dataset_ids()
    if not allowed or not isinstance(data, dict) or not isinstance(data.get("data"), list):
        return data
    filtered = [
        item
        for item in data["data"]
        if isinstance(item, dict) and str(item.get("id") or "") in allowed
    ]
    copied = dict(data)
    copied["data"] = filtered
    copied["filtered_by_allowlist"] = True
    copied["allowlist_count"] = len(allowed)
    return copied


XIAOQIN_LOOKUP_RISK_PATTERNS = {
    "member_scoped": ["成员资料", "村民档案", "成员档案", "入住", "房型"],
    "personal_data": ["手机号", "身份证", "生日", "联系方式", "微信号"],
    "internal_information": ["内部", "未公开", "服务器", "日志", "prompt", "提示词"],
    "commercial_commitment": ["报价", "合同", "sla", "交付承诺", "排期"],
    "credentials": ["token", "secret", "password", "密钥", "凭证"],
}


ERHUA_LOOKUP_RISK_PATTERNS = {
    "member_privacy": ["手机号", "身份证", "生日", "房间", "入住时间", "成员档案", "隐藏画像", "私密历史", "村民档案"],
    "complaint_or_service_recovery": ["投诉", "服务不满", "不满意", "入住体验不好", "客服介入", "反馈处理"],
}


def _lookup_risk_flags(caller_profile: str, audience: str, text: str) -> list[str]:
    patterns = XIAOQIN_LOOKUP_RISK_PATTERNS if (caller_profile, audience) == ("xiaoqin", "external_customer") else ERHUA_LOOKUP_RISK_PATTERNS
    lowered = text.lower()
    flags = []
    for flag, needles in patterns.items():
        if any(needle.lower() in lowered for needle in needles):
            flags.append(flag)
    return flags


def _dify_record_segment(record: Any) -> dict[str, Any]:
    if not isinstance(record, dict):
        return {}
    segment = record.get("segment")
    return segment if isinstance(segment, dict) else {}


def _dify_record_content(record: Any) -> str:
    return _body_text(_dify_record_segment(record).get("content"), max_len=1200)


def _dify_record_document_name(record: Any) -> str:
    document = _dify_record_segment(record).get("document")
    if isinstance(document, dict):
        return _clean_text(document.get("name"), max_len=240)
    return ""


def _dify_lookup_sources(records: list[Any]) -> list[dict[str, Any]]:
    sources: list[dict[str, Any]] = []
    dataset_id = _dify_lookup_dataset_id()
    for record in records:
        segment = _dify_record_segment(record)
        if not segment:
            continue
        sources.append(
            {
                "dataset_id": dataset_id,
                "document_name": _dify_record_document_name(record),
                "document_id": segment.get("document_id"),
                "segment_id": segment.get("id"),
                "score": record.get("score") if isinstance(record, dict) else None,
            }
        )
    return sources


def _dify_lookup_answer_basis(records: list[Any]) -> str:
    snippets = []
    for record in records:
        content = _dify_record_content(record)
        if not content:
            continue
        snippets.append(content[:360])
    return "\n\n".join(snippets)[:1000]


def _record_source_quality(record: Any) -> int:
    document_name = _dify_record_document_name(record).lower()
    content = _dify_record_content(record).lower()
    score = 0
    if "/wiki/" in document_name or " wiki / " in document_name:
        score += 20
    if "public" in content or "member-facing" in content:
        score += 30
    if any(term in document_name or term in content for term in ["公共设施", "设施", "须知", "规则", "faq"]):
        score += 15
    if "difyradio" in document_name or "日更" in document_name:
        score -= 35
    if "risk: internal" in content or "status: stub" in content or "待补全" in content:
        score -= 40
    if "/profiles/" in document_name or "/ profiles /" in document_name or "profiles /" in document_name:
        score -= 100
    return score


def _record_query_score(record: Any, query: str) -> tuple[int, float]:
    keywords = _query_keywords(query)
    haystack = "\n".join([_dify_record_document_name(record), _dify_record_content(record)]).lower()
    lexical_score = 0
    for keyword in keywords:
        lowered = keyword.lower()
        if lowered and lowered in haystack:
            lexical_score += max(len(lowered), 1)
    try:
        retrieval_score = float(record.get("score") or 0) if isinstance(record, dict) else 0.0
    except (TypeError, ValueError):
        retrieval_score = 0.0
    return lexical_score, _record_source_quality(record), retrieval_score


def _query_keywords(query: str) -> list[str]:
    normalized = query.replace("Wi-Fi", "wifi").replace("wi-fi", "wifi")
    tokens = []
    for token in _tokenize(normalized):
        cleaned = token.strip()
        if len(cleaned) >= 2 and cleaned not in {"多少", "是什么", "怎么", "如何", "一下", "这个", "那个"}:
            tokens.append(cleaned)
    compact = re.sub(r"\s+", "", normalized)
    if compact and compact not in tokens:
        tokens.append(compact)
    seen = set()
    keywords = []
    for token in tokens:
        lowered = token.lower()
        if lowered not in seen:
            seen.add(lowered)
            keywords.append(token)
    return sorted(keywords[:8], key=lambda item: (-len(item), item.lower()))


def _dify_document_item_id(item: Any) -> str:
    if not isinstance(item, dict):
        return ""
    return _clean_text(item.get("id") or item.get("document_id"), max_len=120)


def _dify_document_item_name(item: Any) -> str:
    if not isinstance(item, dict):
        return ""
    return _clean_text(item.get("name") or item.get("title"), max_len=240)


def _dify_segment_record(dataset_id: str, document: dict[str, Any], segment: dict[str, Any]) -> dict[str, Any]:
    segment_id = segment.get("id") or segment.get("segment_id")
    content = segment.get("content") or segment.get("text") or ""
    document_id = _dify_document_item_id(document)
    document_name = _dify_document_item_name(document)
    return {
        "score": segment.get("score"),
        "segment": {
            "id": segment_id,
            "document_id": document_id,
            "content": content,
            "document": {
                "id": document_id,
                "name": document_name,
                "dataset_id": dataset_id,
            },
        },
    }


def _dify_document_keyword_records(dataset_id: str, query: str, top_k: int, trace: list[dict[str, Any]]) -> list[Any]:
    records: list[Any] = []
    seen_segments: set[tuple[str, str]] = set()
    max_records = max(top_k * 6, top_k)
    for keyword in _query_keywords(query):
        documents_payload = json.loads(
            handle_qintopia_dify_document_list(
                {
                    "dataset_id": dataset_id,
                    "keyword": keyword,
                    "page": 1,
                    "limit": min(max(top_k, 1), 10),
                }
            )
        )
        documents_data = documents_payload.get("data") if isinstance(documents_payload.get("data"), dict) else {}
        documents = documents_data.get("data") if isinstance(documents_data, dict) else []
        if not isinstance(documents, list):
            documents = []
        trace.append(
            {
                "search_method": "document_keyword",
                "query": keyword,
                "success": bool(documents_payload.get("success")),
                "result_count": len(documents),
                "status": documents_payload.get("status"),
                "error": documents_payload.get("error"),
            }
        )
        if not documents_payload.get("success"):
            continue
        for document in documents[:top_k]:
            document_id = _dify_document_item_id(document)
            if not document_id:
                continue
            segments_payload = json.loads(
                handle_qintopia_dify_segment_list(
                    {
                        "dataset_id": dataset_id,
                        "document_id": document_id,
                        "keyword": keyword,
                        "page": 1,
                        "limit": min(max(top_k, 1), 10),
                    }
                )
            )
            segments_data = segments_payload.get("data") if isinstance(segments_payload.get("data"), dict) else {}
            segments = segments_data.get("data") if isinstance(segments_data, dict) else []
            if not isinstance(segments, list):
                segments = []
            trace.append(
                {
                    "search_method": "segment_keyword",
                    "query": keyword,
                    "document_id": document_id,
                    "success": bool(segments_payload.get("success")),
                    "result_count": len(segments),
                    "status": segments_payload.get("status"),
                    "error": segments_payload.get("error"),
                }
            )
            for segment in segments:
                if not isinstance(segment, dict):
                    continue
                segment_id = _clean_text(segment.get("id") or segment.get("segment_id"), max_len=120)
                key = (document_id, segment_id)
                if key in seen_segments:
                    continue
                seen_segments.add(key)
                records.append(_dify_segment_record(dataset_id, document, segment))
                if len(records) >= max_records:
                    return records
    return records


def _dify_lookup_records(dataset_id: str, query: str, top_k: int) -> tuple[list[Any], list[dict[str, Any]], dict[str, Any] | None]:
    attempts = [
        {"search_method": "semantic_search", "query": query},
        {"search_method": "hybrid_search", "query": query},
        {"search_method": "full_text_search", "query": query},
    ]

    seen_attempts: set[tuple[str, str]] = set()
    trace: list[dict[str, Any]] = []
    first_error: dict[str, Any] | None = None
    saw_success = False
    collected_records: list[Any] = []
    seen_segments: set[tuple[str, str]] = set()
    for attempt in attempts:
        key = (attempt["search_method"], attempt["query"])
        if key in seen_attempts:
            continue
        seen_attempts.add(key)
        retrieve_payload = json.loads(
            handle_qintopia_dify_knowledge_retrieve(
                {
                    "dataset_id": dataset_id,
                    "query": attempt["query"],
                    "top_k": top_k,
                    "search_method": attempt["search_method"],
                    "score_threshold_enabled": False,
                    "reranking_enable": False,
                }
            )
        )
        if not retrieve_payload.get("success"):
            trace.append(
                {
                    "search_method": attempt["search_method"],
                    "query": attempt["query"],
                    "success": False,
                    "status": retrieve_payload.get("status"),
                    "error": retrieve_payload.get("error"),
                }
            )
            if first_error is None:
                first_error = retrieve_payload
            continue
        data = retrieve_payload.get("data") if isinstance(retrieve_payload.get("data"), dict) else {}
        raw_records = data.get("records") if isinstance(data, dict) else []
        records = raw_records[:top_k] if isinstance(raw_records, list) else []
        saw_success = True
        trace.append(
            {
                "search_method": attempt["search_method"],
                "query": attempt["query"],
                "success": True,
                "result_count": len(records),
            }
        )
        for record in records:
            segment = _dify_record_segment(record)
            key = (
                _clean_text(segment.get("document_id"), max_len=120),
                _clean_text(segment.get("id"), max_len=120),
            )
            if key in seen_segments:
                continue
            seen_segments.add(key)
            collected_records.append(record)
    for record in _dify_document_keyword_records(dataset_id, query, top_k, trace):
        segment = _dify_record_segment(record)
        key = (
            _clean_text(segment.get("document_id"), max_len=120),
            _clean_text(segment.get("id"), max_len=120),
        )
        if key in seen_segments:
            continue
        seen_segments.add(key)
        collected_records.append(record)
    collected_records.sort(key=lambda record: _record_query_score(record, query), reverse=True)
    return collected_records, trace, None if saw_success or collected_records else first_error


def _source_risk_flags(caller_profile: str, audience: str, record: Any) -> list[str]:
    document_name = _dify_record_document_name(record).lower()
    content = _dify_record_content(record)
    flags = []
    if (caller_profile, audience) == ("erhua", "member_reply"):
        if "difyradio" in document_name or "日更" in document_name:
            flags.append("low_authority_digest")
        member_source_markers = [
            "/profiles/",
            "/ profiles /",
            "profiles /",
            "成员档案",
            "村民档案",
        ]
        if any(marker in document_name for marker in member_source_markers):
            flags.append("member_privacy")
        if "成员档案" in content or "村民档案" in content:
            flags.append("member_privacy")
        lowered_content = content.lower()
        if "risk: internal" in lowered_content or "status: stub" in lowered_content or "待补全" in content:
            flags.append("internal_information")
    if (caller_profile, audience) == ("xiaoqin", "external_customer"):
        if any(marker in document_name for marker in ["/profiles/", "/ profiles /", "profiles /"]):
            flags.append("member_scoped")
    return flags


def _filter_lookup_records(
    caller_profile: str,
    audience: str,
    query: str,
    purpose: str,
    records: list[Any],
) -> tuple[list[Any], list[str]]:
    safe_records = []
    blocked_flags: list[str] = []
    for record in records:
        text = " ".join([query, purpose, _dify_record_document_name(record), _dify_record_content(record)])
        flags = _lookup_risk_flags(caller_profile, audience, text)
        for flag in _source_risk_flags(caller_profile, audience, record):
            if flag not in flags:
                flags.append(flag)
        if flags:
            for flag in flags:
                if flag not in blocked_flags:
                    blocked_flags.append(flag)
            continue
        safe_records.append(record)
    return safe_records, blocked_flags


def _message_store_limit(args: dict[str, Any]) -> int:
    try:
        raw = int(args.get("limit") or DEFAULT_MESSAGE_STORE_LIMIT)
    except (TypeError, ValueError):
        raw = DEFAULT_MESSAGE_STORE_LIMIT
    return min(max(raw, 1), MAX_MESSAGE_STORE_LIMIT)


def _message_store_search_mode(args: dict[str, Any]) -> str:
    mode = _clean_text(args.get("search_mode"), max_len=40).lower()
    return mode if mode in {"hybrid", "semantic", "keyword", "recent"} else "hybrid"


def _message_store_semantic_candidate_limit(limit: int) -> int:
    return min(
        max(limit * 3, DEFAULT_MESSAGE_STORE_SEMANTIC_CANDIDATE_LIMIT),
        MAX_MESSAGE_STORE_SEMANTIC_CANDIDATE_LIMIT,
    )


def _message_store_query_terms(query: str) -> list[str]:
    terms = list(_query_keywords(query))
    compact = re.sub(r"\s+", "", query)
    chinese_runs = re.findall(r"[\u3400-\u9fff]{2,}", compact)
    for run in chinese_runs:
        if run not in terms:
            terms.append(run)
        for size in (2, 3, 4):
            if len(run) <= size:
                continue
            for start in range(0, len(run) - size + 1):
                fragment = run[start : start + size]
                if fragment not in terms:
                    terms.append(fragment)
    seen: set[str] = set()
    unique_terms: list[str] = []
    stop_terms = {"大家", "今天", "昨天", "明天", "什么", "怎么", "如何", "多少", "一下", "这个", "那个"}
    for term in terms:
        cleaned = _clean_text(term, max_len=80)
        lowered = cleaned.lower()
        if not cleaned or lowered in seen or cleaned in stop_terms:
            continue
        seen.add(lowered)
        unique_terms.append(cleaned)
    return unique_terms[:24]


def _message_store_embedding_endpoint(url: str) -> str:
    if not url:
        return ""
    if url.endswith("/embeddings") or "/embeddings?" in url:
        return url
    if url.endswith("/v1"):
        return f"{url}/embeddings"
    return url


def _parse_embedding_payload(payload: Any) -> list[float]:
    candidate: Any = None
    if isinstance(payload, dict):
        if isinstance(payload.get("embedding"), list):
            candidate = payload.get("embedding")
        elif isinstance(payload.get("data"), list) and payload["data"]:
            first = payload["data"][0]
            if isinstance(first, dict):
                candidate = first.get("embedding")
        elif isinstance(payload.get("result"), dict):
            return _parse_embedding_payload(payload["result"])
    elif isinstance(payload, list):
        candidate = payload
    if not isinstance(candidate, list):
        return []
    values: list[float] = []
    for item in candidate:
        try:
            values.append(float(item))
        except (TypeError, ValueError):
            return []
    return values


def _embedding_to_pgvector(value: list[float]) -> str:
    return "[" + ",".join(f"{item:.9g}" for item in value) + "]"


def _message_store_query_embedding(query: str) -> tuple[list[float], dict[str, Any]]:
    url = _message_store_embedding_url()
    model = _message_store_embedding_model()
    trace = {
        "search_method": "query_embedding",
        "configured": bool(url),
        "model": model,
        "success": False,
    }
    if not url:
        trace["error"] = "embedding endpoint is not configured"
        trace["required_env"] = ["QINTOPIA_MESSAGE_STORE_EMBEDDING_URL"]
        return [], trace

    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json",
    }
    credential = _message_store_embedding_api_key()
    if credential:
        headers["Authorization"] = f"Bearer {credential}"
    body = {
        "model": model,
        "input": query,
    }
    request = urlrequest.Request(
        _message_store_embedding_endpoint(url),
        data=json.dumps(body, ensure_ascii=False).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    try:
        with urlrequest.urlopen(
            request,
            timeout=DEFAULT_MESSAGE_STORE_EMBEDDING_TIMEOUT_SECONDS,
        ) as response:
            payload = _dify_parse_json(response.read(4_000_000))
            embedding = _parse_embedding_payload(payload)
            trace.update(
                {
                    "success": bool(embedding),
                    "status": response.status,
                    "dimension": len(embedding),
                }
            )
            if not embedding:
                trace["error"] = "embedding response did not contain a numeric vector"
            return embedding, trace
    except urlerror.HTTPError as exc:
        trace.update(
            {
                "status": exc.code,
                "error": "embedding endpoint returned an HTTP error",
                "data": _dify_parse_json(exc.read(200_000)),
            }
        )
    except urlerror.URLError as exc:
        trace.update(
            {
                "error": "embedding endpoint connection failed",
                "detail": _clean_text(getattr(exc, "reason", exc), max_len=500),
            }
        )
    except TimeoutError:
        trace["error"] = "embedding endpoint request timed out"
    return [], trace


def _message_store_readiness_error() -> dict[str, Any] | None:
    if not _message_store_database_url():
        return {
            "success": False,
            "error": "Qintopia message store database URL is not configured",
            "required_env": [
                "QINTOPIA_MESSAGE_STORE_DATABASE_URL",
                "QINTOPIA_SIDECAR_DATABASE_URL",
                "DATABASE_URL",
            ],
        }
    return None


def _message_store_timestamp_filter(raw: Any) -> datetime | None:
    text = _clean_text(raw, max_len=80)
    if not text:
        return None
    try:
        return datetime.fromisoformat(text)
    except ValueError as exc:
        raise ValueError(f"invalid timestamp: {text}") from exc


def _message_store_row(row: Any) -> dict[str, Any]:
    return {
        "id": str(row["id"]),
        "platform": row["platform"],
        "message_id": row["message_id"],
        "chat_id": row["chat_id"],
        "chat_type": row["chat_type"],
        "sender_id": row["sender_id"],
        "sender_name": row["sender_name"],
        "message_kind": row["message_kind"],
        "text": row["text"],
        "is_mention_bot": bool(row["is_mention_bot"]),
        "should_trigger": bool(row["should_trigger"]),
        "trigger_reason": row["trigger_reason"],
        "sent_at": row["sent_at"].isoformat() if row["sent_at"] else None,
        "received_at": row["received_at"].isoformat() if row["received_at"] else None,
        "created_at": row["created_at"].isoformat() if row["created_at"] else None,
    }


def _message_store_row_get(row: Any, key: str, default: Any = None) -> Any:
    if hasattr(row, "get"):
        return row.get(key, default)
    try:
        return row[key]
    except (KeyError, TypeError):
        return default


async def _message_store_search_async(args: dict[str, Any]) -> dict[str, Any]:
    import asyncpg

    query = _clean_text(args.get("query"), max_len=500)
    search_mode = _message_store_search_mode(args)
    query_terms = _message_store_query_terms(query) if query else []
    chat_id = _clean_text(args.get("chat_id"), max_len=200)
    sender_id = _clean_text(args.get("sender_id"), max_len=200)
    chat_type = _clean_text(args.get("chat_type"), max_len=40)
    message_kind = _clean_text(args.get("message_kind"), max_len=80)
    since = _message_store_timestamp_filter(args.get("since"))
    until = _message_store_timestamp_filter(args.get("until"))
    limit = _message_store_limit(args)

    def build_filters(values: list[Any], *, include_keyword: bool = False) -> list[str]:
        where = ["m.platform = 'qiwe'"]

        def add(value: Any) -> str:
            values.append(value)
            return f"${len(values)}"

        if include_keyword and query_terms:
            term_placeholders = [add(term) for term in query_terms]
            where.append(
                "("
                + " OR ".join(f"m.text ILIKE '%' || {placeholder} || '%'" for placeholder in term_placeholders)
                + ")"
            )
        if chat_id:
            where.append(f"m.chat_id = {add(chat_id)}")
        if sender_id:
            where.append(f"m.sender_id = {add(sender_id)}")
        if chat_type in {"group", "direct"}:
            where.append(f"m.chat_type = {add(chat_type)}")
        if message_kind:
            where.append(f"m.message_kind = {add(message_kind)}")
        if since:
            where.append(f"COALESCE(m.sent_at, m.received_at) >= {add(since)}::timestamptz")
        if until:
            where.append(f"COALESCE(m.sent_at, m.received_at) <= {add(until)}::timestamptz")
        return where

    def add_limit(values: list[Any], value: int) -> str:
        values.append(value)
        return f"${len(values)}"

    base_select = """
        SELECT
            m.id, m.platform, m.message_id, m.chat_id, m.chat_type, m.sender_id, m.sender_name,
            m.message_kind, m.text, m.is_mention_bot, m.should_trigger, m.trigger_reason,
            m.sent_at, m.received_at, m.created_at
        FROM qintopia_messages.messages
    """

    async def fetch_keyword_rows(conn: Any, row_limit: int) -> tuple[list[Any], dict[str, Any]]:
        values: list[Any] = []
        where = build_filters(values, include_keyword=bool(query_terms))
        limit_placeholder = add_limit(values, row_limit)
        sql = f"""
            {base_select} m
            WHERE {' AND '.join(where)}
            ORDER BY COALESCE(m.sent_at, m.received_at) DESC, m.created_at DESC
            LIMIT {limit_placeholder}
        """
        rows = await conn.fetch(sql, *values)
        return rows, {
            "search_method": "keyword" if query_terms else "recent",
            "success": True,
            "query_terms": query_terms,
            "result_count": len(rows),
        }

    async def fetch_recent_rows(conn: Any, row_limit: int) -> tuple[list[Any], dict[str, Any]]:
        values: list[Any] = []
        where = build_filters(values, include_keyword=False)
        limit_placeholder = add_limit(values, row_limit)
        sql = f"""
            {base_select} m
            WHERE {' AND '.join(where)}
            ORDER BY COALESCE(m.sent_at, m.received_at) DESC, m.created_at DESC
            LIMIT {limit_placeholder}
        """
        rows = await conn.fetch(sql, *values)
        return rows, {
            "search_method": "recent",
            "success": True,
            "result_count": len(rows),
        }

    async def fetch_semantic_rows(conn: Any, row_limit: int) -> tuple[list[Any], dict[str, Any]]:
        if not query:
            return [], {
                "search_method": "semantic",
                "success": False,
                "skipped": True,
                "error": "semantic search requires query",
            }
        embedding, embedding_trace = _message_store_query_embedding(query)
        if not embedding:
            return [], {
                "search_method": "semantic",
                "success": False,
                "skipped": True,
                "embedding": embedding_trace,
            }
        values: list[Any] = []
        where = build_filters(values, include_keyword=False)
        values.append(_embedding_to_pgvector(embedding))
        vector_placeholder = f"${len(values)}"
        values.append(_message_store_embedding_db_model())
        model_placeholder = f"${len(values)}"
        limit_placeholder = add_limit(values, row_limit)
        sql = f"""
            SELECT
                m.id, m.platform, m.message_id, m.chat_id, m.chat_type, m.sender_id, m.sender_name,
                m.message_kind, m.text, m.is_mention_bot, m.should_trigger, m.trigger_reason,
                m.sent_at, m.received_at, m.created_at,
                MIN(e.embedding <=> {vector_placeholder}::vector) AS semantic_distance
            FROM qintopia_messages.message_embeddings e
            JOIN qintopia_messages.messages m ON m.id = e.message_id
            WHERE {' AND '.join(where)}
              AND e.embedding_model = {model_placeholder}
            GROUP BY
                m.id, m.platform, m.message_id, m.chat_id, m.chat_type, m.sender_id, m.sender_name,
                m.message_kind, m.text, m.is_mention_bot, m.should_trigger, m.trigger_reason,
                m.sent_at, m.received_at, m.created_at
            ORDER BY semantic_distance ASC, COALESCE(m.sent_at, m.received_at) DESC
            LIMIT {limit_placeholder}
        """
        rows = await conn.fetch(sql, *values)
        return rows, {
            "search_method": "semantic",
            "success": True,
            "embedding": embedding_trace,
            "embedding_model": _message_store_embedding_db_model(),
            "candidate_limit": row_limit,
            "result_count": len(rows),
        }

    def merge_rows(rows_by_method: list[tuple[str, list[Any]]]) -> list[dict[str, Any]]:
        merged: dict[str, dict[str, Any]] = {}
        method_weights = {"semantic": 1000, "keyword": 500, "recent": 0}
        for method, rows in rows_by_method:
            for rank, row in enumerate(rows):
                message_id = str(row["id"])
                if message_id not in merged:
                    item = _message_store_row(row)
                    item["retrieval_methods"] = []
                    item["retrieval_score"] = 0.0
                    item["semantic_distance"] = None
                    merged[message_id] = item
                item = merged[message_id]
                if method not in item["retrieval_methods"]:
                    item["retrieval_methods"].append(method)
                score = method_weights.get(method, 0) - rank
                if method == "semantic":
                    distance = _message_store_row_get(row, "semantic_distance")
                    if distance is not None:
                        try:
                            item["semantic_distance"] = float(distance)
                            score += max(0.0, 1.0 - float(distance)) * 100
                        except (TypeError, ValueError):
                            item["semantic_distance"] = None
                elif method == "keyword":
                    text = (row["text"] or "").lower()
                    matched = [term for term in query_terms if term.lower() in text]
                    item["matched_terms"] = matched
                    score += len(matched) * 10
                item["retrieval_score"] = max(float(item["retrieval_score"]), float(score))
        return sorted(
            merged.values(),
            key=lambda item: (
                item.get("retrieval_score") or 0,
                item.get("sent_at") or item.get("received_at") or "",
            ),
            reverse=True,
        )[:limit]

    conn = await asyncpg.connect(_message_store_database_url())
    try:
        await conn.execute("SET search_path TO qintopia_messages, public")
        trace: list[dict[str, Any]] = []
        rows_by_method: list[tuple[str, list[Any]]] = []
        if search_mode in {"hybrid", "semantic"}:
            try:
                semantic_rows, semantic_trace = await fetch_semantic_rows(
                    conn,
                    _message_store_semantic_candidate_limit(limit),
                )
            except Exception as exc:
                semantic_rows = []
                semantic_trace = {
                    "search_method": "semantic",
                    "success": False,
                    "error": "semantic search query failed",
                    "detail": _clean_text(exc, max_len=500),
                }
            trace.append(semantic_trace)
            if semantic_rows:
                rows_by_method.append(("semantic", semantic_rows))
        if search_mode in {"hybrid", "keyword"}:
            keyword_rows, keyword_trace = await fetch_keyword_rows(conn, limit)
            trace.append(keyword_trace)
            if keyword_rows:
                rows_by_method.append(("keyword", keyword_rows))
        if search_mode in {"hybrid", "recent"} or (search_mode == "semantic" and not rows_by_method):
            recent_rows, recent_trace = await fetch_recent_rows(conn, limit)
            trace.append(recent_trace)
            if recent_rows:
                rows_by_method.append(("recent", recent_rows))
        messages = merge_rows(rows_by_method)
    finally:
        await conn.close()
    return {
        "success": True,
        "skill": "qintopia_message_store_search",
        "source": "postgres_qintopia_messages",
        "read_only": True,
        "query": query,
        "query_terms": query_terms,
        "search_mode": search_mode,
        "retrieval_trace": trace,
        "filters": {
            "chat_id": chat_id,
            "sender_id": sender_id,
            "chat_type": chat_type,
            "message_kind": message_kind,
            "since": since.isoformat() if since else "",
            "until": until.isoformat() if until else "",
        },
        "result_count": len(messages),
        "messages": messages,
    }


def _run_message_store_search(args: dict[str, Any]) -> dict[str, Any]:
    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(_message_store_search_async(args))

    with ThreadPoolExecutor(max_workers=1) as executor:
        return executor.submit(lambda: asyncio.run(_message_store_search_async(args))).result()


def _lookup_safe_reply_guidance(can_answer: bool, caller_profile: str) -> str:
    if can_answer:
        return "可以基于 answer_basis 组织回复；不要提及内部来源、工具名或 Dify。"
    if caller_profile == "xiaoqin":
        return "不要直接回答；请记录需求，并通过商机跟进或披露审核交给团队负责人确认。"
    return "不要直接回答；请转人工/负责人确认。投诉或服务不满场景继续使用受控投诉受理流程。"


def _resolve_requester_channel_user_id(args: dict[str, Any], source_channel: str) -> str:
    explicit = _clean_text(args.get("requester_channel_user_id"), max_len=160)
    if explicit:
        return explicit
    if source_channel in {"qiwe_group_internal", "qiwe_direct"}:
        return _clean_text(_session_env("HERMES_SESSION_USER_ID"), max_len=160)
    return ""


def _complaint_idempotency_key(args: dict[str, Any]) -> str:
    explicit = _clean_text(args.get("idempotency_key"), max_len=200)
    if explicit:
        return explicit
    source = "|".join(
        [
            _clean_text(args.get("source_channel"), max_len=80),
            _clean_text(args.get("source_conversation_id"), max_len=160),
            _clean_text(args.get("source_message_id"), max_len=160),
            _clean_text(args.get("original_message"), max_len=600),
        ]
    )
    return "qintopia-complaint-" + hashlib.sha256(source.encode("utf-8")).hexdigest()[:24]


def _complaint_followup_idempotency_key(task_id: str, requester_id: str, approved_resolution: str) -> str:
    source = "|".join([task_id, requester_id, approved_resolution])
    return "qintopia-complaint-followup-" + hashlib.sha256(source.encode("utf-8")).hexdigest()[:24]


def _private_detail_request_message(name: str) -> str:
    prefix = f"{name}，" if name else ""
    return (
        f"{prefix}我已经把这件事作为投诉/反馈提交给大总管分诊了。\n\n"
        "为了避免在群里公开你的细节，我在这里补充确认几项就好：\n"
        "1. 希望怎么称呼你？\n"
        "2. 具体发生了什么？\n"
        "3. 大概时间、地点或涉及区域是哪里？\n"
        "4. 是否已经联系过前台、小客服或工作人员？\n"
        "5. 你期望怎么处理，或者希望什么时候收到回复？"
    )


def _complaint_task_title(summary: str) -> str:
    base = summary or "投诉/反馈待分诊"
    return f"投诉受理：{base[:48]}"


def _complaint_task_body(args: dict[str, Any], summary: str) -> str:
    requester = _clean_text(args.get("requester_display_name"), max_len=80) or "未提供"
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    requester_id = _resolve_requester_channel_user_id(args, source_channel) or "未提供"
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    message_id = _clean_text(args.get("source_message_id"), max_len=160) or "未提供"
    source_url = _clean_text(args.get("source_message_url"), max_len=300) or "未提供"
    original = _body_text(args.get("original_message"))
    return textwrap.dedent(
        f"""
        ## 投诉/反馈摘要

        {summary}

        ## 来源

        - 发起人：{requester}
        - 渠道用户 ID：{requester_id}
        - 来源渠道：{source_channel}
        - 会话 ID：{conversation_id}
        - 消息 ID：{message_id}
        - 来源链接：{source_url}

        ## 原始反馈

        {original}

        ## 待补充信息

        - 希望如何称呼投诉人
        - 具体内容
        - 发生时间、地点或涉及区域
        - 是否已联系前台、小客服或工作人员
        - 期望的处理方式或回复时间

        ## 首次私聊状态

        二花入口会通过受控 `qiwe_send_direct_message` 动作向投诉人发送一次私聊收集请求。
        大总管不要再创建或派发“二花私聊补充受理”子任务，避免投诉人收到重复私聊。
        投诉人回复后，二花应通过 `qintopia_complaint_intake_update` 补充到同一张投诉卡。

        ## 受理边界

        二花只负责受理、私聊收集最小必要信息、补充到同一张卡，并在处理结果批准后回访。
        大总管负责分诊、派单、确认处理结果和对外回复口径。
        """
    ).strip()


def _kanban_runtime():
    try:
        from hermes_cli import kanban_db as kb  # type: ignore
    except Exception:
        return None, None
    try:
        conn = kb.connect(board=COMPLAINT_BOARD)
    except Exception:
        return kb, None
    return kb, conn


def _kanban_create_complaint(title: str, body: str, priority: int, idempotency_key: str) -> tuple[str | None, str]:
    kb, conn = _kanban_runtime()
    if kb is None or conn is None:
        return None, "dry_run_no_hermes_kanban_runtime"
    try:
        task_id = kb.create_task(
            conn,
            title=title,
            body=body,
            assignee=COMPLAINT_OWNER_PROFILE,
            created_by="erhua",
            tenant=QINTOPIA_TENANT,
            priority=priority,
            triage=True,
            idempotency_key=idempotency_key,
            initial_status="blocked",
            workspace_kind="scratch",
            board=COMPLAINT_BOARD,
        )
        return str(task_id), "created"
    finally:
        conn.close()


def _kanban_add_complaint_comment(task_id: str, body: str) -> tuple[int | None, str]:
    kb, conn = _kanban_runtime()
    if kb is None or conn is None:
        return None, "dry_run_no_hermes_kanban_runtime"
    try:
        comment_id = kb.add_comment(conn, task_id, "erhua", body)
        return int(comment_id), "comment_added"
    finally:
        conn.close()


def _sales_idempotency_key(args: dict[str, Any]) -> str:
    explicit = _clean_text(args.get("idempotency_key"), max_len=200)
    if explicit:
        return explicit
    source = "|".join(
        [
            _clean_text(args.get("task_type") or "sales_lead", max_len=80),
            _clean_text(args.get("source_channel"), max_len=80),
            _clean_text(args.get("source_conversation_id"), max_len=160),
            _clean_text(args.get("source_message_id"), max_len=160),
            _clean_text(args.get("customer_request"), max_len=600),
        ]
    )
    return "qintopia-sales-" + hashlib.sha256(source.encode("utf-8")).hexdigest()[:24]


def _sales_task_title(task_type: str, customer: str, request: str) -> str:
    label = SALES_TASK_TYPES.get(task_type, SALES_TASK_TYPES["sales_lead"])["label"]
    who = customer or "外部客户"
    summary = _clean_text(request, max_len=42) or "待补充需求"
    return f"{label}：{who} - {summary}"


def _sales_task_body(args: dict[str, Any], task_type: str) -> str:
    customer = _clean_text(args.get("customer_display_name"), max_len=120) or "未提供"
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    message_id = _clean_text(args.get("source_message_id"), max_len=160) or "未提供"
    source_url = _clean_text(args.get("source_message_url"), max_len=300) or "未提供"
    request = _body_text(args.get("customer_request"))
    business_scenario = _body_text(args.get("business_scenario"), max_len=1200) or "待补充"
    public_answer = _body_text(args.get("public_answer_given"), max_len=1600) or "尚未形成公开回答"
    boundary_trigger = _body_text(args.get("boundary_trigger"), max_len=800) or "未触发特殊边界"
    fields = [
        ("预算范围", _clean_text(args.get("budget_range"), max_len=180)),
        ("紧急程度", _clean_text(args.get("urgency"), max_len=180)),
        ("现有系统", _clean_text(args.get("current_system"), max_len=260)),
        ("试点期望", _clean_text(args.get("pilot_expectation"), max_len=500)),
        ("建议下一步", _clean_text(args.get("next_step"), max_len=500) or "由小秦或团队负责人继续确认"),
    ]
    field_lines = [f"- {label}：{value}" for label, value in fields if value]
    return textwrap.dedent(
        f"""
        ## 客户与来源

        - 客户：{customer}
        - 来源渠道：{source_channel}
        - 会话 ID：{conversation_id}
        - 消息 ID：{message_id}
        - 来源链接：{source_url}
        - 任务类型：{task_type}

        ## 客户诉求

        {request}

        ## 场景与约束

        {business_scenario}

        {chr(10).join(field_lines) if field_lines else "- 待补充预算、紧急程度、现有系统和试点期望。"}

        ## 已公开回答

        {public_answer}

        ## 触发边界

        {boundary_trigger}

        ## 需要团队负责人决策

        - 报价、合同、交付范围、排期、SLA 或客户案例细节需要团队负责人确认。
        - 若涉及内部资料、未公开案例、客户数据或基础设施细节，需要先做披露审核。

        ## 建议下一步

        {_clean_text(args.get("next_step"), max_len=500) or "确认客户场景、预算范围、演示时间和可使用的公开/脱敏材料。"}
        """
    ).strip()


def _kanban_create_sales_task(title: str, body: str, task_type: str, priority: int, idempotency_key: str) -> tuple[str | None, str]:
    kb, conn = _kanban_runtime()
    if kb is None or conn is None:
        return None, "dry_run_no_hermes_kanban_runtime"
    status = SALES_TASK_TYPES.get(task_type, SALES_TASK_TYPES["sales_lead"])["status"]
    initial_status = "blocked" if status == "review" else "running"
    assignee = "default" if task_type == "external_disclosure_review" else SALES_OWNER_PROFILE
    try:
        task_id = kb.create_task(
            conn,
            title=title,
            body=body,
            assignee=assignee,
            created_by=SALES_OWNER_PROFILE,
            tenant=QINTOPIA_TENANT,
            priority=priority,
            triage=status == "triage",
            idempotency_key=idempotency_key,
            initial_status=initial_status,
            workspace_kind="scratch",
            board=SALES_BOARD,
        )
        return str(task_id), "created"
    finally:
        conn.close()


def _safe_list(value: str) -> list[str]:
    return [line.strip(" -\t") for line in str(value or "").splitlines() if line.strip()]


SENSITIVE_DISCLOSURE_PATTERNS = {
    "internal_information": ["内部", "未公开", "私有", "prompt", "提示词", "服务器", "日志", "数据库", "路由", "成本", "模型供应商"],
    "commercial_commitment": ["报价", "价格", "折扣", "合同", "交付", "排期", "sla", "验收", "付款", "采购"],
    "customer_or_member_data": ["客户资料", "客户案例", "成员资料", "联系方式", "聊天记录", "入住信息", "村民档案"],
    "credentials": ["secret", "token", "credential", "password", "密钥", "凭证", "私钥"],
}


def _disclosure_hits(text: str) -> dict[str, list[str]]:
    lowered = text.lower()
    hits: dict[str, list[str]] = {}
    for category, patterns in SENSITIVE_DISCLOSURE_PATTERNS.items():
        matched = [pattern for pattern in patterns if pattern.lower() in lowered]
        if matched:
            hits[category] = matched[:5]
    return hits


def _index_dir() -> Path:
    return Path(os.getenv("QINTOPIA_KB_INDEX_DIR", "") or DEFAULT_INDEX_DIR)


def _read_jsonl(path: Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    rows: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def _tokenize(value: str) -> list[str]:
    return [
        part.lower()
        for part in re.split(r"[\s,.;:!?，。；：！？、/\\|()（）\[\]【】\"']+", value)
        if part.strip()
    ]


def _snippet(text: str, terms: list[str], size: int = 220) -> str:
    collapsed = re.sub(r"\s+", " ", text).strip()
    lower = collapsed.lower()
    positions = [lower.find(term) for term in terms if term and lower.find(term) >= 0]
    if not positions:
        return collapsed[:size]
    start = max(min(positions) - 48, 0)
    return collapsed[start : start + size]


def _kb_score(record: dict[str, Any], terms: list[str]) -> int:
    title = str(record.get("title", "")).lower()
    path = str(record.get("path", "")).lower()
    body = str(record.get("body", "")).lower()
    score = 0
    for term in terms:
        if not term:
            continue
        if term in title:
            score += 12
        if term in path:
            score += 6
        score += min(body.count(term), 10) * 2
    return score


def _requested_classes(args: dict[str, Any]) -> tuple[list[str], list[str]]:
    raw = args.get("information_classes")
    if not raw:
        return ["Public"], []
    if isinstance(raw, str):
        requested = [raw]
    elif isinstance(raw, list):
        requested = [str(item) for item in raw]
    else:
        requested = []

    allowed: list[str] = []
    denied: list[str] = []
    for info_class in requested:
        if info_class not in INDEX_FILES:
            denied.append(info_class)
            continue
        if info_class == "Member-scoped" and not bool(args.get("allow_member_scoped")):
            denied.append(info_class)
            continue
        allowed.append(info_class)
    return allowed or ["Public"], denied


def handle_qintopia_dify_dataset_list(args: dict[str, Any], **_: Any) -> str:
    params = {
        "page": _dify_page(args),
        "limit": _dify_limit(args),
        "keyword": _clean_text(args.get("keyword"), max_len=200),
    }
    response = _dify_request("GET", "/datasets", params=params)
    if response.get("success"):
        response = {**response, "data": _filter_dify_dataset_list(response.get("data"))}
    return _dify_tool_payload(
        "qintopia_dify_dataset_list",
        "list_datasets",
        response,
        page=params["page"],
        limit=params["limit"],
        filtered_to_allowed_datasets=bool(_dify_allowed_dataset_ids()),
    )


def handle_qintopia_dify_dataset_get(args: dict[str, Any], **_: Any) -> str:
    dataset_id = _dify_dataset_id(args)
    denied = _dify_dataset_denied(dataset_id)
    if denied:
        return _json(denied)
    response = _dify_request("GET", f"/datasets/{quote(dataset_id, safe='')}")
    return _dify_tool_payload(
        "qintopia_dify_dataset_get",
        "get_dataset",
        response,
        dataset_id=dataset_id,
    )


def handle_qintopia_dify_knowledge_retrieve(args: dict[str, Any], **_: Any) -> str:
    dataset_id = _dify_dataset_id(args)
    denied = _dify_dataset_denied(dataset_id)
    if denied:
        return _json(denied)
    query = _clean_text(args.get("query"), max_len=1000)
    if not query:
        return _json({"success": False, "error": "query is required"})
    try:
        top_k = min(max(int(args.get("top_k") or 5), 1), 10)
    except (TypeError, ValueError):
        top_k = 5
    search_method = _clean_text(args.get("search_method"), max_len=80) or "semantic_search"
    if search_method not in {"semantic_search", "full_text_search", "hybrid_search"}:
        return _json({"success": False, "error": "search_method is not allowed"})
    retrieval_model: dict[str, Any] = {
        "search_method": search_method,
        "top_k": top_k,
        "score_threshold_enabled": bool(args.get("score_threshold_enabled", False)),
        "reranking_enable": bool(args.get("reranking_enable", False)),
    }
    if retrieval_model["score_threshold_enabled"]:
        try:
            retrieval_model["score_threshold"] = min(max(float(args.get("score_threshold") or 0), 0), 1)
        except (TypeError, ValueError):
            retrieval_model["score_threshold"] = 0
    response = _dify_request(
        "POST",
        f"/datasets/{quote(dataset_id, safe='')}/retrieve",
        body={"query": query, "retrieval_model": retrieval_model},
    )
    return _dify_tool_payload(
        "qintopia_dify_knowledge_retrieve",
        "retrieve_chunks",
        response,
        dataset_id=dataset_id,
        query=query,
        retrieval_model=retrieval_model,
    )


def handle_qintopia_dify_document_list(args: dict[str, Any], **_: Any) -> str:
    dataset_id = _dify_dataset_id(args)
    denied = _dify_dataset_denied(dataset_id)
    if denied:
        return _json(denied)
    params = {
        "page": _dify_page(args),
        "limit": _dify_limit(args),
        "keyword": _clean_text(args.get("keyword"), max_len=200),
    }
    response = _dify_request(
        "GET",
        f"/datasets/{quote(dataset_id, safe='')}/documents",
        params=params,
    )
    return _dify_tool_payload(
        "qintopia_dify_document_list",
        "list_documents",
        response,
        dataset_id=dataset_id,
        page=params["page"],
        limit=params["limit"],
    )


def handle_qintopia_dify_document_get(args: dict[str, Any], **_: Any) -> str:
    dataset_id = _dify_dataset_id(args)
    document_id = _dify_document_id(args)
    denied = _dify_dataset_denied(dataset_id)
    if denied:
        return _json(denied)
    if not document_id:
        return _json({"success": False, "error": "document_id is required"})
    response = _dify_request(
        "GET",
        f"/datasets/{quote(dataset_id, safe='')}/documents/{quote(document_id, safe='')}",
    )
    return _dify_tool_payload(
        "qintopia_dify_document_get",
        "get_document",
        response,
        dataset_id=dataset_id,
        document_id=document_id,
    )


def handle_qintopia_dify_indexing_status_get(args: dict[str, Any], **_: Any) -> str:
    dataset_id = _dify_dataset_id(args)
    batch = _clean_text(args.get("batch"), max_len=160)
    denied = _dify_dataset_denied(dataset_id)
    if denied:
        return _json(denied)
    if not batch:
        return _json({"success": False, "error": "batch is required"})
    response = _dify_request(
        "GET",
        f"/datasets/{quote(dataset_id, safe='')}/documents/{quote(batch, safe='')}/indexing-status",
    )
    return _dify_tool_payload(
        "qintopia_dify_indexing_status_get",
        "get_indexing_status",
        response,
        dataset_id=dataset_id,
        batch=batch,
    )


def handle_qintopia_dify_segment_list(args: dict[str, Any], **_: Any) -> str:
    dataset_id = _dify_dataset_id(args)
    document_id = _dify_document_id(args)
    denied = _dify_dataset_denied(dataset_id)
    if denied:
        return _json(denied)
    if not document_id:
        return _json({"success": False, "error": "document_id is required"})
    params = {
        "page": _dify_page(args),
        "limit": _dify_limit(args),
        "keyword": _clean_text(args.get("keyword"), max_len=200),
        "status": _clean_text(args.get("status"), max_len=80),
    }
    response = _dify_request(
        "GET",
        f"/datasets/{quote(dataset_id, safe='')}/documents/{quote(document_id, safe='')}/segments",
        params=params,
    )
    return _dify_tool_payload(
        "qintopia_dify_segment_list",
        "list_segments",
        response,
        dataset_id=dataset_id,
        document_id=document_id,
        page=params["page"],
        limit=params["limit"],
    )


def handle_qintopia_dify_segment_get(args: dict[str, Any], **_: Any) -> str:
    dataset_id = _dify_dataset_id(args)
    document_id = _dify_document_id(args)
    segment_id = _dify_segment_id(args)
    denied = _dify_dataset_denied(dataset_id)
    if denied:
        return _json(denied)
    if not document_id:
        return _json({"success": False, "error": "document_id is required"})
    if not segment_id:
        return _json({"success": False, "error": "segment_id is required"})
    response = _dify_request(
        "GET",
        (
            f"/datasets/{quote(dataset_id, safe='')}"
            f"/documents/{quote(document_id, safe='')}"
            f"/segments/{quote(segment_id, safe='')}"
        ),
    )
    return _dify_tool_payload(
        "qintopia_dify_segment_get",
        "get_segment",
        response,
        dataset_id=dataset_id,
        document_id=document_id,
        segment_id=segment_id,
    )


def handle_qintopia_wenyuange_lookup(args: dict[str, Any], **_: Any) -> str:
    query = _clean_text(args.get("query"), max_len=1000)
    caller_profile = _clean_text(args.get("caller_profile"), max_len=80)
    audience = _clean_text(args.get("audience"), max_len=80)
    purpose = _clean_text(args.get("purpose"), max_len=500)
    if not query:
        return _json({"success": False, "error": "query is required"})
    if not purpose:
        return _json({"success": False, "error": "purpose is required"})
    if (caller_profile, audience) not in {
        ("erhua", "member_reply"),
        ("xiaoqin", "external_customer"),
    }:
        return _json(
            {
                "success": False,
                "error": "caller_profile and audience combination is not allowed",
                "allowed_combinations": [
                    {"caller_profile": "erhua", "audience": "member_reply"},
                    {"caller_profile": "xiaoqin", "audience": "external_customer"},
                ],
            }
        )

    dataset_id = _dify_lookup_dataset_id()
    if not dataset_id:
        return _json(
            {
                "success": False,
                "error": "exactly one Dify lookup dataset must be configured",
                "required_env": [
                    "QINTOPIA_DIFY_LOOKUP_DATASET_ID",
                    "QINTOPIA_DIFY_ALLOWED_DATASET_IDS",
                ],
            }
        )

    try:
        top_k = min(max(int(args.get("top_k") or 3), 1), 5)
    except (TypeError, ValueError):
        top_k = 3

    records, retrieval_trace, retrieve_error = _dify_lookup_records(dataset_id, query, top_k)
    if retrieve_error:
        return _json(
            {
                "success": False,
                "skill": "qintopia_wenyuange_lookup",
                "caller_profile": caller_profile,
                "audience": audience,
                "error": retrieve_error.get("error") or "Dify lookup failed",
                "status": retrieve_error.get("status"),
                "can_answer": False,
                "risk_flags": ["lookup_failed"],
                "retrieval_trace": retrieval_trace,
                "safe_reply_guidance": _lookup_safe_reply_guidance(False, caller_profile),
            }
        )

    safe_records, blocked_flags = _filter_lookup_records(caller_profile, audience, query, purpose, records)
    safe_records.sort(key=lambda record: _record_query_score(record, query), reverse=True)
    if safe_records and _record_source_quality(safe_records[0]) >= 30:
        safe_records = [safe_records[0]]
    else:
        safe_records = safe_records[:top_k]
    can_answer = bool(safe_records)
    risk_flags = [] if can_answer else blocked_flags
    return _json(
        {
            "success": True,
            "skill": "qintopia_wenyuange_lookup",
            "caller_profile": caller_profile,
            "audience": audience,
            "can_answer": can_answer,
            "answer_basis": _dify_lookup_answer_basis(safe_records) if can_answer else "",
            "sources": _dify_lookup_sources(safe_records if can_answer else records),
            "risk_flags": risk_flags,
            "retrieval_trace": retrieval_trace,
            "safe_reply_guidance": _lookup_safe_reply_guidance(can_answer, caller_profile),
            "result_count": len(safe_records if can_answer else records),
            "blocked_result_count": max(len(records) - len(safe_records), 0),
            "read_only": True,
        }
    )


def handle_qintopia_message_store_search(args: dict[str, Any], **_: Any) -> str:
    caller = _clean_text(args.get("caller"), max_len=80) or _qintopia_profile_id()
    if caller != "wenyuange":
        return _json(
            {
                "success": False,
                "error": "qintopia_message_store_search is only available to wenyuange",
                "caller": caller,
            }
        )
    purpose = _clean_text(args.get("purpose"), max_len=500)
    if not purpose:
        return _json({"success": False, "error": "purpose is required"})
    filter_keys = ["query", "chat_id", "sender_id", "chat_type", "message_kind", "since", "until"]
    if not any(_clean_text(args.get(key), max_len=500) for key in filter_keys):
        return _json(
            {
                "success": False,
                "error": "at least one message search filter is required",
                "required_filter_keys": filter_keys,
            }
        )
    readiness = _message_store_readiness_error()
    if readiness:
        return _json(readiness)
    try:
        return _json(_run_message_store_search(args))
    except Exception as exc:
        return _json(
            {
                "success": False,
                "skill": "qintopia_message_store_search",
                "source": "postgres_qintopia_messages",
                "read_only": True,
                "error": "Qintopia message store query failed",
                "detail": _clean_text(exc, max_len=500),
            }
        )


def handle_qintopia_daily_digest_publish(args: dict[str, Any], **_: Any) -> str:
    digest_id = _clean_text(args.get("digest_id"), max_len=80)
    if not digest_id:
        return _json({"success": False, "error": "digest_id is required"})
    actor_agent = _clean_text(args.get("actor_agent"), max_len=80) or _qintopia_profile_id() or "xiaoman"
    if actor_agent != "xiaoman":
        return _json({"success": False, "error": "actor_agent must be xiaoman", "actor_agent": actor_agent})
    dry_run = bool(args.get("dry_run", False))
    if not _daily_digest_publish_enabled():
        return _json(
            {
                "success": False,
                "skill": DAILY_DIGEST_PUBLISH_TOOL,
                "error": "QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE=1 is required",
                "actor_agent": actor_agent,
                "digest_id": digest_id,
                "guardrails": [
                    "disabled by default",
                    "digest_id only",
                    "no generic Feishu document write",
                    "publisher must enforce chat and parent-node allowlists",
                ],
            }
        )
    command = [
        _daily_digest_publisher_bin(),
        "daily-digest-publish",
        "--digest-id",
        digest_id,
        "--actor-agent",
        actor_agent,
        "--dry-run" if dry_run else "--apply",
    ]
    return _json(
        {
            "success": True,
            "skill": DAILY_DIGEST_PUBLISH_TOOL,
            "actor_agent": actor_agent,
            "digest_id": digest_id,
            "dry_run": dry_run,
            "action": {
                "tool": "agentos_worker_command",
                "command": command,
                "shell_preview": " ".join(shlex.quote(part) for part in command),
                "requires_local_execution": True,
                "idempotency_key": f"{DAILY_DIGEST_PUBLISH_TOOL}:{digest_id}:{'dry_run' if dry_run else 'apply'}",
            },
            "guardrails": [
                "digest_id only; no arbitrary Markdown",
                "actor_agent must be xiaoman",
                "publisher validates owner_agent, target chat, parent node, and publish status",
                "publisher writes audit rows for success, failure, or denial",
                "does not post to QiWe groups",
            ],
        }
    )


def _xiaoman_activity_actor(args: dict[str, Any]) -> str:
    return _clean_text(args.get("actor_agent"), max_len=80) or _qintopia_profile_id() or "xiaoman"


def _clean_string_list(value: Any, *, max_items: int = 20, max_len: int = 120) -> list[str]:
    if value is None:
        return []
    raw_items = value if isinstance(value, list) else [value]
    cleaned: list[str] = []
    for item in raw_items[:max_items]:
        text = _clean_text(item, max_len=max_len)
        if text:
            cleaned.append(text)
    return cleaned


def _xiaoman_activity_error(skill: str, error: str, **extra: Any) -> str:
    payload: dict[str, Any] = {
        "success": False,
        "skill": skill,
        "error": error,
            "guardrails": [
                "actor_agent must be xiaoman",
                "disabled unless QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1",
                "structured activity operations only",
                "no raw Base skill, arbitrary Feishu URL, generic SQL, shell, or Markdown input",
            ],
    }
    payload.update(extra)
    return _json(payload)


def _xiaoman_activity_safe_skip(skill: str, status: str, **extra: Any) -> str:
    payload: dict[str, Any] = {
        "success": True,
        "skill": skill,
        "safe_status": status,
        "action": {
            "tool": "none",
            "reason": "connectivity_or_test_payload_skipped_before_worker",
        },
        "guardrails": [
            "test/connectivity payloads must not read or write Feishu Base",
            "test/connectivity payloads must not dispatch WeCom, Kanban, or downstream agents",
            "do not expose Base internals, record ids, commands, or traces to WeCom users",
        ],
    }
    payload.update(extra)
    return _json(payload)


def _xiaoman_activity_is_connectivity_probe(args: dict[str, Any], payload: dict[str, Any]) -> bool:
    markers = [
        _clean_text(payload.get("record_id"), max_len=200),
        _clean_text(payload.get("source_record_id"), max_len=200),
        _clean_text(args.get("idempotency_key"), max_len=240),
        _clean_text(payload.get("status"), max_len=120),
        _clean_text(payload.get("status_note"), max_len=500),
        _clean_text(payload.get("material_notes"), max_len=500),
        _clean_text(payload.get("brief_summary"), max_len=500),
    ]
    marker_text = " ".join(markers).lower()
    if "test_record" in marker_text:
        return True
    return any(
        token in marker_text
        for token in [
            "connectivity_probe",
            "safe-agent",
            "safe_agent",
            "gateway_shadow",
            "连通测试",
            "不要写业务记录",
        ]
    )


def _xiaoman_activity_command(
    *,
    skill: str,
    operation: str,
    args: dict[str, Any],
    payload: dict[str, Any],
    required: list[str],
    writes_business_state: bool,
) -> str:
    actor_agent = _xiaoman_activity_actor(args)
    if actor_agent != "xiaoman":
        return _xiaoman_activity_error(skill, "actor_agent must be xiaoman", actor_agent=actor_agent)
    if not _xiaoman_activity_wrappers_enabled():
        return _xiaoman_activity_error(
            skill,
            "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1 is required",
            actor_agent=actor_agent,
        )
    for field in required:
        if not _clean_text(payload.get(field), max_len=500):
            return _xiaoman_activity_error(skill, f"{field} is required", actor_agent=actor_agent)

    if _xiaoman_activity_is_connectivity_probe(args, payload):
        return _xiaoman_activity_safe_skip(
            skill,
            "skip_connectivity_probe",
            actor_agent=actor_agent,
            operation=operation,
        )

    table_role = _clean_text(payload.get("table_role"), max_len=80)
    if table_role and table_role not in XIAOMAN_ACTIVITY_TABLE_ROLES:
        return _xiaoman_activity_error(skill, "table_role is not allowed", table_role=table_role)

    target_agent = _clean_text(payload.get("target_agent"), max_len=80)
    if target_agent and target_agent not in XIAOMAN_ACTIVITY_HANDOFF_TARGETS:
        return _xiaoman_activity_error(skill, "target_agent is not allowed", target_agent=target_agent)

    handoff_type = _clean_text(payload.get("handoff_type"), max_len=80)
    if handoff_type and handoff_type not in XIAOMAN_ACTIVITY_HANDOFF_TYPES:
        return _xiaoman_activity_error(skill, "handoff_type is not allowed", handoff_type=handoff_type)

    risk_level = _clean_text(payload.get("risk_level"), max_len=80)
    if risk_level and risk_level not in {"low", "medium", "high"}:
        return _xiaoman_activity_error(skill, "risk_level is not allowed", risk_level=risk_level)

    default_dry_run = writes_business_state
    dry_run = bool(args.get("dry_run", default_dry_run))
    payload = {key: value for key, value in payload.items() if value not in ("", None, [])}
    payload["actor_agent"] = actor_agent
    payload["operation"] = operation
    payload["dry_run"] = dry_run
    payload_json = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))

    explicit_key = _clean_text(args.get("idempotency_key"), max_len=200)
    idempotency_seed = explicit_key or hashlib.sha256(payload_json.encode("utf-8")).hexdigest()[:24]
    idempotency_key = f"{skill}:{idempotency_seed}:{'dry_run' if dry_run else 'apply'}"
    command = [
        _xiaoman_activity_worker_bin(),
        "xiaoman-activity",
        operation,
        "--payload-json",
        payload_json,
    ]
    fixture_path = _xiaoman_activity_fixture_path()
    if fixture_path:
        command.extend(["--fixture-path", fixture_path])
    if _xiaoman_activity_use_feishu_base():
        command.append("--use-feishu-base")
    command.append("--dry-run" if dry_run else "--apply")
    return _json(
        {
            "success": True,
            "skill": skill,
            "actor_agent": actor_agent,
            "operation": operation,
            "dry_run": dry_run,
            "writes_business_state": writes_business_state,
            "payload": payload,
            "action": {
                "tool": "agentos_worker_command",
                "command": command,
                "shell_preview": " ".join(shlex.quote(part) for part in command),
                "requires_local_execution": True,
                "idempotency_key": idempotency_key,
            },
            "guardrails": [
                "xiaoman activity wrapper only; no generic Base access",
                "worker must enforce approved table/field allowlists",
                "worker must write audit rows for success, failure, or denial",
                "write operations default to dry-run",
                "do not expose Base internals, record ids, commands, or traces to WeCom users",
            ],
        }
    )


def handle_qintopia_xiaoman_activity_record_get(args: dict[str, Any], **_: Any) -> str:
    payload = {
        "record_id": _clean_text(args.get("record_id"), max_len=160),
        "table_role": _clean_text(args.get("table_role"), max_len=80),
    }
    return _xiaoman_activity_command(
        skill="qintopia_xiaoman_activity_record_get",
        operation="record-get",
        args=args,
        payload=payload,
        required=["record_id", "table_role"],
        writes_business_state=False,
    )


def handle_qintopia_xiaoman_activity_list_by_date(args: dict[str, Any], **_: Any) -> str:
    payload = {
        "date": _clean_text(args.get("date"), max_len=40),
        "table_role": _clean_text(args.get("table_role") or "activity_plan", max_len=80),
        "timezone": _clean_text(args.get("timezone") or "Asia/Shanghai", max_len=80),
    }
    return _xiaoman_activity_command(
        skill="qintopia_xiaoman_activity_list_by_date",
        operation="list-by-date",
        args=args,
        payload=payload,
        required=["date", "table_role"],
        writes_business_state=False,
    )


def handle_qintopia_xiaoman_activity_status_update(args: dict[str, Any], **_: Any) -> str:
    payload = {
        "record_id": _clean_text(args.get("record_id"), max_len=160),
        "table_role": _clean_text(args.get("table_role"), max_len=80),
        "status": _clean_text(args.get("status"), max_len=120),
        "status_note": _body_text(args.get("status_note"), max_len=800),
    }
    return _xiaoman_activity_command(
        skill="qintopia_xiaoman_activity_status_update",
        operation="status-update",
        args=args,
        payload=payload,
        required=["record_id", "table_role", "status"],
        writes_business_state=True,
    )


def handle_qintopia_xiaoman_activity_gap_update(args: dict[str, Any], **_: Any) -> str:
    payload = {
        "record_id": _clean_text(args.get("record_id"), max_len=160),
        "table_role": _clean_text(args.get("table_role"), max_len=80),
        "gap_summary": _body_text(args.get("gap_summary"), max_len=1000),
        "missing_fields": _clean_string_list(args.get("missing_fields")),
    }
    return _xiaoman_activity_command(
        skill="qintopia_xiaoman_activity_gap_update",
        operation="gap-update",
        args=args,
        payload=payload,
        required=["record_id", "table_role", "gap_summary"],
        writes_business_state=True,
    )


def handle_qintopia_xiaoman_activity_handoff_create(args: dict[str, Any], **_: Any) -> str:
    payload = {
        "source_record_id": _clean_text(args.get("source_record_id"), max_len=160),
        "source_event_signal_id": _clean_text(args.get("source_event_signal_id"), max_len=160),
        "handoff_type": _clean_text(args.get("handoff_type"), max_len=80),
        "target_agent": _clean_text(args.get("target_agent"), max_len=80),
        "brief_summary": _body_text(args.get("brief_summary"), max_len=1600),
        "purpose": _body_text(args.get("purpose"), max_len=800),
        "risk_level": _clean_text(args.get("risk_level") or "medium", max_len=80),
    }
    return _xiaoman_activity_command(
        skill="qintopia_xiaoman_activity_handoff_create",
        operation="handoff-create",
        args=args,
        payload=payload,
        required=["source_record_id", "handoff_type", "target_agent", "brief_summary"],
        writes_business_state=True,
    )


def handle_qintopia_xiaoman_activity_material_summary(args: dict[str, Any], **_: Any) -> str:
    payload = {
        "record_id": _clean_text(args.get("record_id"), max_len=160),
        "table_role": _clean_text(args.get("table_role"), max_len=80),
        "source_event_signal_id": _clean_text(args.get("source_event_signal_id"), max_len=160),
        "material_notes": _body_text(args.get("material_notes"), max_len=1600),
    }
    return _xiaoman_activity_command(
        skill="qintopia_xiaoman_activity_material_summary",
        operation="material-summary",
        args=args,
        payload=payload,
        required=["record_id", "table_role"],
        writes_business_state=False,
    )


def handle_qintopia_weather_lookup(args: dict[str, Any], **_: Any) -> str:
    location = _qintopia_weather_location()
    if "," not in location:
        return _json(
            {
                "success": False,
                "skill": QINTOPIA_WEATHER_TOOL,
                "error": "QINTOPIA_WEATHER_LOCATION must be fixed lon,lat coordinates",
                "guardrails": {
                    "fixed_location_only": True,
                    "excluded_capabilities": ["tropical_cyclone_typhoon", "ocean_marine", "solar_radiation"],
                },
            }
        )

    bundle = _qweather_call_bundle(location)
    if _qweather_successful(bundle):
        return _json(_qweather_payload(args, bundle))

    fallback = _open_meteo_fallback()
    fallback["qweather_errors"] = {
        name: {key: value for key, value in call.items() if key in {"error", "detail", "status", "exit_code"}}
        for name, call in bundle.items()
    }
    return _json(fallback)


def handle_qintopia_kb_search(args: dict[str, Any], **_: Any) -> str:
    query = str(args.get("query") or "").strip()
    terms = _tokenize(query)
    if not terms:
        return _json({"success": False, "error": "query is empty"})

    classes, denied = _requested_classes(args)
    limit = min(max(int(args.get("limit") or DEFAULT_KB_LIMIT), 1), MAX_KB_LIMIT)
    index_dir = _index_dir()
    records: list[dict[str, Any]] = []
    for info_class in classes:
        records.extend(_read_jsonl(index_dir / INDEX_FILES[info_class]))

    ranked: list[tuple[int, dict[str, Any]]] = []
    for record in records:
        score = _kb_score(record, terms)
        if score > 0:
            ranked.append((score, record))
    ranked.sort(key=lambda item: (-item[0], str(item[1].get("title", ""))))

    results = []
    for score, record in ranked[:limit]:
        body = str(record.get("body", ""))
        results.append(
            {
                "source_id": record.get("source_id"),
                "title": record.get("title"),
                "path": record.get("path"),
                "information_class": record.get("information_class"),
                "updated_at": record.get("updated_at"),
                "score": score,
                "snippet": _snippet(body, terms),
            }
        )

    return _json(
        {
            "success": True,
            "skill": "qintopia_kb_search",
            "query": query,
            "scope_used": classes,
            "denied_classes": denied,
            "result_count": len(results),
            "results": results,
            "not_accessed": [
                info_class
                for info_class in INDEX_FILES
                if info_class not in classes
            ],
        }
    )


def _plain_cell(cell: str) -> str:
    return re.sub(r"\s+", " ", cell.strip())


def _image_url(cell: str) -> str:
    match = re.search(r"!\[[^\]]*]\(([^)]+)\)", cell)
    return match.group(1).strip() if match else ""


def _parse_gis_locations(index_dir: Path) -> list[dict[str, Any]]:
    rows = _read_jsonl(index_dir / INDEX_FILES["Public"])
    gis = next((row for row in rows if row.get("path") == "gis-locations.md"), None)
    if not gis:
        return []
    locations: list[dict[str, Any]] = []
    for line in str(gis.get("body", "")).splitlines():
        stripped = line.strip()
        if not stripped.startswith("|") or "---" in stripped or "名称" in stripped:
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if len(cells) < 3:
            continue
        name = _plain_cell(cells[0])
        try:
            longitude = float(cells[1])
            latitude = float(cells[2])
        except ValueError:
            continue
        image = _image_url(cells[3]) if len(cells) > 3 else ""
        locations.append(
            {
                "name": name,
                "longitude": longitude,
                "latitude": latitude,
                "address": name,
                "amap_url": f"https://uri.amap.com/marker?position={longitude},{latitude}&name={quote(name)}",
                "image_url": image,
                "source": {
                    "source_id": gis.get("source_id"),
                    "title": gis.get("title"),
                    "path": gis.get("path"),
                    "information_class": gis.get("information_class"),
                    "updated_at": gis.get("updated_at"),
                },
            }
        )
    return locations


_CN_NUMBERS = {
    "一": "1",
    "二": "2",
    "三": "3",
    "四": "4",
    "五": "5",
    "六": "6",
    "七": "7",
    "八": "8",
    "九": "9",
}


def _normalize_location(value: str) -> str:
    normalized = str(value).lower()
    for cn, digit in _CN_NUMBERS.items():
        normalized = normalized.replace(cn, digit)
    normalized = normalized.replace("秦托邦", "").replace("qintopia", "")
    normalized = normalized.replace("幢", "栋").replace("楼", "栋")
    normalized = re.sub(r"[\s,.;:!?，。；：！？、/\\|()（）\[\]【】\"'_-]+", "", normalized)
    return normalized


def _location_score(query_norm: str, name_norm: str) -> int:
    if not query_norm or not name_norm:
        return 0
    if query_norm == name_norm:
        return 100
    if query_norm in name_norm or name_norm in query_norm:
        return 80
    if query_norm.endswith("栋") and query_norm[:-1] and query_norm[:-1] == name_norm.rstrip("栋"):
        return 70
    common = set(query_norm) & set(name_norm)
    if len(common) >= min(len(set(query_norm)), len(set(name_norm)), 2):
        return 35
    return 0


def handle_qintopia_gis_location_lookup(args: dict[str, Any], **_: Any) -> str:
    query = str(args.get("query") or "").strip()
    query_norm = _normalize_location(query)
    if not query_norm:
        return _json({"success": False, "error": "query is empty"})

    limit = min(max(int(args.get("limit") or DEFAULT_GIS_LIMIT), 1), 5)
    locations = _parse_gis_locations(_index_dir())
    scored: list[tuple[int, dict[str, Any]]] = []
    for location in locations:
        score = _location_score(query_norm, _normalize_location(str(location.get("name", ""))))
        if score > 0:
            scored.append((score, location))
    scored.sort(key=lambda item: (-item[0], str(item[1].get("name", ""))))

    candidates = []
    for score, location in scored[:limit]:
        candidate = dict(location)
        candidate["match_score"] = score
        candidate["confidence"] = "high" if score >= 80 else "medium"
        candidates.append(candidate)

    return _json(
        {
            "success": True,
            "skill": "qintopia_gis_location_lookup",
            "query": query,
            "matched": bool(candidates),
            "location": candidates[0] if candidates else None,
            "candidates": candidates,
            "scope_used": ["Public"],
            "not_accessed": ["Internal", "Member-scoped", "Feishu live", "external search"],
        }
    )


def handle_qintopia_complaint_intake_create(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_complaint_intake_create(args)


def handle_qintopia_complaint_intake_update(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_complaint_intake_update(args)


def handle_qintopia_complaint_followup_send(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_complaint_followup_send(args)


def handle_qintopia_external_product_kb_search(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_external_product_kb_search(args)


def handle_qintopia_public_case_search(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_public_case_search(args)


def handle_qintopia_customer_context_lookup(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_customer_context_lookup(args)


def handle_qintopia_lead_capture(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_lead_capture(args)


def handle_qintopia_proposal_outline_generate(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_proposal_outline_generate(args)


def handle_qintopia_demo_script_generate(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_demo_script_generate(args)


def handle_qintopia_external_disclosure_filter(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_external_disclosure_filter(args)


def handle_qintopia_conversation_summary(args: dict[str, Any], **_: Any) -> str:
    return _configured_operations_intake_plugin().handle_qintopia_conversation_summary(args)


def check_requirements() -> bool:
    index_dir = _index_dir()
    return (index_dir / INDEX_FILES["Public"]).exists()


def check_complaint_requirements() -> bool:
    return _configured_operations_intake_plugin().check_complaint_requirements()


def check_sales_requirements() -> bool:
    return _configured_operations_intake_plugin().check_sales_requirements()


def check_dify_read_requirements() -> bool:
    return bool(_dify_base_url() and _dify_api_key())


def check_message_store_requirements() -> bool:
    return bool(_message_store_database_url())


def check_daily_digest_publish_requirements() -> bool:
    return _daily_digest_publish_enabled()


def check_weather_lookup_requirements() -> bool:
    return True


def check_xiaoman_activity_requirements() -> bool:
    return _xiaoman_activity_wrappers_enabled()


def register(ctx) -> None:
    ctx.register_tool(
        name="qintopia_kb_search",
        toolset="qintopia",
        schema=QINTOPIA_KB_SEARCH_SCHEMA,
        handler=handle_qintopia_kb_search,
        check_fn=check_requirements,
        description=QINTOPIA_KB_SEARCH_SCHEMA["description"],
        emoji="📚",
    )
    ctx.register_tool(
        name="qintopia_gis_location_lookup",
        toolset="qintopia",
        schema=QINTOPIA_GIS_LOCATION_LOOKUP_SCHEMA,
        handler=handle_qintopia_gis_location_lookup,
        check_fn=check_requirements,
        description=QINTOPIA_GIS_LOCATION_LOOKUP_SCHEMA["description"],
        emoji="📍",
    )
    ctx.register_tool(
        name=QINTOPIA_WEATHER_TOOL,
        toolset="qintopia",
        schema=QINTOPIA_WEATHER_LOOKUP_SCHEMA,
        handler=handle_qintopia_weather_lookup,
        check_fn=check_weather_lookup_requirements,
        description=QINTOPIA_WEATHER_LOOKUP_SCHEMA["description"],
        emoji="⛅",
    )
    ctx.register_tool(
        name="qintopia_wenyuange_lookup",
        toolset="qintopia",
        schema=QINTOPIA_WENYUANGE_LOOKUP_SCHEMA,
        handler=handle_qintopia_wenyuange_lookup,
        check_fn=check_dify_read_requirements,
        description=QINTOPIA_WENYUANGE_LOOKUP_SCHEMA["description"],
        emoji="🏛️",
    )
    if _dify_raw_tools_enabled():
        ctx.register_tool(
            name="qintopia_dify_dataset_list",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_DATASET_LIST_SCHEMA,
            handler=handle_qintopia_dify_dataset_list,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_DATASET_LIST_SCHEMA["description"],
            emoji="🗂️",
        )
        ctx.register_tool(
            name="qintopia_dify_dataset_get",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_DATASET_GET_SCHEMA,
            handler=handle_qintopia_dify_dataset_get,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_DATASET_GET_SCHEMA["description"],
            emoji="🗂️",
        )
        ctx.register_tool(
            name="qintopia_dify_knowledge_retrieve",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_KNOWLEDGE_RETRIEVE_SCHEMA,
            handler=handle_qintopia_dify_knowledge_retrieve,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_KNOWLEDGE_RETRIEVE_SCHEMA["description"],
            emoji="🔎",
        )
        ctx.register_tool(
            name="qintopia_dify_document_list",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_DOCUMENT_LIST_SCHEMA,
            handler=handle_qintopia_dify_document_list,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_DOCUMENT_LIST_SCHEMA["description"],
            emoji="📄",
        )
        ctx.register_tool(
            name="qintopia_dify_document_get",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_DOCUMENT_GET_SCHEMA,
            handler=handle_qintopia_dify_document_get,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_DOCUMENT_GET_SCHEMA["description"],
            emoji="📄",
        )
        ctx.register_tool(
            name="qintopia_dify_indexing_status_get",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_INDEXING_STATUS_GET_SCHEMA,
            handler=handle_qintopia_dify_indexing_status_get,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_INDEXING_STATUS_GET_SCHEMA["description"],
            emoji="⏱️",
        )
        ctx.register_tool(
            name="qintopia_dify_segment_list",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_SEGMENT_LIST_SCHEMA,
            handler=handle_qintopia_dify_segment_list,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_SEGMENT_LIST_SCHEMA["description"],
            emoji="🧱",
        )
        ctx.register_tool(
            name="qintopia_dify_segment_get",
            toolset="qintopia",
            schema=QINTOPIA_DIFY_SEGMENT_GET_SCHEMA,
            handler=handle_qintopia_dify_segment_get,
            check_fn=check_dify_read_requirements,
            description=QINTOPIA_DIFY_SEGMENT_GET_SCHEMA["description"],
            emoji="🧱",
        )
    if _message_store_tools_enabled():
        ctx.register_tool(
            name="qintopia_message_store_search",
            toolset="qintopia",
            schema=QINTOPIA_MESSAGE_STORE_SEARCH_SCHEMA,
            handler=handle_qintopia_message_store_search,
            check_fn=check_message_store_requirements,
            description=QINTOPIA_MESSAGE_STORE_SEARCH_SCHEMA["description"],
            emoji="💬",
        )
    ctx.register_tool(
        name="qintopia_complaint_intake_create",
        toolset="qintopia",
        schema=QINTOPIA_COMPLAINT_INTAKE_CREATE_SCHEMA,
        handler=handle_qintopia_complaint_intake_create,
        check_fn=check_complaint_requirements,
        description=QINTOPIA_COMPLAINT_INTAKE_CREATE_SCHEMA["description"],
        emoji="📝",
    )
    ctx.register_tool(
        name="qintopia_complaint_intake_update",
        toolset="qintopia",
        schema=QINTOPIA_COMPLAINT_INTAKE_UPDATE_SCHEMA,
        handler=handle_qintopia_complaint_intake_update,
        check_fn=check_complaint_requirements,
        description=QINTOPIA_COMPLAINT_INTAKE_UPDATE_SCHEMA["description"],
        emoji="➕",
    )
    ctx.register_tool(
        name="qintopia_complaint_followup_send",
        toolset="qintopia",
        schema=QINTOPIA_COMPLAINT_FOLLOWUP_SEND_SCHEMA,
        handler=handle_qintopia_complaint_followup_send,
        check_fn=check_complaint_requirements,
        description=QINTOPIA_COMPLAINT_FOLLOWUP_SEND_SCHEMA["description"],
        emoji="💬",
    )
    ctx.register_tool(
        name="qintopia_external_product_kb_search",
        toolset="qintopia",
        schema=QINTOPIA_EXTERNAL_PRODUCT_KB_SEARCH_SCHEMA,
        handler=handle_qintopia_external_product_kb_search,
        check_fn=check_requirements,
        description=QINTOPIA_EXTERNAL_PRODUCT_KB_SEARCH_SCHEMA["description"],
        emoji="📣",
    )
    ctx.register_tool(
        name="qintopia_public_case_search",
        toolset="qintopia",
        schema=QINTOPIA_PUBLIC_CASE_SEARCH_SCHEMA,
        handler=handle_qintopia_public_case_search,
        check_fn=check_requirements,
        description=QINTOPIA_PUBLIC_CASE_SEARCH_SCHEMA["description"],
        emoji="🧾",
    )
    ctx.register_tool(
        name="qintopia_customer_context_lookup",
        toolset="qintopia",
        schema=QINTOPIA_CUSTOMER_CONTEXT_LOOKUP_SCHEMA,
        handler=handle_qintopia_customer_context_lookup,
        check_fn=check_sales_requirements,
        description=QINTOPIA_CUSTOMER_CONTEXT_LOOKUP_SCHEMA["description"],
        emoji="👤",
    )
    ctx.register_tool(
        name="qintopia_lead_capture",
        toolset="qintopia",
        schema=QINTOPIA_LEAD_CAPTURE_SCHEMA,
        handler=handle_qintopia_lead_capture,
        check_fn=check_sales_requirements,
        description=QINTOPIA_LEAD_CAPTURE_SCHEMA["description"],
        emoji="📌",
    )
    ctx.register_tool(
        name="qintopia_proposal_outline_generate",
        toolset="qintopia",
        schema=QINTOPIA_PROPOSAL_OUTLINE_GENERATE_SCHEMA,
        handler=handle_qintopia_proposal_outline_generate,
        check_fn=check_sales_requirements,
        description=QINTOPIA_PROPOSAL_OUTLINE_GENERATE_SCHEMA["description"],
        emoji="🧩",
    )
    ctx.register_tool(
        name="qintopia_demo_script_generate",
        toolset="qintopia",
        schema=QINTOPIA_DEMO_SCRIPT_GENERATE_SCHEMA,
        handler=handle_qintopia_demo_script_generate,
        check_fn=check_sales_requirements,
        description=QINTOPIA_DEMO_SCRIPT_GENERATE_SCHEMA["description"],
        emoji="🎬",
    )
    ctx.register_tool(
        name="qintopia_external_disclosure_filter",
        toolset="qintopia",
        schema=QINTOPIA_EXTERNAL_DISCLOSURE_FILTER_SCHEMA,
        handler=handle_qintopia_external_disclosure_filter,
        check_fn=check_sales_requirements,
        description=QINTOPIA_EXTERNAL_DISCLOSURE_FILTER_SCHEMA["description"],
        emoji="🛡️",
    )
    ctx.register_tool(
        name="qintopia_conversation_summary",
        toolset="qintopia",
        schema=QINTOPIA_CONVERSATION_SUMMARY_SCHEMA,
        handler=handle_qintopia_conversation_summary,
        check_fn=check_sales_requirements,
        description=QINTOPIA_CONVERSATION_SUMMARY_SCHEMA["description"],
        emoji="🧭",
    )
    ctx.register_tool(
        name=DAILY_DIGEST_PUBLISH_TOOL,
        toolset="qintopia",
        schema=QINTOPIA_DAILY_DIGEST_PUBLISH_SCHEMA,
        handler=handle_qintopia_daily_digest_publish,
        check_fn=check_daily_digest_publish_requirements,
        description=QINTOPIA_DAILY_DIGEST_PUBLISH_SCHEMA["description"],
        emoji="🗞️",
    )
    ctx.register_tool(
        name="qintopia_xiaoman_activity_record_get",
        toolset="qintopia",
        schema=QINTOPIA_XIAOMAN_ACTIVITY_RECORD_GET_SCHEMA,
        handler=handle_qintopia_xiaoman_activity_record_get,
        check_fn=check_xiaoman_activity_requirements,
        description=QINTOPIA_XIAOMAN_ACTIVITY_RECORD_GET_SCHEMA["description"],
        emoji="📋",
    )
    ctx.register_tool(
        name="qintopia_xiaoman_activity_list_by_date",
        toolset="qintopia",
        schema=QINTOPIA_XIAOMAN_ACTIVITY_LIST_BY_DATE_SCHEMA,
        handler=handle_qintopia_xiaoman_activity_list_by_date,
        check_fn=check_xiaoman_activity_requirements,
        description=QINTOPIA_XIAOMAN_ACTIVITY_LIST_BY_DATE_SCHEMA["description"],
        emoji="📅",
    )
    ctx.register_tool(
        name="qintopia_xiaoman_activity_status_update",
        toolset="qintopia",
        schema=QINTOPIA_XIAOMAN_ACTIVITY_STATUS_UPDATE_SCHEMA,
        handler=handle_qintopia_xiaoman_activity_status_update,
        check_fn=check_xiaoman_activity_requirements,
        description=QINTOPIA_XIAOMAN_ACTIVITY_STATUS_UPDATE_SCHEMA["description"],
        emoji="🧭",
    )
    ctx.register_tool(
        name="qintopia_xiaoman_activity_gap_update",
        toolset="qintopia",
        schema=QINTOPIA_XIAOMAN_ACTIVITY_GAP_UPDATE_SCHEMA,
        handler=handle_qintopia_xiaoman_activity_gap_update,
        check_fn=check_xiaoman_activity_requirements,
        description=QINTOPIA_XIAOMAN_ACTIVITY_GAP_UPDATE_SCHEMA["description"],
        emoji="🧩",
    )
    ctx.register_tool(
        name="qintopia_xiaoman_activity_handoff_create",
        toolset="qintopia",
        schema=QINTOPIA_XIAOMAN_ACTIVITY_HANDOFF_CREATE_SCHEMA,
        handler=handle_qintopia_xiaoman_activity_handoff_create,
        check_fn=check_xiaoman_activity_requirements,
        description=QINTOPIA_XIAOMAN_ACTIVITY_HANDOFF_CREATE_SCHEMA["description"],
        emoji="🤝",
    )
    ctx.register_tool(
        name="qintopia_xiaoman_activity_material_summary",
        toolset="qintopia",
        schema=QINTOPIA_XIAOMAN_ACTIVITY_MATERIAL_SUMMARY_SCHEMA,
        handler=handle_qintopia_xiaoman_activity_material_summary,
        check_fn=check_xiaoman_activity_requirements,
        description=QINTOPIA_XIAOMAN_ACTIVITY_MATERIAL_SUMMARY_SCHEMA["description"],
        emoji="🧾",
    )
