"""Qintopia Hermes-native knowledge, GIS, and complaint workflow tools.

Knowledge and GIS tools are read-only. Complaint tools are narrow write-capable
wrappers for Erhua's controlled complaint/service-recovery workflow; they create
or update only complaint_intake cards and leave dispatch with 大总管/default.
"""

from __future__ import annotations

import asyncio
import hashlib
import importlib.util
import json
import os
import re
import textwrap
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
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
_KNOWLEDGE_RETRIEVAL_PLUGIN = None


def _knowledge_retrieval_plugin():
    global _KNOWLEDGE_RETRIEVAL_PLUGIN
    if _KNOWLEDGE_RETRIEVAL_PLUGIN is not None:
        return _KNOWLEDGE_RETRIEVAL_PLUGIN
    plugin_path = Path(__file__).resolve().parents[3] / "knowledge-retrieval" / "__init__.py"
    spec = importlib.util.spec_from_file_location("knowledge_retrieval_plugin", plugin_path)
    if not spec or not spec.loader:
        raise RuntimeError(f"Cannot load knowledge-retrieval skill from {plugin_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    _KNOWLEDGE_RETRIEVAL_PLUGIN = module
    return module


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


QINTOPIA_DIFY_DATASET_LIST_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_DATASET_LIST_SCHEMA
QINTOPIA_DIFY_DATASET_GET_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_DATASET_GET_SCHEMA
QINTOPIA_DIFY_KNOWLEDGE_RETRIEVE_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_KNOWLEDGE_RETRIEVE_SCHEMA
QINTOPIA_DIFY_DOCUMENT_LIST_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_DOCUMENT_LIST_SCHEMA
QINTOPIA_DIFY_DOCUMENT_GET_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_DOCUMENT_GET_SCHEMA
QINTOPIA_DIFY_INDEXING_STATUS_GET_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_INDEXING_STATUS_GET_SCHEMA
QINTOPIA_DIFY_SEGMENT_LIST_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_SEGMENT_LIST_SCHEMA
QINTOPIA_DIFY_SEGMENT_GET_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_DIFY_SEGMENT_GET_SCHEMA
QINTOPIA_WENYUANGE_LOOKUP_SCHEMA = _knowledge_retrieval_plugin().QINTOPIA_WENYUANGE_LOOKUP_SCHEMA



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


QINTOPIA_COMPLAINT_INTAKE_CREATE_SCHEMA = {
    "description": (
        "Create a controlled Qintopia complaint/service-recovery intake card "
        "for 大总管 dispatch. This is the only Kanban-create path available "
        "to Erhua and it always creates task_type=complaint_intake."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "requester_display_name": {
                "type": "string",
                "description": "How the requester is known in the current channel.",
            },
            "requester_channel_user_id": {
                "type": "string",
                "description": (
                    "Channel-scoped requester id. For QiWe this is the webhook "
                    "senderId used as /msg/sendText toId; omitted calls fall "
                    "back to the current Hermes session user id."
                ),
            },
            "source_channel": {
                "type": "string",
                "enum": ["qiwe_group_internal", "qiwe_direct", "wechat_work_internal", "feishu_internal"],
                "description": "Where the complaint was received.",
            },
            "source_conversation_id": {
                "type": "string",
                "description": "Channel conversation id for audit and follow-up.",
            },
            "source_message_id": {
                "type": "string",
                "description": "Original message id, if available.",
            },
            "source_message_url": {
                "type": "string",
                "description": "Original message URL, if available.",
            },
            "original_message": {
                "type": "string",
                "description": "Requester complaint/feedback text.",
            },
            "complaint_summary": {
                "type": "string",
                "description": "Short Chinese summary. Defaults to a safe summary of original_message.",
            },
            "priority": {
                "type": "integer",
                "minimum": 0,
                "maximum": 3,
                "description": "Hermes priority. Defaults to 1 for service recovery.",
            },
            "idempotency_key": {
                "type": "string",
                "description": "Optional webhook idempotency key.",
            },
        },
        "required": ["source_channel", "source_conversation_id", "original_message"],
        "additionalProperties": False,
    },
}


QINTOPIA_COMPLAINT_INTAKE_UPDATE_SCHEMA = {
    "description": (
        "Append requester-provided details to an existing complaint_intake card. "
        "Does not change owner or assign an executor."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "task_id": {
                "type": "string",
                "description": "Existing complaint_intake Kanban task id.",
            },
            "requester_display_name": {
                "type": "string",
                "description": "Requester display name, if available.",
            },
            "details": {
                "type": "string",
                "description": "New details provided by the complainant.",
            },
            "occurred_at": {
                "type": "string",
                "description": "When the issue happened, if provided.",
            },
            "location_or_area": {
                "type": "string",
                "description": "Where the issue happened, if provided.",
            },
            "already_contacted": {
                "type": "string",
                "description": "Whether the requester contacted staff/front desk.",
            },
            "expected_resolution": {
                "type": "string",
                "description": "Requested resolution or reply expectation.",
            },
        },
        "required": ["task_id", "details"],
        "additionalProperties": False,
    },
}


QINTOPIA_COMPLAINT_FOLLOWUP_SEND_SCHEMA = {
    "description": (
        "Prepare an approved private follow-up message to a complainant after "
        "a complaint_intake card is completed/reviewed. The caller must provide "
        "the approved resolution text."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "task_id": {
                "type": "string",
                "description": "Completed complaint_intake Kanban task id.",
            },
            "requester_channel_user_id": {
                "type": "string",
                "description": "Channel-scoped requester id for private follow-up.",
            },
            "requester_display_name": {
                "type": "string",
                "description": "Requester display name, if available.",
            },
            "approved_resolution": {
                "type": "string",
                "description": "大总管/负责人批准可对外发送的处理结果。",
            },
            "followup_question": {
                "type": "string",
                "description": "Optional short follow-up question.",
            },
        },
        "required": ["task_id", "requester_channel_user_id", "approved_resolution"],
        "additionalProperties": False,
    },
}


QINTOPIA_EXTERNAL_PRODUCT_KB_SEARCH_SCHEMA = {
    "description": (
        "Search only Public Qintopia Agent OS product knowledge for Xiaoqin. "
        "Returns approved baseline statements when the Public KB has no useful match."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Customer-facing product question."},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_KB_LIMIT},
            "purpose": {"type": "string", "description": "Why Xiaoqin needs this answer."},
        },
        "required": ["query"],
        "additionalProperties": False,
    },
}


QINTOPIA_PUBLIC_CASE_SEARCH_SCHEMA = {
    "description": (
        "Search approved Public case/demo references for Xiaoqin. If no approved "
        "case exists, returns a safe escalation result instead of inventing one."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Case, customer, demo, or pilot query."},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_KB_LIMIT},
        },
        "required": ["query"],
        "additionalProperties": False,
    },
}


QINTOPIA_CUSTOMER_CONTEXT_LOOKUP_SCHEMA = {
    "description": (
        "Return only current-channel customer context for Xiaoqin. This is not a "
        "CRM lookup and does not expose other customers or internal records."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "customer_display_name": {"type": "string"},
            "source_channel": {"type": "string"},
            "source_conversation_id": {"type": "string"},
            "source_message_id": {"type": "string"},
            "customer_provided_context": {"type": "string"},
        },
        "additionalProperties": False,
    },
}


QINTOPIA_LEAD_CAPTURE_SCHEMA = {
    "description": (
        "Create a controlled Xiaoqin sales/customer Kanban card. It can create "
        "only sales_lead, demo_request, proposal, or external_disclosure_review."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "task_type": {
                "type": "string",
                "enum": list(SALES_TASK_TYPES.keys()),
                "description": "Controlled sales task type. Defaults to sales_lead.",
            },
            "customer_display_name": {"type": "string"},
            "source_channel": {
                "type": "string",
                "enum": ["worktool_external_contact", "worktool_external_group", "wechat_external", "wecom_external", "feishu_external", "manual"],
            },
            "source_conversation_id": {"type": "string"},
            "source_message_id": {"type": "string"},
            "source_message_url": {"type": "string"},
            "customer_request": {"type": "string"},
            "business_scenario": {"type": "string"},
            "budget_range": {"type": "string"},
            "urgency": {"type": "string"},
            "current_system": {"type": "string"},
            "pilot_expectation": {"type": "string"},
            "next_step": {"type": "string"},
            "public_answer_given": {"type": "string"},
            "boundary_trigger": {"type": "string"},
            "idempotency_key": {"type": "string"},
            "priority": {"type": "integer", "minimum": 0, "maximum": 3},
        },
        "required": ["source_channel", "source_conversation_id", "customer_request"],
        "additionalProperties": False,
    },
}


QINTOPIA_PROPOSAL_OUTLINE_GENERATE_SCHEMA = {
    "description": (
        "Generate a safe proposal outline draft for Xiaoqin. The output is a draft "
        "and does not include binding price, schedule, scope, SLA, or contract terms."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "customer_display_name": {"type": "string"},
            "business_scenario": {"type": "string"},
            "goals": {"type": "string"},
            "current_system": {"type": "string"},
            "constraints": {"type": "string"},
            "pilot_expectation": {"type": "string"},
            "public_knowledge_summary": {"type": "string"},
        },
        "required": ["business_scenario"],
        "additionalProperties": False,
    },
}


QINTOPIA_DEMO_SCRIPT_GENERATE_SCHEMA = {
    "description": (
        "Generate a low-risk demo script for public samples, redacted materials, "
        "or customer-provided materials explicitly allowed for demo use."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "customer_display_name": {"type": "string"},
            "demo_goal": {"type": "string"},
            "business_scenario": {"type": "string"},
            "allowed_materials": {"type": "string"},
            "timebox_minutes": {"type": "integer", "minimum": 5, "maximum": 120},
        },
        "required": ["demo_goal", "business_scenario"],
        "additionalProperties": False,
    },
}


QINTOPIA_EXTERNAL_DISCLOSURE_FILTER_SCHEMA = {
    "description": (
        "Filter a Xiaoqin draft for external disclosure. Returns public-safe text, "
        "internal-only notes, and whether team review is required."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "draft_answer": {"type": "string"},
            "recipient": {"type": "string"},
            "purpose": {"type": "string"},
        },
        "required": ["draft_answer", "purpose"],
        "additionalProperties": False,
    },
}


QINTOPIA_CONVERSATION_SUMMARY_SCHEMA = {
    "description": (
        "Summarize a Xiaoqin customer conversation into a safe handoff format."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "conversation_text": {"type": "string"},
            "customer_display_name": {"type": "string"},
            "source_channel": {"type": "string"},
            "public_answer_given": {"type": "string"},
            "boundary_trigger": {"type": "string"},
        },
        "required": ["conversation_text"],
        "additionalProperties": False,
    },
}


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
    return _knowledge_retrieval_plugin().handle_qintopia_dify_dataset_list(args)


def handle_qintopia_dify_dataset_get(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_dify_dataset_get(args)


def handle_qintopia_dify_knowledge_retrieve(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_dify_knowledge_retrieve(args)


def handle_qintopia_dify_document_list(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_dify_document_list(args)


def handle_qintopia_dify_document_get(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_dify_document_get(args)


def handle_qintopia_dify_indexing_status_get(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_dify_indexing_status_get(args)


def handle_qintopia_dify_segment_list(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_dify_segment_list(args)


def handle_qintopia_dify_segment_get(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_dify_segment_get(args)


def handle_qintopia_wenyuange_lookup(args: dict[str, Any], **_: Any) -> str:
    return _knowledge_retrieval_plugin().handle_qintopia_wenyuange_lookup(args)



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
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    original_message = _body_text(args.get("original_message"))
    if source_channel not in {"qiwe_group_internal", "qiwe_direct", "wechat_work_internal", "feishu_internal"}:
        return _json({"success": False, "error": "source_channel is not allowed for complaint intake"})
    if not conversation_id:
        return _json({"success": False, "error": "source_conversation_id is required"})
    if not original_message:
        return _json({"success": False, "error": "original_message is required"})

    summary = _clean_text(args.get("complaint_summary"), max_len=160)
    if not summary:
        summary = _clean_text(original_message, max_len=80) or "投诉/反馈待分诊"
    priority = args.get("priority")
    try:
        priority_value = int(priority) if priority is not None else 1
    except (TypeError, ValueError):
        return _json({"success": False, "error": "priority must be an integer"})
    priority_value = min(max(priority_value, 0), 3)

    title = _complaint_task_title(summary)
    body = _complaint_task_body(args, summary)
    idempotency_key = _complaint_idempotency_key(args)
    task_id, status = _kanban_create_complaint(title, body, priority_value, idempotency_key)
    requester_name = _clean_text(args.get("requester_display_name"), max_len=80)
    requester_id = _resolve_requester_channel_user_id(args, source_channel)

    kanban_action = {
        "action": "kanban_task_create_request",
        "board": COMPLAINT_BOARD,
        "tenant": QINTOPIA_TENANT,
        "title": title,
        "body": body,
        "assignee": COMPLAINT_OWNER_PROFILE,
        "owner_profile": COMPLAINT_OWNER_PROFILE,
        "task_type": COMPLAINT_TASK_TYPE,
        "status": "triage",
        "priority": priority_value,
        "information_class": "Member-scoped",
        "risk_level": "P1",
        "human_approval_requirement": "大总管确认处理结果后才能对投诉人回访",
        "idempotency_key": idempotency_key,
    }
    actions: list[dict[str, Any]] = [kanban_action]
    if requester_id:
        actions.append(
            {
                "tool": "qiwe_send_direct_message",
                "recipient_user_id": requester_id,
                "recipient_display_name": requester_name,
                "conversation_scope": "private",
                "message": _private_detail_request_message(requester_name),
                "purpose": "complaint_intake_detail_collection",
                "idempotency_key": f"{idempotency_key}:direct:intake",
            }
        )

    return _json(
        {
            "success": True,
            "skill": "qintopia_complaint_intake_create",
            "mode": status,
            "task_id": task_id,
            "task_type": COMPLAINT_TASK_TYPE,
            "owner_profile": COMPLAINT_OWNER_PROFILE,
            "board": COMPLAINT_BOARD,
            "tenant": QINTOPIA_TENANT,
            "followup_status": "need_private_details",
            "requester_channel_user_id_resolved": bool(requester_id),
            "actions": actions,
            "guardrails": [
                "only complaint_intake may be created",
                "dispatch owner remains default/大总管",
                "do not assign executor from Erhua",
                "collect details in private conversation",
                "do not promise compensation, punishment, refund, deadline, or final result",
            ],
        }
    )


def handle_qintopia_complaint_intake_update(args: dict[str, Any], **_: Any) -> str:
    task_id = _clean_text(args.get("task_id"), max_len=80)
    details = _body_text(args.get("details"))
    if not task_id:
        return _json({"success": False, "error": "task_id is required"})
    if not details:
        return _json({"success": False, "error": "details is required"})

    requester = _clean_text(args.get("requester_display_name"), max_len=80) or "投诉人"
    fields = [
        ("补充人", requester),
        ("发生时间", _clean_text(args.get("occurred_at"), max_len=120)),
        ("地点/区域", _clean_text(args.get("location_or_area"), max_len=120)),
        ("已联系人员", _clean_text(args.get("already_contacted"), max_len=240)),
        ("期望处理", _clean_text(args.get("expected_resolution"), max_len=300)),
    ]
    field_lines = [f"- {label}：{value}" for label, value in fields if value]
    comment = "\n".join(
        [
            "## 二花补充的投诉详情",
            "",
            *field_lines,
            "",
            "### 新增说明",
            "",
            details,
        ]
    ).strip()
    comment_id, status = _kanban_add_complaint_comment(task_id, comment)

    return _json(
        {
            "success": True,
            "skill": "qintopia_complaint_intake_update",
            "mode": status,
            "task_id": task_id,
            "comment_id": comment_id,
            "task_type": COMPLAINT_TASK_TYPE,
            "actions": [
                {
                    "action": "kanban_comment_add_request",
                    "board": COMPLAINT_BOARD,
                    "task_id": task_id,
                    "author": "erhua",
                    "body": comment,
                    "does_not_assign_executor": True,
                }
            ],
            "guardrails": [
                "append details to existing complaint card only",
                "do not change owner or assignee",
                "do not publish private details to group chat",
            ],
        }
    )


def handle_qintopia_complaint_followup_send(args: dict[str, Any], **_: Any) -> str:
    task_id = _clean_text(args.get("task_id"), max_len=80)
    requester_id = _clean_text(args.get("requester_channel_user_id"), max_len=160)
    approved_resolution = _body_text(args.get("approved_resolution"), max_len=2000)
    if not task_id:
        return _json({"success": False, "error": "task_id is required"})
    if not requester_id:
        return _json({"success": False, "error": "requester_channel_user_id is required"})
    if not approved_resolution:
        return _json({"success": False, "error": "approved_resolution is required"})

    requester_name = _clean_text(args.get("requester_display_name"), max_len=80)
    prefix = f"{requester_name}，" if requester_name else ""
    followup_question = (
        _clean_text(args.get("followup_question"), max_len=200)
        or "你看这个处理结果是否解决了你的问题？如果还需要继续跟进，可以直接告诉我。"
    )
    message = f"{prefix}这件投诉/反馈已经有处理结果了：\n\n{approved_resolution}\n\n{followup_question}"
    comment_id, status = _kanban_add_complaint_comment(
        task_id,
        "## 二花回访记录\n\n已根据批准口径私聊投诉人同步处理结果，并询问是否需要继续跟进。",
    )
    idempotency_key = _complaint_followup_idempotency_key(task_id, requester_id, approved_resolution)

    return _json(
        {
            "success": True,
            "skill": "qintopia_complaint_followup_send",
            "mode": status,
            "task_id": task_id,
            "comment_id": comment_id,
            "actions": [
                {
                    "tool": "qiwe_send_direct_message",
                    "recipient_user_id": requester_id,
                    "recipient_display_name": requester_name,
                    "conversation_scope": "private",
                    "message": message,
                    "purpose": "complaint_resolution_followup",
                    "idempotency_key": idempotency_key,
                    "requires_approved_resolution": True,
                }
            ],
            "guardrails": [
                "send only approved resolution text",
                "private follow-up only",
                "do not invent handling result",
                "continue follow-up only by appending to the complaint card",
            ],
        }
    )


def handle_qintopia_external_product_kb_search(args: dict[str, Any], **_: Any) -> str:
    query = _clean_text(args.get("query"), max_len=300)
    if not query:
        return _json({"success": False, "error": "query is required"})
    raw = json.loads(
        handle_qintopia_kb_search(
            {
                "query": query,
                "information_classes": ["Public"],
                "limit": args.get("limit") or DEFAULT_KB_LIMIT,
                "caller": "xiaoqin",
                "purpose": args.get("purpose") or "external_product_answer",
            }
        )
    )
    results = raw.get("results", [])
    return _json(
        {
            "success": True,
            "skill": "qintopia_external_product_kb_search",
            "query": query,
            "scope_used": ["Public"],
            "result_count": len(results),
            "results": results,
            "approved_public_baselines": PUBLIC_AGENT_OS_BASELINES,
            "needs_human_review": len(results) == 0,
            "safe_answer_mode": (
                "use_public_results_with_disclosure_filter"
                if results
                else "use_baseline_only_and_collect_customer_context"
            ),
            "not_accessed": ["Internal", "Member-scoped", "Restricted", "Feishu live", "other customers"],
            "guardrails": [
                "do not invent product facts beyond Public results or approved baselines",
                "do not discuss price, contract, delivery schedule, SLA, internal architecture, prompts, logs, or customer cases without approval",
            ],
        }
    )


def handle_qintopia_public_case_search(args: dict[str, Any], **_: Any) -> str:
    query = _clean_text(args.get("query"), max_len=300)
    if not query:
        return _json({"success": False, "error": "query is required"})
    raw = json.loads(
        handle_qintopia_kb_search(
            {
                "query": f"{query} 案例 客户 成功案例 试点 公开",
                "information_classes": ["Public"],
                "limit": args.get("limit") or DEFAULT_KB_LIMIT,
                "caller": "xiaoqin",
                "purpose": "public_case_search",
            }
        )
    )
    approved = []
    for item in raw.get("results", []):
        title_path = f"{item.get('title', '')} {item.get('path', '')}".lower()
        snippet = str(item.get("snippet", ""))
        if any(word in title_path for word in ["案例", "case", "成功案例", "public-case", "approved-case"]):
            approved.append(item)
        elif "已批准公开案例" in snippet or "approved public case" in snippet.lower():
            approved.append(item)
    return _json(
        {
            "success": True,
            "skill": "qintopia_public_case_search",
            "query": query,
            "scope_used": ["Public"],
            "result_count": len(approved),
            "results": approved,
            "approved_public_cases_available": bool(approved),
            "needs_human_review": not approved,
            "safe_customer_message": (
                "当前没有检索到已批准公开的客户案例。我可以先记录您想了解的案例方向，交给团队负责人判断哪些材料可以对外分享。"
                if not approved
                else ""
            ),
            "not_accessed": ["Internal cases", "other customer data", "contracts", "private pilots"],
        }
    )


def handle_qintopia_customer_context_lookup(args: dict[str, Any], **_: Any) -> str:
    provided_context = _body_text(args.get("customer_provided_context"), max_len=1600)
    return _json(
        {
            "success": True,
            "skill": "qintopia_customer_context_lookup",
            "mode": "current_channel_context_only",
            "customer": {
                "display_name": _clean_text(args.get("customer_display_name"), max_len=120) or "未提供",
                "source_channel": _clean_text(args.get("source_channel"), max_len=80) or "未提供",
                "source_conversation_id": _clean_text(args.get("source_conversation_id"), max_len=160) or "未提供",
                "source_message_id": _clean_text(args.get("source_message_id"), max_len=160) or "未提供",
            },
            "customer_provided_context": provided_context,
            "stored_context_found": False,
            "not_accessed": ["CRM", "other customer records", "member profiles", "private chat history"],
            "guardrails": [
                "use only current conversation and customer-provided context",
                "ask for permission before using customer-provided material in a demo",
            ],
        }
    )


def handle_qintopia_lead_capture(args: dict[str, Any], **_: Any) -> str:
    task_type = _clean_text(args.get("task_type") or "sales_lead", max_len=80)
    if task_type not in SALES_TASK_TYPES:
        return _json({"success": False, "error": "task_type is not allowed for Xiaoqin lead capture"})
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    customer_request = _body_text(args.get("customer_request"), max_len=2400)
    if not source_channel:
        return _json({"success": False, "error": "source_channel is required"})
    if not conversation_id:
        return _json({"success": False, "error": "source_conversation_id is required"})
    if not customer_request:
        return _json({"success": False, "error": "customer_request is required"})
    try:
        priority = int(args.get("priority") if args.get("priority") is not None else 1)
    except (TypeError, ValueError):
        return _json({"success": False, "error": "priority must be an integer"})
    priority = min(max(priority, 0), 3)
    customer = _clean_text(args.get("customer_display_name"), max_len=120)
    title = _sales_task_title(task_type, customer, customer_request)
    body = _sales_task_body(args, task_type)
    idempotency_key = _sales_idempotency_key(args)
    task_id, status = _kanban_create_sales_task(title, body, task_type, priority, idempotency_key)
    assignee = "default" if task_type == "external_disclosure_review" else SALES_OWNER_PROFILE

    return _json(
        {
            "success": True,
            "skill": "qintopia_lead_capture",
            "mode": status,
            "task_id": task_id,
            "task_type": task_type,
            "board": SALES_BOARD,
            "tenant": QINTOPIA_TENANT,
            "owner_profile": assignee,
            "safe_customer_message": XIAOQIN_SAFE_FOLLOWUP_MESSAGE,
            "customer_response_policy": [
                "Use safe_customer_message for the customer-facing reply after this tool call.",
                "Do not add internal execution details to the customer-facing reply.",
            ],
            "actions": [
                {
                    "action": "kanban_task_create_request",
                    "board": SALES_BOARD,
                    "tenant": QINTOPIA_TENANT,
                    "title": title,
                    "body": body,
                    "assignee": assignee,
                    "owner_profile": assignee,
                    "task_type": task_type,
                    "status": SALES_TASK_TYPES[task_type]["status"],
                    "priority": priority,
                    "information_class": "Public" if task_type in {"sales_lead", "demo_request"} else "Internal",
                    "risk_level": "P1" if task_type == "external_disclosure_review" else "P2",
                    "human_approval_requirement": "团队负责人需确认报价、合同、交付、SLA、内部披露或客户案例细节",
                    "idempotency_key": idempotency_key,
                }
            ],
            "guardrails": [
                "only controlled sales task types may be created",
                "do not create binding price, contract, delivery, schedule, or SLA commitments",
                "external_disclosure_review is routed to default/team review",
            ],
        }
    )


def handle_qintopia_proposal_outline_generate(args: dict[str, Any], **_: Any) -> str:
    scenario = _body_text(args.get("business_scenario"), max_len=1200)
    if not scenario:
        return _json({"success": False, "error": "business_scenario is required"})
    customer = _clean_text(args.get("customer_display_name"), max_len=120) or "客户"
    outline = textwrap.dedent(
        f"""
        # Qintopia Agent OS 方案草案

        ## 客户与场景

        - 客户：{customer}
        - 场景：{scenario}

        ## 目标

        {_body_text(args.get("goals"), max_len=1000) or "待客户进一步确认业务目标、成功指标和试点范围。"}

        ## 初步思路

        - 用 Agent OS 承接需求收集、公开知识问答、方案草拟、任务交接和人工审批。
        - 先使用公开样例、脱敏材料或客户明确允许用于演示的材料做低风险试点。
        - 将需要人工判断的报价、合同、交付范围、排期和 SLA 交给团队负责人。

        ## 当前系统与约束

        - 现有系统：{_body_text(args.get("current_system"), max_len=800) or "待补充"}
        - 约束：{_body_text(args.get("constraints"), max_len=800) or "待补充"}
        - 试点期望：{_body_text(args.get("pilot_expectation"), max_len=800) or "待补充"}

        ## 需要确认

        - 可用于演示或试点的数据范围。
        - 是否涉及内部资料、客户案例、报价、合同或交付承诺。
        - 下一步会议时间和团队负责人参与方式。
        """
    ).strip()
    return _json(
        {
            "success": True,
            "skill": "qintopia_proposal_outline_generate",
            "draft": outline,
            "requires_disclosure_filter": True,
            "requires_human_review_before_external_send": True,
            "guardrails": [
                "draft only",
                "no binding price, contract, delivery schedule, scope, SLA, or customer case claims",
                "run qintopia_external_disclosure_filter before sending externally",
            ],
        }
    )


def handle_qintopia_demo_script_generate(args: dict[str, Any], **_: Any) -> str:
    demo_goal = _body_text(args.get("demo_goal"), max_len=800)
    scenario = _body_text(args.get("business_scenario"), max_len=1000)
    if not demo_goal:
        return _json({"success": False, "error": "demo_goal is required"})
    if not scenario:
        return _json({"success": False, "error": "business_scenario is required"})
    timebox = args.get("timebox_minutes")
    try:
        minutes = int(timebox) if timebox is not None else 30
    except (TypeError, ValueError):
        return _json({"success": False, "error": "timebox_minutes must be an integer"})
    minutes = min(max(minutes, 5), 120)
    allowed_materials = _body_text(args.get("allowed_materials"), max_len=1000) or "公开样例或客户明确允许用于演示的材料"
    script = textwrap.dedent(
        f"""
        # 小秦演示脚本草案

        ## 演示目标

        {demo_goal}

        ## 场景设定

        {scenario}

        ## 可用材料

        {allowed_materials}

        ## {minutes} 分钟流程

        1. 开场确认：说明本次演示只使用公开样例、脱敏材料或客户授权材料。
        2. 需求收集：让客户描述目标、现有流程、预算范围、紧急程度和试点期望。
        3. 公开知识问答：只回答 Public-safe 产品能力和流程边界。
        4. 方案草拟：生成一份标注“草案/需审核”的方案结构。
        5. 任务交接：展示如何形成商机、演示准备、方案草案或披露审核任务。
        6. 收尾升级：报价、合同、交付范围、排期、SLA、客户案例交给团队负责人。
        """
    ).strip()
    return _json(
        {
            "success": True,
            "skill": "qintopia_demo_script_generate",
            "script": script,
            "requires_human_review_before_external_send": True,
            "guardrails": [
                "do not use real member records, other customer data, internal logs, prompts, or server details",
                "mark demo output as draft/needs review",
                "do not promise price, schedule, delivery result, or SLA",
            ],
        }
    )


def handle_qintopia_external_disclosure_filter(args: dict[str, Any], **_: Any) -> str:
    draft = _body_text(args.get("draft_answer"), max_len=5000)
    if not draft:
        return _json({"success": False, "error": "draft_answer is required"})
    purpose = _clean_text(args.get("purpose"), max_len=300)
    hits = _disclosure_hits(draft)
    approval_required = bool(hits)
    public_safe = draft
    internal_notes: list[str] = []
    if approval_required:
        internal_notes.append("草稿命中敏感披露关键词，不能直接发送给外部客户。")
        public_safe = (
            "这部分涉及需要进一步确认的信息，我不能直接对外确认。"
            "我可以先记录您的问题和背景，交给团队负责人判断哪些内容可以公开说明。"
        )
    return _json(
        {
            "success": True,
            "skill": "qintopia_external_disclosure_filter",
            "recipient": _clean_text(args.get("recipient"), max_len=120) or "external_customer",
            "purpose": purpose,
            "approval_required": approval_required,
            "public_safe_draft": public_safe,
            "internal_only_notes": internal_notes,
            "matched_risk_categories": hits,
            "blocked_topics": list(hits.keys()),
            "guardrails": [
                "send public_safe_draft only",
                "create external_disclosure_review if approval_required is true",
                "do not disclose internal notes externally",
            ],
        }
    )


def handle_qintopia_conversation_summary(args: dict[str, Any], **_: Any) -> str:
    conversation = _body_text(args.get("conversation_text"), max_len=6000)
    if not conversation:
        return _json({"success": False, "error": "conversation_text is required"})
    customer = _clean_text(args.get("customer_display_name"), max_len=120) or "未提供"
    source_channel = _clean_text(args.get("source_channel"), max_len=80) or "未提供"
    hits = _disclosure_hits(conversation)
    summary = textwrap.dedent(
        f"""
        ## 客户与来源

        - 客户：{customer}
        - 来源渠道：{source_channel}

        ## 客户诉求

        {_clean_text(conversation, max_len=800)}

        ## 已公开回答

        {_body_text(args.get("public_answer_given"), max_len=1200) or "待补充"}

        ## 触发边界

        {_body_text(args.get("boundary_trigger"), max_len=1000) or ("命中需审核主题：" + ", ".join(hits.keys()) if hits else "未发现明确敏感边界")}

        ## 需要团队负责人决策

        {"需要审核后再对外确认。" if hits else "如客户继续追问报价、合同、交付、SLA、客户案例或内部信息，需要团队负责人确认。"}

        ## 建议下一步

        继续确认业务场景、预算范围、紧急程度、现有系统、试点期望和下一步会议安排。
        """
    ).strip()
    return _json(
        {
            "success": True,
            "skill": "qintopia_conversation_summary",
            "summary": summary,
            "matched_risk_categories": hits,
            "suggested_task_type": "external_disclosure_review" if hits else "sales_lead",
            "guardrails": [
                "summary is for handoff and Kanban only",
                "do not include private data not provided in the current conversation",
            ],
        }
    )


def check_requirements() -> bool:
    index_dir = _index_dir()
    return (index_dir / INDEX_FILES["Public"]).exists()


def check_complaint_requirements() -> bool:
    return True


def check_sales_requirements() -> bool:
    return True


def check_dify_read_requirements() -> bool:
    return _knowledge_retrieval_plugin().check_dify_read_requirements()


def check_message_store_requirements() -> bool:
    return bool(_message_store_database_url())


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
