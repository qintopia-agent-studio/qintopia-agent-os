"""Public-safe Qintopia knowledge retrieval capability.

This package owns Dify Knowledge read wrappers and the WenYuanGe-filtered lookup
surface used by frontline profiles. It does not own Postgres message-store search.
"""

from __future__ import annotations

import json
import os
import re
from typing import Any
from urllib import error as urlerror
from urllib import request as urlrequest
from urllib.parse import quote, urlencode


DEFAULT_DIFY_KB_BASE_URL = "https://qintopia.cn/remote/v1"
DEFAULT_DIFY_LIMIT = 20
MAX_DIFY_LIMIT = 100
DEFAULT_DIFY_TIMEOUT_SECONDS = 10


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


XIAOQIN_LOOKUP_RISK_PATTERNS = {
    "member_scoped": ["成员资料", "村民档案", "成员档案", "入住", "房型"],
    "personal_data": ["手机号", "身份证", "生日", "联系方式", "微信号"],
    "internal_information": ["内部", "未公开", "服务器", "日志", "prompt", "提示词"],
    "commercial_commitment": ["报价", "合同", "sla", "交付承诺", "排期"],
    "credentials": ["token", "secret", "password", "密钥", "凭证"],
}


ERHUA_LOOKUP_RISK_PATTERNS = {
    "member_privacy": [
        "手机号",
        "身份证",
        "生日",
        "房间",
        "入住时间",
        "成员档案",
        "隐藏画像",
        "私密历史",
        "村民档案",
    ],
    "complaint_or_service_recovery": [
        "投诉",
        "服务不满",
        "不满意",
        "入住体验不好",
        "客服介入",
        "反馈处理",
    ],
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


def _lookup_safe_reply_guidance(can_answer: bool, caller_profile: str) -> str:
    if can_answer:
        return "可以基于 answer_basis 组织回复；不要提及内部来源、工具名或 Dify。"
    if caller_profile == "xiaoqin":
        return "不要直接回答；请记录需求，并通过商机跟进或披露审核交给团队负责人确认。"
    return "不要直接回答；请转人工/负责人确认。投诉或服务不满场景继续使用受控投诉受理流程。"


def _tokenize(value: str) -> list[str]:
    return [
        part.lower()
        for part in re.split(r"[\s,.;:!?，。；：！？、/\\|()（）\[\]【】\"']+", value)
        if part.strip()
    ]


def _lookup_risk_flags(caller_profile: str, audience: str, text: str) -> list[str]:
    patterns = (
        XIAOQIN_LOOKUP_RISK_PATTERNS
        if (caller_profile, audience) == ("xiaoqin", "external_customer")
        else ERHUA_LOOKUP_RISK_PATTERNS
    )
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


def _record_query_score(record: Any, query: str) -> tuple[int, int, float]:
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


def check_dify_read_requirements() -> bool:
    return bool(_dify_base_url() and _dify_api_key())
