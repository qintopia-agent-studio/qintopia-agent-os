from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import logging
import os
import re
import shlex
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Tuple

try:
    from aiohttp import ClientSession, ClientTimeout, web

    AIOHTTP_AVAILABLE = True
except ImportError:  # pragma: no cover - exercised by Hermes check_fn in prod
    ClientSession = None  # type: ignore[assignment]
    ClientTimeout = None  # type: ignore[assignment]
    web = None  # type: ignore[assignment]
    AIOHTTP_AVAILABLE = False

try:
    from gateway.config import Platform
    from gateway.platforms.base import (
        BasePlatformAdapter,
        MessageEvent,
        MessageType,
        SendResult,
    )
except ImportError:  # pragma: no cover - local parser tests run outside Hermes
    from dataclasses import dataclass as _dataclass
    from enum import Enum

    class Platform(str, Enum):
        QIWE = "qiwe"

    class MessageType(Enum):
        TEXT = "text"

    @_dataclass
    class SendResult:
        success: bool
        message_id: Optional[str] = None
        error: Optional[str] = None
        raw_response: Any = None
        retryable: bool = False

    @_dataclass
    class MessageEvent:
        text: str
        message_type: MessageType = MessageType.TEXT
        source: Any = None
        raw_message: Any = None
        message_id: Optional[str] = None
        channel_prompt: Optional[str] = None

    class BasePlatformAdapter:
        def __init__(self, config: Any, platform: Platform):
            self.config = config
            self.platform = platform

        def _mark_connected(self) -> None:
            return None

        def _mark_disconnected(self) -> None:
            return None

        def _set_fatal_error(self, code: str, message: str, *, retryable: bool) -> None:
            return None

        def build_source(self, **kwargs: Any) -> Dict[str, Any]:
            return kwargs

        async def handle_message(self, event: MessageEvent) -> None:
            return None

try:
    from .passive_pipeline import PassiveEventPipeline, PassivePipelineConfig
    from .nats_capture import (
        QiWeNatsCaptureConfig,
        QiWeNatsPublisher,
        build_capture_events,
    )
    from .qiwe_events import normalized_event_from_parsed
    from .solitaire.llm_parser import parser_from_context
    from .solitaire.reminder import ReminderWorker, ReminderWorkerConfig
except ImportError:  # pragma: no cover - local tests import adapter.py directly
    from nats_capture import (
        QiWeNatsCaptureConfig,
        QiWeNatsPublisher,
        build_capture_events,
    )
    from passive_pipeline import PassiveEventPipeline, PassivePipelineConfig
    from qiwe_events import normalized_event_from_parsed
    from solitaire.llm_parser import parser_from_context
    from solitaire.reminder import ReminderWorker, ReminderWorkerConfig

logger = logging.getLogger(__name__)

DEFAULT_API_URL = "http://manager.qiweapi.com/qiwe/api/qw/doApi"
DEFAULT_WEBHOOK_HOST = "127.0.0.1"
DEFAULT_WEBHOOK_PORT = 18661
DEFAULT_WEBHOOK_PATH = "/qiwe/webhook"
DEFAULT_MAX_BODY_BYTES = 1_048_576
DEFAULT_MAX_REPLY_CHARS = 3500
DEFAULT_DEDUPE_TTL_SECONDS = 600
DEFAULT_LOCATION_TOOL_DEDUPE_TTL_SECONDS = 300
DEFAULT_DIRECT_TOOL_DEDUPE_TTL_SECONDS = 300
DEFAULT_RICH_MESSAGE_TOOL_DEDUPE_TTL_SECONDS = 300
DEFAULT_REVOKE_MESSAGE_TOOL_DEDUPE_TTL_SECONDS = 300
DEFAULT_VOICE_TO_TEXT_TOOL_DEDUPE_TTL_SECONDS = 300
DEFAULT_HUMAN_HANDOFF_TOOL_DEDUPE_TTL_SECONDS = 300
DEFAULT_CONTACT_REQUEST_TOOL_DEDUPE_TTL_SECONDS = 86_400
DEFAULT_CONTACT_GUARD_CACHE_TTL_SECONDS = 300
DEFAULT_CONTACT_GUARD_PAGE_LIMIT = 100
DEFAULT_CONTACT_GUARD_MAX_PAGES = 20
DEFAULT_IDENTITY_CACHE_TTL_SECONDS = 86_400
DEFAULT_RECENT_MESSAGE_REF_TTL_SECONDS = 600
DEFAULT_ANSWER_CONTEXT_MCP_COMMAND = "/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/hermes/qintopia-context-mcp"
QIWE_NORMAL_FRIEND_CONTACT_TYPE = 2057
MENTION_SPACES = "\u00a0\u2005\u200b"
MENTION_BOUNDARY_PUNCT = ":：,，、。.!！?？~～…"
INTERNAL_PROCESS_SKIP_REASON = "internal_process_message"
_LOCATION_TOOL_SEEN: Dict[str, float] = {}
_DIRECT_TOOL_SEEN: Dict[str, float] = {}
_RICH_MESSAGE_TOOL_SEEN: Dict[str, float] = {}
_REVOKE_MESSAGE_TOOL_SEEN: Dict[str, float] = {}
_VOICE_TO_TEXT_TOOL_SEEN: Dict[str, float] = {}
_HUMAN_HANDOFF_TOOL_SEEN: Dict[str, float] = {}
_CONTACT_REQUEST_TOOL_SEEN: Dict[str, float] = {}
_RECENT_QIWE_MESSAGE_REFS: Dict[Tuple[str, str], Tuple[float, Dict[str, Any]]] = {}
_RECENT_QIWE_MESSAGE_CONTEXTS: Dict[Tuple[str, str], Tuple[float, Dict[str, Any]]] = {}


_INTERNAL_PROCESS_PATTERNS = (
    re.compile(r"Dangerous command requires approval", re.IGNORECASE),
    re.compile(r"Reply\s+[`'\"]?/approve\b", re.IGNORECASE),
    re.compile(
        r"^\s*(?:\(\s*Response\s+formatting\s+failed,\s*plain\s+text:\s*\)\s*)?"
        r"(?:⚡\s*)?Interrupting\s+current\s+task\.\s*"
        r"I(?:'|’)ll\s+respond\s+to\s+your\s+message\s+shortly\.\s*$",
        re.IGNORECASE | re.DOTALL,
    ),
    re.compile(
        r"^\s*(?:[（(]\s*响应格式(?:设置)?失败[,，]\s*显示为纯文本[：:]\s*[）)]\s*)?"
        r"(?:⚡\s*)?(?:我)?(?:正在)?中断当前的?任务[,，]\s*"
        r"稍后(?:我)?(?:就)?会回复[您你]的消息[。.]?\s*$",
        re.DOTALL,
    ),
    re.compile(r"^\s*⏳\s*Working\b", re.IGNORECASE | re.MULTILINE),
    re.compile(r"\bexecute_code\b", re.IGNORECASE),
    re.compile(r"\bskill_view\b", re.IGNORECASE),
    re.compile(r"\btool_progress\b", re.IGNORECASE),
    re.compile(r"\btraceback\s+\(most recent call last\)", re.IGNORECASE),
    re.compile(r"\bFile \"(?:/home|/Users|/tmp|/var|/opt)/[^\"]+\"", re.IGNORECASE),
    re.compile(r"(?:/home/ubuntu|/Users/evans|/tmp|/var/tmp)/\S+"),
    re.compile(r"\b(?:tbl|rec)[A-Za-z0-9]{8,}\b"),
    re.compile(r"\b(?:obj_token|app_token|tenant_access_token|record_id|runId|session_key)\b", re.IGNORECASE),
    re.compile(r"\b[A-Z0-9]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b"),
)


QIWE_RICH_MESSAGE_SPECS: Dict[str, Dict[str, Any]] = {
    "image": {
        "method": "/msg/sendImage",
        "fields": [
            ("fileAesKey", ("file_aes_key", "fileAesKey"), True, "text"),
            ("fileId", ("file_id", "fileId"), True, "text"),
            ("fileMd5", ("file_md5", "fileMd5"), True, "text"),
            ("fileSize", ("file_size", "fileSize"), True, "int"),
            ("filename", ("filename", "file_name", "fileName"), True, "text"),
        ],
    },
    "gif": {
        "method": "/msg/sendGif",
        "fields": [
            ("wxFileUrl", ("wx_file_url", "wxFileUrl"), True, "text"),
        ],
    },
    "file": {
        "method": "/msg/sendFile",
        "fields": [
            ("fileAesKey", ("file_aes_key", "fileAesKey"), True, "text"),
            ("fileId", ("file_id", "fileId"), True, "text"),
            ("fileSize", ("file_size", "fileSize"), True, "int"),
            ("filename", ("filename", "file_name", "fileName"), True, "text"),
        ],
    },
    "voice": {
        "method": "/msg/sendVoice",
        "fields": [
            ("fileAesKey", ("file_aes_key", "fileAesKey"), True, "text"),
            ("fileId", ("file_id", "fileId"), True, "text"),
            ("fileSize", ("file_size", "fileSize"), True, "int"),
            ("voiceTime", ("voice_time", "voiceTime"), True, "int"),
        ],
    },
    "link": {
        "method": "/msg/sendLink",
        "fields": [
            ("title", ("title",), True, "text"),
            ("iconUrl", ("icon_url", "iconUrl"), False, "text"),
            ("linkUrl", ("link_url", "linkUrl", "url"), True, "text"),
            ("desc", ("desc", "description"), False, "text"),
        ],
    },
    "weapp": {
        "method": "/msg/sendWeapp",
        "fields": [
            ("appId", ("app_id", "appId"), True, "text"),
            ("coverFileAesKey", ("cover_file_aes_key", "coverFileAesKey"), True, "text"),
            ("coverFileId", ("cover_file_id", "coverFileId"), True, "text"),
            ("coverFileSize", ("cover_file_size", "coverFileSize"), True, "int"),
            ("desc", ("desc", "description"), True, "text"),
            ("pagePath", ("page_path", "pagePath"), True, "text"),
            ("thumbUrl", ("thumb_url", "thumbUrl"), True, "text"),
            ("title", ("title",), True, "text"),
            ("username", ("username",), True, "text"),
        ],
    },
    "personal_card": {
        "method": "/msg/sendPersonalCard",
        "fields": [
            ("sharedId", ("shared_id", "sharedId"), True, "text"),
        ],
    },
}

QIWE_RICH_MESSAGE_TYPE_ALIASES = {
    "personalcard": "personal_card",
    "personal-card": "personal_card",
    "card": "personal_card",
    "mini_program": "weapp",
    "mini-program": "weapp",
    "miniprogram": "weapp",
    "we_app": "weapp",
    "we-app": "weapp",
}


@dataclass
class QiWeConfig:
    api_url: str = DEFAULT_API_URL
    token: str = ""
    guid: str = ""
    bot_user_id: str = ""
    bot_names: List[str] = field(default_factory=lambda: ["二花"])
    webhook_host: str = DEFAULT_WEBHOOK_HOST
    webhook_port: int = DEFAULT_WEBHOOK_PORT
    webhook_path: str = DEFAULT_WEBHOOK_PATH
    max_body_bytes: int = DEFAULT_MAX_BODY_BYTES
    max_reply_chars: int = DEFAULT_MAX_REPLY_CHARS
    dedupe_ttl_seconds: int = DEFAULT_DEDUPE_TTL_SECONDS
    mention_sender: bool = True
    is_no_need_read: bool = True
    send_enabled: bool = True
    direct_enabled: bool = True
    direct_allow_all: bool = False
    direct_allowed_users: List[str] = field(default_factory=list)
    location_tool_dedupe_ttl_seconds: int = DEFAULT_LOCATION_TOOL_DEDUPE_TTL_SECONDS
    direct_tool_dedupe_ttl_seconds: int = DEFAULT_DIRECT_TOOL_DEDUPE_TTL_SECONDS
    rich_message_tool_dedupe_ttl_seconds: int = DEFAULT_RICH_MESSAGE_TOOL_DEDUPE_TTL_SECONDS
    revoke_message_tool_dedupe_ttl_seconds: int = DEFAULT_REVOKE_MESSAGE_TOOL_DEDUPE_TTL_SECONDS
    voice_to_text_tool_dedupe_ttl_seconds: int = DEFAULT_VOICE_TO_TEXT_TOOL_DEDUPE_TTL_SECONDS
    human_handoff_tool_dedupe_ttl_seconds: int = DEFAULT_HUMAN_HANDOFF_TOOL_DEDUPE_TTL_SECONDS
    human_handoff_enabled: bool = False
    human_handoff_group_map: Dict[str, Dict[str, str]] = field(default_factory=dict)
    human_handoff_user_id: str = ""
    human_handoff_display_name: str = "秦托邦小客服"
    contact_request_tool_dedupe_ttl_seconds: int = DEFAULT_CONTACT_REQUEST_TOOL_DEDUPE_TTL_SECONDS
    contact_guard_enabled: bool = True
    contact_guard_cache_ttl_seconds: int = DEFAULT_CONTACT_GUARD_CACHE_TTL_SECONDS
    contact_guard_page_limit: int = DEFAULT_CONTACT_GUARD_PAGE_LIMIT
    contact_guard_max_pages: int = DEFAULT_CONTACT_GUARD_MAX_PAGES
    identity_lookup_enabled: bool = True
    identity_cache_ttl_seconds: int = DEFAULT_IDENTITY_CACHE_TTL_SECONDS
    state_dir: str = ""
    audit_enabled: bool = False
    voice_to_text_enabled: bool = False
    voice_to_text_poll_attempts: int = 5
    voice_to_text_poll_interval_seconds: float = 0.5
    pipeline_enabled: bool = False
    passive_pipeline_enabled: bool = False
    solitaire_processor_enabled: bool = False
    passive_allowed_groups: List[str] = field(default_factory=list)
    passive_ack_enabled: bool = False
    passive_ack_allowed_groups: List[str] = field(default_factory=list)
    active_attachment_preprocess_enabled: bool = False
    activity_reminder_enabled: bool = False
    activity_reminder_dry_run: bool = True
    activity_reminder_scan_interval_seconds: int = 60
    activity_reminder_allowed_groups: List[str] = field(default_factory=list)
    nats_capture_enabled: bool = False
    nats_url: str = "nats://127.0.0.1:4222"
    nats_raw_subject: str = "qintopia.qiwe.raw"
    nats_message_subject: str = "qintopia.qiwe.message"
    nats_capture_timeout_seconds: float = 0.5
    answer_context_prepare_enabled: bool = True
    answer_context_mcp_command: str = DEFAULT_ANSWER_CONTEXT_MCP_COMMAND
    answer_context_prepare_timeout_seconds: float = 1.2


@dataclass
class ParsedQiWeMessage:
    accepted: bool
    reason: str
    should_trigger: bool = False
    text: str = ""
    content: str = ""
    message_kind: str = "text"
    attachments: List[Dict[str, Any]] = field(default_factory=list)
    conversation_type: str = "group"
    group_id: str = ""
    sender_id: str = ""
    sender_name: str = ""
    receiver_id: str = ""
    message_id: str = ""
    guid: str = ""
    timestamp: Optional[datetime] = None
    at_list: List[Dict[str, Any]] = field(default_factory=list)
    payload: Dict[str, Any] = field(default_factory=dict)
    raw_event: Dict[str, Any] = field(default_factory=dict)
    referenced_message: Dict[str, Any] = field(default_factory=dict)
    group_id_mismatch: bool = False
    outer_group_id: str = ""
    is_mentioned: bool = False

    @property
    def chat_id(self) -> str:
        return self.group_id if self.conversation_type == "group" else self.sender_id


def _csv(value: str) -> List[str]:
    if isinstance(value, (list, tuple, set)):
        return [str(part).strip() for part in value if str(part).strip()]
    return [part.strip() for part in str(value or "").split(",") if part.strip()]


def _bool(value: Any, default: bool = False) -> bool:
    if value is None or value == "":
        return default
    if isinstance(value, bool):
        return value
    return str(value).strip().lower() in {"1", "true", "yes", "on"}


def _int(value: Any, default: int) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _text(value: Any) -> str:
    return str(value if value is not None else "").replace("\r\n", "\n").strip()


def _json_object(value: Any) -> Dict[str, Any]:
    if isinstance(value, dict):
        return value
    text = _text(value)
    if not text:
        return {}
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        logger.warning("[qiwe] invalid JSON object config ignored")
        return {}
    return parsed if isinstance(parsed, dict) else {}


def _human_handoff_group_map(value: Any) -> Dict[str, Dict[str, str]]:
    payload = _json_object(value)
    mapping: Dict[str, Dict[str, str]] = {}
    for group_id, item in payload.items():
        group = _text(group_id)
        if not group or not isinstance(item, dict):
            continue
        user_id = _text(item.get("user_id") or item.get("userId") or item.get("support_user_id"))
        if not user_id:
            continue
        mapping[group] = {
            "user_id": user_id,
            "display_name": _text(item.get("display_name") or item.get("displayName") or item.get("name")),
        }
    return mapping


def _display_text(value: Any) -> str:
    text = _text(value)
    if not text:
        return ""
    try:
        padded = text + "=" * (-len(text) % 4)
        decoded = base64.b64decode(padded, validate=True).decode("utf-8").strip()
    except Exception:
        return text
    return decoded if decoded and decoded.isprintable() else text


def _is_no_reply_sentinel(value: Any) -> bool:
    return _text(value) == "NO_REPLY"


def _is_silent_sentinel(value: Any) -> bool:
    return _text(value) == "[SILENT]"


def _is_internal_process_message(value: Any) -> bool:
    text = str(value if value is not None else "")
    if not text.strip():
        return False
    return any(pattern.search(text) for pattern in _INTERNAL_PROCESS_PATTERNS)


def _strip_cron_delivery_wrapper(value: Any) -> str:
    text = _text(value)
    if not text:
        return ""

    if text.startswith("Cronjob Response:"):
        lines = text.splitlines()
        body_start = None
        for index, line in enumerate(lines):
            stripped = line.strip()
            if len(stripped) >= 3 and set(stripped) == {"-"}:
                body_start = index + 1
                break
        if body_start is not None:
            body = "\n".join(lines[body_start:]).strip()
            body = re.sub(r"\n\s*To stop or manage this job,.*\Z", "", body, flags=re.DOTALL).strip()
            return body

    if text.startswith("# Cron Job:") and "\n---\n" in text:
        return text.split("\n---\n", 1)[1].strip()

    return text


def _dedupe_texts(values: Iterable[Any]) -> List[str]:
    seen = set()
    result = []
    for value in values:
        text = _text(value)
        if not text or text in seen:
            continue
        seen.add(text)
        result.append(text)
    return result


def _strip_leading_sender_name(text: str, name: str, *, require_at: bool = False) -> Optional[str]:
    reply = _text(text)
    candidate = _text(name)
    if not reply or not candidate:
        return None

    index = 0
    if reply.startswith("@"):
        index = 1
        while index < len(reply) and reply[index] in f" \t{MENTION_SPACES}":
            index += 1
    elif require_at:
        return None

    if not reply[index:].startswith(candidate):
        return None

    end = index + len(candidate)
    if end < len(reply):
        next_char = reply[end]
        boundary_chars = f" \t\n\r{MENTION_SPACES}:：,，、。.!！?？"
        if next_char not in boundary_chars:
            return None

    while end < len(reply) and reply[end] in f" \t{MENTION_SPACES}":
        end += 1
    if end < len(reply) and reply[end] in ":：,，、":
        end += 1
        while end < len(reply) and reply[end] in f" \t{MENTION_SPACES}":
            end += 1
    return _text(reply[end:])


def _strip_redundant_sender_prefix(
    text: str,
    *,
    sender_id: str = "",
    sender_display_names: Optional[Iterable[str]] = None,
) -> str:
    reply = _text(text)
    display_names = [name for name in _dedupe_texts(sender_display_names or []) if name != _text(sender_id)]
    for name in display_names:
        stripped = _strip_leading_sender_name(reply, name)
        if stripped is not None:
            return stripped
    if sender_id:
        stripped = _strip_leading_sender_name(reply, sender_id, require_at=True)
        if stripped is not None:
            return stripped
    return reply


def _build_qiwe_reply_payload(reply_ref: Optional[Dict[str, Any]]) -> Dict[str, Any]:
    if not isinstance(reply_ref, dict):
        return {}
    msg_data = reply_ref.get("msgData") if isinstance(reply_ref.get("msgData"), dict) else {}
    content = _text(msg_data.get("content") or msg_data.get("title"))
    if not content:
        return {}
    payload: Dict[str, Any] = {
        "type": 0,
        "msgData": {"content": content},
    }
    for key in ("userId", "showName", "timeStamp", "msgUniqueIdentifier"):
        value = reply_ref.get(key)
        if value not in ("", None):
            payload[key] = value
    return payload


def _first_mapping(value: Any) -> Dict[str, Any]:
    if isinstance(value, dict):
        return value
    if isinstance(value, list):
        for item in value:
            if isinstance(item, dict):
                return item
    return {}


def _first_present(values: Dict[str, Any], names: Iterable[str]) -> Any:
    for name in names:
        if isinstance(values, dict) and name in values:
            return values.get(name)
    return None


def _normalize_rich_message_type(value: Any) -> str:
    kind = _text(value).lower().replace(" ", "_")
    return QIWE_RICH_MESSAGE_TYPE_ALIASES.get(kind, kind)


def _safe_qiwe_status(raw_response: Any) -> Dict[str, Any]:
    if not isinstance(raw_response, dict):
        return {}
    data = raw_response.get("data")
    message_info = _first_mapping(data)
    if not message_info and isinstance(data, list) and data:
        message_info = _first_mapping(data[0])
    safe_data = {
        key: message_info.get(key)
        for key in ("isSendSuccess", "msgServerId", "msgType", "msgUniqueIdentifier", "seq", "timestamp")
        if message_info.get(key) is not None
    }
    result = {
        "qiwe_code": raw_response.get("code"),
        "qiwe_msg": _text(raw_response.get("msg")),
    }
    if safe_data:
        result["message"] = safe_data
    return result


def _parse_body(raw_body: bytes | str | Dict[str, Any]) -> Dict[str, Any]:
    if isinstance(raw_body, dict):
        return raw_body
    if isinstance(raw_body, bytes):
        raw_body = raw_body.decode("utf-8")
    parsed = json.loads(raw_body)
    return _first_mapping(parsed)


def parse_nested_data(payload: Dict[str, Any]) -> Dict[str, Any]:
    raw = payload.get("data")
    if isinstance(raw, str):
        try:
            return _first_mapping(json.loads(raw))
        except json.JSONDecodeError:
            return {}
    return _first_mapping(raw)


def _at_list(payload: Dict[str, Any], raw_event: Dict[str, Any]) -> List[Dict[str, Any]]:
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    payload_msg_data = payload.get("msgData") if isinstance(payload.get("msgData"), dict) else {}
    candidates = msg_data.get("atList") or raw_event.get("atList") or payload_msg_data.get("atList") or payload.get("atList")
    return [item for item in candidates if isinstance(item, dict)] if isinstance(candidates, list) else []


def _content(payload: Dict[str, Any], raw_event: Dict[str, Any]) -> str:
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    payload_msg_data = payload.get("msgData") if isinstance(payload.get("msgData"), dict) else {}
    if _is_solitaire_message(payload, raw_event):
        return _text(msg_data.get("title") or payload_msg_data.get("title") or raw_event.get("title") or payload.get("title"))
    return _text(msg_data.get("content") or raw_event.get("content") or payload_msg_data.get("content") or payload.get("content"))


def _msg_data(payload: Dict[str, Any], raw_event: Dict[str, Any]) -> Any:
    if raw_event.get("msgData") is not None:
        return raw_event.get("msgData")
    if payload.get("msgData") is not None:
        return payload.get("msgData")
    return {}


def _message_kind(payload: Dict[str, Any], raw_event: Dict[str, Any]) -> str:
    new_msg_type = _text(raw_event.get("newMsgType") or payload.get("newMsgType") or payload.get("commonMsgType")).upper()
    if _is_solitaire_message(payload, raw_event):
        return "solitaire"
    if new_msg_type:
        if new_msg_type in {"TEXT", "TEXT_ALT"}:
            return "text"
        if "IMAGE" in new_msg_type:
            return "image"
        if "VIDEO" in new_msg_type:
            return "video"
        if "FILE" in new_msg_type:
            return "file"
        if "GIF" in new_msg_type:
            return "gif"
        if "LOCATION" in new_msg_type:
            return "location"
        if "LINK" in new_msg_type:
            return "link"
        if "VOICE" in new_msg_type:
            return "voice"
        if "MIXED" in new_msg_type:
            return "mixed"
        if "BUSINESS_CARD" in new_msg_type:
            return "card"
        return "system" if new_msg_type.endswith("NOTIFY") or new_msg_type.startswith("GROUP_") else "unsupported"

    try:
        msg_type = int(raw_event.get("msgType", payload.get("msgType")))
    except (TypeError, ValueError):
        return "text" if _is_text_message(payload, raw_event) else "unsupported"
    if msg_type in {0, 2}:
        return "text"
    if msg_type in {7, 14, 101}:
        return "image"
    if msg_type in {22, 23, 103}:
        return "video"
    if msg_type in {15, 20, 102}:
        return "file"
    if msg_type in {29, 104}:
        return "gif"
    if msg_type == 6:
        return "location"
    if msg_type == 13:
        return "link"
    if msg_type == 16:
        return "voice"
    if msg_type == 123:
        return "mixed"
    if msg_type == 41:
        return "card"
    if msg_type in {2001, 2005, 2063} or msg_type >= 1000:
        return "system"
    return "unsupported"


def _is_solitaire_message(payload: Dict[str, Any], raw_event: Dict[str, Any]) -> bool:
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    payload_msg_data = payload.get("msgData") if isinstance(payload.get("msgData"), dict) else {}
    common_type = _text(raw_event.get("newMsgType") or payload.get("newMsgType") or payload.get("commonMsgType")).upper()
    if common_type == "SOLITAIRE":
        return True
    try:
        if int(raw_event.get("msgType", payload.get("msgType"))) == 213:
            return True
    except (TypeError, ValueError):
        pass
    return isinstance(msg_data.get("solitaireInfo"), dict) or isinstance(payload_msg_data.get("solitaireInfo"), dict)


def _attachment_from_data(kind: str, data: Any, raw_event: Dict[str, Any]) -> Dict[str, Any]:
    attachment: Dict[str, Any] = {
        "kind": kind,
        "msg_type": raw_event.get("msgType"),
        "new_msg_type": _text(raw_event.get("newMsgType")),
        "msg_server_id": _text(raw_event.get("msgServerId")),
    }
    if not isinstance(data, dict):
        attachment["raw_shape"] = type(data).__name__
        return attachment

    if kind == "location":
        attachment.update(
            {
                "title": _display_text(data.get("title")),
                "address": _display_text(data.get("address")),
                "latitude": data.get("latitude"),
                "longitude": data.get("longitude"),
                "zoom": data.get("zoom"),
            }
        )
    elif kind == "link":
        attachment.update(
            {
                "title": _display_text(data.get("title")),
                "description": _display_text(data.get("desc")),
                "url": _text(data.get("linkUrl")),
                "icon_url": _text(data.get("iconUrl")),
            }
        )
    elif kind == "voice":
        attachment.update(
            {
                "file_id": _text(data.get("fileId")),
                "file_md5": _text(data.get("fileMd5")),
                "file_size": data.get("fileSize"),
                "voice_time": data.get("voiceTime"),
            }
        )
    elif kind in {"image", "video", "file", "gif"}:
        attachment.update(
            {
                "file_id": _text(data.get("fileId")),
                "file_name": _display_text(data.get("fileName") or data.get("filename")),
                "file_md5": _text(data.get("fileMd5")),
                "file_size": data.get("fileSize"),
                "file_url": _text(data.get("fileHttpUrl") or data.get("fileBigHttpUrl") or data.get("fileMiddleHttpUrl")),
            }
        )
    elif kind == "card":
        attachment.update({"nickname": _display_text(data.get("nickname")), "shared_id": _text(data.get("shared_id"))})
    elif kind == "solitaire":
        solitaire_info = data.get("solitaireInfo") if isinstance(data.get("solitaireInfo"), dict) else {}
        attachment.update(
            {
                "title": _text(data.get("title")),
                "solitaire_info": solitaire_info,
                "author_id": _text(solitaire_info.get("authorId")),
                "items": solitaire_info.get("items") if isinstance(solitaire_info.get("items"), list) else [],
            }
        )
    return {key: value for key, value in attachment.items() if value not in ("", None)}


def _attachments(payload: Dict[str, Any], raw_event: Dict[str, Any], kind: str) -> List[Dict[str, Any]]:
    if kind == "text":
        return []
    data = _msg_data(payload, raw_event)
    if kind == "mixed" and isinstance(data, list):
        attachments = []
        for item in data:
            if not isinstance(item, dict):
                continue
            sub_type = item.get("subMsgType")
            sub_data = item.get("subMsgData", {})
            sub_kind = _message_kind({"msgType": sub_type}, {"msgType": sub_type, "msgData": sub_data})
            attachments.append(_attachment_from_data(sub_kind, sub_data, {"msgType": sub_type, "msgServerId": raw_event.get("msgServerId")}))
        return attachments
    return [_attachment_from_data(kind, data, raw_event)]


def _message_context_from_parsed(parsed: ParsedQiWeMessage) -> Dict[str, Any]:
    context: Dict[str, Any] = {
        "message_id": parsed.message_id,
        "message_kind": parsed.message_kind,
        "text": _text(parsed.content or parsed.text),
    }
    if parsed.attachments:
        context["attachments"] = parsed.attachments[:3]
    if parsed.message_kind == "link" and parsed.attachments:
        link = parsed.attachments[0]
        context["link"] = {
            key: value
            for key, value in {
                "title": _display_text(link.get("title")),
                "description": _display_text(link.get("description")),
                "url": _text(link.get("url")),
            }.items()
            if value
        }
    return {key: value for key, value in context.items() if value not in ("", None, [], {})}


def _store_recent_message_context(parsed: ParsedQiWeMessage) -> None:
    chat = _text(parsed.chat_id)
    message_id = _text(parsed.message_id)
    if not chat or not message_id:
        return
    context = _message_context_from_parsed(parsed)
    if not context:
        return
    now = time.time()
    expired = [
        key
        for key, (seen_at, _) in _RECENT_QIWE_MESSAGE_CONTEXTS.items()
        if now - seen_at > DEFAULT_RECENT_MESSAGE_REF_TTL_SECONDS
    ]
    for key in expired:
        _RECENT_QIWE_MESSAGE_CONTEXTS.pop(key, None)
    _RECENT_QIWE_MESSAGE_CONTEXTS[(chat, message_id)] = (now, context)


def _recent_message_context(chat_id: str, message_id: str) -> Dict[str, Any]:
    key = (_text(chat_id), _text(message_id))
    if not key[0] or not key[1]:
        return {}
    entry = _RECENT_QIWE_MESSAGE_CONTEXTS.get(key)
    if not entry:
        return {}
    seen_at, context = entry
    if time.time() - seen_at > DEFAULT_RECENT_MESSAGE_REF_TTL_SECONDS:
        _RECENT_QIWE_MESSAGE_CONTEXTS.pop(key, None)
        return {}
    return dict(context)


def _reply_reference(raw_event: Dict[str, Any]) -> Dict[str, Any]:
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    reply = msg_data.get("reply")
    if not isinstance(reply, dict) or not reply:
        return {}
    msg_id = _text(reply.get("msgId") or reply.get("msgUniqueIdentifier"))
    ref: Dict[str, Any] = {
        "message_id": msg_id,
        "message_kind": _message_kind({"msgType": reply.get("msgType")}, {"msgType": reply.get("msgType"), "msgData": reply}),
        "text": _text(reply.get("content") or reply.get("title")),
    }
    if isinstance(reply.get("reply"), dict):
        ref["nested_reply"] = True
    return {key: value for key, value in ref.items() if value not in ("", None, [], {})}


def _resolve_referenced_message(parsed: ParsedQiWeMessage) -> Dict[str, Any]:
    ref = _reply_reference(parsed.raw_event)
    ref_id = _text(ref.get("message_id"))
    if not ref_id:
        return {}
    cached = _recent_message_context(parsed.chat_id, ref_id)
    if cached:
        merged = dict(cached)
        merged["source"] = "recent_message_cache"
        return merged
    ref["source"] = "reply_payload"
    return ref


def _referenced_message_text(reference: Dict[str, Any]) -> str:
    if not reference:
        return ""
    lines = ["引用消息上下文："]
    kind = _text(reference.get("message_kind"))
    if kind:
        lines.append(f"- 类型：{kind}")
    link = reference.get("link") if isinstance(reference.get("link"), dict) else {}
    if link:
        title = _display_text(link.get("title"))
        description = _display_text(link.get("description"))
        url = _text(link.get("url"))
        if title:
            lines.append(f"- 链接标题：{title}")
        if description:
            lines.append(f"- 链接摘要：{description}")
        if url:
            lines.append(f"- 链接地址：{url}")
    else:
        text = _display_text(reference.get("text"))
        if text:
            lines.append(f"- 内容：{text}")
    source = _text(reference.get("source"))
    if source:
        lines.append(f"- 来源：{source}")
    return "\n".join(lines)


def _is_text_message(payload: Dict[str, Any], raw_event: Dict[str, Any]) -> bool:
    new_msg_type = _text(raw_event.get("newMsgType") or payload.get("newMsgType") or payload.get("commonMsgType")).upper()
    if new_msg_type in {"TEXT", "TEXT_ALT"}:
        return True
    msg_type = raw_event.get("msgType", payload.get("msgType"))
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    payload_msg_data = payload.get("msgData") if isinstance(payload.get("msgData"), dict) else {}
    return msg_type in {0, 2} or isinstance(msg_data.get("content"), str) or isinstance(payload_msg_data.get("content"), str) or isinstance(payload.get("content"), str)


def _is_ordinary_message(raw_event: Dict[str, Any]) -> bool:
    if raw_event.get("cmd") is None:
        return True
    try:
        return int(raw_event.get("cmd")) == 15000
    except (TypeError, ValueError):
        return False


def _message_id(payload: Dict[str, Any], raw_event: Dict[str, Any], group_id: str, sender_id: str, content: str) -> str:
    value = (
        raw_event.get("msgUniqueIdentifier")
        or payload.get("msgUniqueIdentifier")
        or raw_event.get("msgServerId")
        or payload.get("msgServerId")
        or raw_event.get("requestId")
        or payload.get("requestId")
        or payload.get("guid")
        or raw_event.get("guid")
    )
    if value is not None:
        return _text(value)
    return f"{group_id}:{sender_id}:{raw_event.get('timestamp') or int(time.time())}:{content}"


def _timestamp(raw_event: Dict[str, Any]) -> Optional[datetime]:
    raw = raw_event.get("timestamp")
    try:
        return datetime.fromtimestamp(int(raw), tz=timezone.utc)
    except (TypeError, ValueError, OSError):
        return None


def _mention_pattern(name: str) -> re.Pattern[str]:
    return re.compile(rf"@\s*{re.escape(name)}(?:\s|[{MENTION_SPACES}{re.escape(MENTION_BOUNDARY_PUNCT)}]|$)")


def _mention_matches(content: str, at_list: Iterable[Dict[str, Any]], bot_names: Iterable[str], bot_user_id: str) -> bool:
    if bot_user_id and any(_text(item.get("userId")) == bot_user_id for item in at_list):
        return True
    for name in bot_names:
        if any(_text(item.get("nickname")) == name for item in at_list):
            return True
        if name and _mention_pattern(name).search(content):
            return True
    return False


def _cue_matches(content: str, bot_names: Iterable[str]) -> bool:
    normalized = _text(content)
    if normalized.startswith("/"):
        return False
    cue_words = ("在吗", "帮", "查", "问", "在哪", "位置", "怎么去", "是谁", "是什么", "?")
    for name in bot_names:
        if name and name in normalized and any(word in normalized for word in cue_words):
            return True
    return False


def _strip_mentions(content: str, at_list: Iterable[Dict[str, Any]], bot_names: Iterable[str], bot_user_id: str) -> str:
    result = content
    names = {name for name in bot_names if name}
    for item in at_list:
        nickname = _text(item.get("nickname"))
        if nickname:
            names.add(nickname)
    for name in names:
        result = re.sub(rf"@\s*{re.escape(name)}[\s{MENTION_SPACES}{re.escape(MENTION_BOUNDARY_PUNCT)}]*", "", result)
    if bot_user_id:
        result = re.sub(rf"@?\s*{re.escape(bot_user_id)}[\s{MENTION_SPACES}{re.escape(MENTION_BOUNDARY_PUNCT)}]*", "", result)
    return _text(result)


def _member_context_channel_prompt(
    parsed: ParsedQiWeMessage,
    identity: Optional["QiWeIdentity"] = None,
    answer_context: Optional[Dict[str, Any]] = None,
) -> str:
    chat_id = _text(parsed.chat_id)
    sender_id = _text(parsed.sender_id)
    if not chat_id or not sender_id:
        return ""
    display_name = _display_text(getattr(identity, "display_name", "") if identity else "") or _display_text(parsed.sender_name)
    context = {
        "platform": "qiwe",
        "chat_id": chat_id,
        "channel_user_id": sender_id,
        "display_name": display_name,
        "conversation_type": parsed.conversation_type,
        "message_kind": parsed.message_kind,
    }
    if answer_context is None:
        answer_context = {
            "success": False,
            "reason": "answer_context_unavailable",
            "answer_rules": {
                "do_not_guess_member_state": True,
                "ask_clarification_when_ambiguous": True,
                "do_not_disclose_profile_source": True,
                "do_not_claim_monitoring": True,
            },
        }
    reply_directives = _answer_context_reply_directives(answer_context)
    return (
        f"{reply_directives}\n\n"
        "QiWe 当前说话人上下文如下。用户没有要求公开人物画像时，不要在回复中提到这些字段、工具名或画像来源。\n"
        f"{json.dumps(context, ensure_ascii=False)}\n"
        "Agent OS 已为本轮回复准备 answer_context。回答前先读取 answer_context，不要自行猜测成员身份或状态。"
        "如果当前是私聊且用户表达“记住”“以后你要”“这个人喜欢”“按这个方式回复”等训练意图，"
        "必须调用 qintopia_erhua_training_note_submit 或其 MCP 前缀工具提交训练；"
        "trainer_user_id 必须使用上方 channel_user_id，chat_id 必须使用上方 chat_id，source_conversation_type 必须使用上方 conversation_type，不要让模型自造训练员身份。"
        "工具返回 success=true 且 accepted=true 时，才可以告诉用户已记录；否则只能说这条暂时没记上或只能作为本轮临时要求。"
        "如果 mentioned_members 中有 resolved=true 的成员，回答相关问题时优先使用其 safe_summary 和 safe_reply_hints。"
        "如果被提及成员 unresolved 或 ambiguous，先反问确认，不要硬猜。"
        "不要说自己在监控群成员，不要说“画像显示”，不要暴露 raw history、隐藏画像、敏感事实、内部标签或日报全文。\n"
        f"answer_context: {json.dumps(answer_context, ensure_ascii=False)}"
    )


def _answer_context_reply_directives(answer_context: Dict[str, Any]) -> str:
    if not isinstance(answer_context, dict) or not answer_context.get("success"):
        return (
            "Agent OS 本轮回答上下文不可用。"
            "如果用户询问某个群成员的状态、原因或身份，不要猜测；请简短说明当前不能确认，并请用户补充对象或直接 @ 对方。"
        )

    lines = [
        "Agent OS 本轮回答约束：",
        "- 这是当前轮的确定性安全上下文，优先级高于会话历史中的旧猜测。",
        "- 不要说自己在实时盯群、监控成员或查看画像；可以自然地基于下面的安全上下文回答。",
        "- 不要暴露工具名、字段名、画像来源、raw history、内部标签或日报内容。",
        "- 如果用户要求“记住”“以后你要”“这个人喜欢”等长期训练，只有训练员授权路径返回成功时才可以说已经记住；否则只能按当前对话临时回应。",
    ]
    training_guidance = answer_context.get("training_guidance")
    if isinstance(training_guidance, dict):
        persona_overlays = training_guidance.get("persona_overlays")
        member_guidance = training_guidance.get("member_guidance")
        reply_examples = training_guidance.get("reply_examples")
        if isinstance(persona_overlays, list) and persona_overlays:
            lines.append("- 有已审核的二花回复风格增量；可自然采用，但不能覆盖隐私、安全、知识源和转人工边界。")
            for overlay in persona_overlays[:3]:
                overlay_text = _display_text(overlay)
                if overlay_text:
                    lines.append(f"  - {overlay_text[:180]}")
        if isinstance(member_guidance, list) and member_guidance:
            lines.append("- 当前说话人有训练员确认的沟通偏好；回复时可以自然使用，不要说来源于训练记录。")
            for item in member_guidance[:3]:
                if not isinstance(item, dict):
                    continue
                summary = _display_text(item.get("summary"))
                if summary:
                    lines.append(f"  - {summary[:180]}")
        if isinstance(reply_examples, list) and reply_examples:
            lines.append("- 有训练员确认的回复示例；可参考表达方式，不要逐字照搬，不要说来源于训练记录。")
            for example in reply_examples[:3]:
                example_text = _display_text(example)
                if example_text:
                    lines.append(f"  - {example_text[:180]}")
    mentioned = answer_context.get("mentioned_members")
    if isinstance(mentioned, list):
        resolved_members = [
            item for item in mentioned
            if isinstance(item, dict) and item.get("resolved") is True
        ]
        ambiguous_members = [
            item for item in mentioned
            if isinstance(item, dict) and item.get("resolved") is not True
        ]
        if resolved_members:
            lines.append("- 用户问题中提到的成员已解析；回答成员相关问题时必须优先使用这些安全上下文，不要回答“我不知道”。")
            for member in resolved_members[:3]:
                mention_text = _display_text(member.get("mention_text"))
                display_name = _display_text(member.get("display_name"))
                safe_summary = _display_text(member.get("safe_summary"))
                hints = member.get("safe_reply_hints") if isinstance(member.get("safe_reply_hints"), dict) else {}
                notes = hints.get("temporary_communication_notes") if isinstance(hints, dict) else []
                topics = hints.get("topics") if isinstance(hints, dict) else []
                label = display_name or mention_text or "已解析成员"
                raw_mention = _text(member.get("mention_text"))
                mention_label = raw_mention or mention_text or label
                lines.append(f"  - {mention_label} => {label}")
                if safe_summary:
                    lines.append(f"    安全摘要：{safe_summary}")
                if isinstance(topics, list) and topics:
                    lines.append(f"    相关主题：{', '.join(_display_text(topic) for topic in topics if _display_text(topic))}")
                lines.append("    如果用户询问该成员的状态、原因、偏好或参与情况，应基于该成员的安全上下文直接回答；不要在已有安全上下文时回答“不知道”。")
                if isinstance(notes, list) and notes:
                    compact_notes = []
                    for note in notes[:3]:
                        note_text = _display_text(note)
                        if note_text:
                            compact_notes.append(note_text[:180])
                    if compact_notes:
                        lines.append("    可用于自然概括的短期沟通提示：")
                        for note_text in compact_notes:
                            lines.append(f"    * {note_text}")
        if ambiguous_members and not resolved_members:
            lines.append("- 用户问题中提到的成员未能稳定解析；必须先反问确认，不要把相似名字硬猜成某个人。")
    return "\n".join(lines)


def _answer_context_mcp_request(
    *,
    chat_id: str,
    sender_id: str,
    message_text: str,
    mentioned_member_names: Optional[Iterable[str]] = None,
) -> str:
    arguments = {
        "caller_profile": "erhua",
        "platform": "qiwe",
        "chat_id": chat_id,
        "sender_id": sender_id,
        "message_text": message_text,
        "purpose": "prepare QiWe reply context",
    }
    names = _dedupe_texts(mentioned_member_names or [])
    if names:
        arguments["mentioned_member_names"] = names
    payloads = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "qiwe-answer-context-prepare", "version": "0.1.0"},
            },
        },
        {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}},
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "qintopia_answer_context_prepare",
                "arguments": arguments,
            },
        },
    ]
    return "\n".join(json.dumps(item, ensure_ascii=False) for item in payloads) + "\n"


def _mentioned_member_names_from_at_list(
    at_list: Iterable[Dict[str, Any]],
    *,
    bot_user_id: str = "",
    bot_names: Iterable[str] = (),
) -> List[str]:
    bot_name_set = set(_dedupe_texts(bot_names))
    names: List[str] = []
    for item in at_list or []:
        if not isinstance(item, dict):
            continue
        if bot_user_id and _text(item.get("userId")) == _text(bot_user_id):
            continue
        name = _display_text(item.get("nickname") or item.get("displayName") or item.get("name"))
        if not name or name in bot_name_set:
            continue
        names.append(name)
    return _dedupe_texts(names)


def _training_note_mcp_request(
    *,
    chat_id: str,
    trainer_user_id: str,
    training_type: str,
    training_text: str,
    source_conversation_type: str = "",
    target_channel_user_id: str = "",
    target_member_name: str = "",
    purpose: str = "submit Erhua trainer memory",
    source_platform_message_id: str = "",
) -> str:
    payloads = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "qiwe-erhua-training-note-submit", "version": "0.1.0"},
            },
        },
        {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}},
        {
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "qintopia_erhua_training_note_submit",
                "arguments": {
                    "caller_profile": "erhua",
                    "platform": "qiwe",
                    "chat_id": chat_id,
                    "source_conversation_type": source_conversation_type,
                    "trainer_user_id": trainer_user_id,
                    "target_channel_user_id": target_channel_user_id,
                    "target_member_name": target_member_name,
                    "training_type": training_type,
                    "training_text": training_text,
                    "purpose": purpose,
                    "source_platform_message_id": source_platform_message_id,
                },
            },
        },
    ]
    return "\n".join(json.dumps(item, ensure_ascii=False) for item in payloads) + "\n"


def _answer_context_from_mcp_stdout(stdout: str) -> Optional[Dict[str, Any]]:
    for line in stdout.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if item.get("id") != 2:
            continue
        content = item.get("result", {}).get("content", [])
        if not content or not isinstance(content[0], dict):
            return None
        text = _text(content[0].get("text"))
        if not text:
            return None
        try:
            data = json.loads(text)
        except json.JSONDecodeError:
            return None
        if not isinstance(data, dict) or not data.get("success"):
            return None
        return data
    return None


def parse_qiwe_payload(
    raw_body: bytes | str | Dict[str, Any],
    *,
    bot_names: Optional[Iterable[str]] = None,
    bot_user_id: str = "",
    direct_enabled: bool = True,
    direct_allow_all: bool = False,
    direct_allowed_users: Optional[Iterable[str]] = None,
    active_attachment_preprocess_enabled: bool = False,
) -> ParsedQiWeMessage:
    payload = _parse_body(raw_body)
    raw_event = parse_nested_data(payload)
    names = list(bot_names or ["二花"])
    allowed_direct = {str(item).strip() for item in (direct_allowed_users or []) if str(item).strip()}

    group_id = _text(raw_event.get("fromRoomId"))
    outer_group_id = _text(payload.get("fromGroup"))
    is_group = bool(group_id and group_id != "0")
    group_id_mismatch = bool(is_group and outer_group_id and group_id != outer_group_id)

    sender_id = _text(raw_event.get("senderId") or payload.get("senderId") or payload.get("fromUser") or raw_event.get("fromUser") or raw_event.get("fromUserId"))
    receiver_id = _text(raw_event.get("receiverId") or payload.get("receiverId") or payload.get("toUser") or raw_event.get("userId"))
    content = _content(payload, raw_event)
    at_list = _at_list(payload, raw_event)
    guid = _text(raw_event.get("guid") or payload.get("guid"))
    msg_id = _message_id(payload, raw_event, group_id, sender_id, content)
    message_kind = _message_kind(payload, raw_event)
    attachments = _attachments(payload, raw_event, message_kind)

    base = {
        "payload": payload,
        "raw_event": raw_event,
        "group_id": group_id,
        "outer_group_id": outer_group_id,
        "group_id_mismatch": group_id_mismatch,
        "conversation_type": "group" if is_group else "direct",
        "sender_id": sender_id,
        "sender_name": _text(raw_event.get("senderName") or payload.get("senderName")),
        "receiver_id": receiver_id,
        "content": content,
        "message_kind": message_kind,
        "attachments": attachments,
        "at_list": at_list,
        "guid": guid,
        "message_id": msg_id,
        "timestamp": _timestamp(raw_event),
    }

    event_code = _text(payload.get("eventCode") or raw_event.get("eventCode"))
    if not is_group and event_code == "group_msg_event":
        return ParsedQiWeMessage(accepted=False, reason="not_group_message", **base)
    if is_group and event_code and event_code != "group_msg_event":
        return ParsedQiWeMessage(accepted=False, reason="not_group_message", **base)
    if not is_group and event_code and event_code not in {"", "single_msg_event", "private_msg_event", "friend_msg_event", "user_msg_event"}:
        return ParsedQiWeMessage(accepted=False, reason="not_direct_message", **base)
    if not _is_ordinary_message(raw_event):
        return ParsedQiWeMessage(accepted=False, reason="not_cmd_15000", **base)
    if message_kind == "solitaire":
        return ParsedQiWeMessage(accepted=True, reason="non_text_solitaire", should_trigger=False, text=content, **base)
    if message_kind != "text":
        mentioned = _mention_matches(content, at_list, names, bot_user_id)
        if mentioned and active_attachment_preprocess_enabled:
            return ParsedQiWeMessage(accepted=True, reason="mentioned", should_trigger=True, text=content, is_mentioned=True, **base)
        return ParsedQiWeMessage(accepted=True, reason=f"non_text_{message_kind}", should_trigger=False, text=content, **base)
    if not content:
        return ParsedQiWeMessage(accepted=False, reason="empty_content", **base)
    if bot_user_id and sender_id == bot_user_id:
        return ParsedQiWeMessage(accepted=False, reason="self_message", **base)
    if is_group and content.lstrip().startswith("/"):
        return ParsedQiWeMessage(accepted=True, reason="blocked_slash_command", should_trigger=False, text=content, **base)

    if not is_group:
        if not direct_enabled:
            return ParsedQiWeMessage(accepted=True, reason="direct_disabled", should_trigger=False, text=content, **base)
        if bot_user_id and receiver_id and receiver_id != bot_user_id:
            return ParsedQiWeMessage(accepted=False, reason="direct_not_to_bot", **base)
        if not direct_allow_all and sender_id not in allowed_direct:
            return ParsedQiWeMessage(accepted=True, reason="direct_not_allowed", should_trigger=False, text=content, **base)
        return ParsedQiWeMessage(accepted=True, reason="direct_message", should_trigger=True, text=content, **base)

    mentioned = _mention_matches(content, at_list, names, bot_user_id)
    cued = _cue_matches(content, names)
    if not mentioned and not cued:
        return ParsedQiWeMessage(accepted=True, reason="not_mentioned", should_trigger=False, text=content, **base)

    stripped = _strip_mentions(content, at_list, names, bot_user_id) or content
    return ParsedQiWeMessage(accepted=True, reason="mentioned" if mentioned else "cued", should_trigger=True, text=stripped, is_mentioned=mentioned, **base)


@dataclass
class QiWeIdentity:
    user_id: str
    display_name: str = ""
    source: str = "none"


@dataclass
class QiWeContactGuardDecision:
    allowed: bool
    user_id: str
    contact_type: Optional[int] = None
    reason: str = ""
    retryable: bool = False


class QiWeIdentityResolver:
    def __init__(self, adapter: "QiWeAdapter"):
        self.adapter = adapter
        self._cache: Dict[Tuple[str, str], Tuple[float, QiWeIdentity]] = {}
        state_dir = _text(self.adapter.qiwe.state_dir)
        self._cache_path = Path(state_dir).expanduser() / "cache" / "identity.json" if state_dir else None
        self._load_cache_file()

    async def resolve(self, parsed: ParsedQiWeMessage) -> QiWeIdentity:
        sender_id = _text(parsed.sender_id)
        if not sender_id:
            return QiWeIdentity(user_id="", display_name="")
        webhook_name = _display_text(parsed.sender_name)
        if not self.adapter.qiwe.identity_lookup_enabled:
            return QiWeIdentity(user_id=sender_id, display_name=webhook_name, source="webhook" if webhook_name else "none")

        cache_key = (parsed.chat_id, sender_id)
        cached = self._cache.get(cache_key)
        now = time.time()
        ttl = max(1, self.adapter.qiwe.identity_cache_ttl_seconds)
        if cached and now - cached[0] <= ttl:
            return cached[1]

        if webhook_name:
            identity = QiWeIdentity(user_id=sender_id, display_name=webhook_name, source="webhook")
            self._cache[cache_key] = (now, identity)
            self._save_cache_file()
            return identity

        return QiWeIdentity(user_id=sender_id)

    def invalidate_room(self, room_id: str) -> None:
        room_id = _text(room_id)
        if not room_id:
            return
        for key in list(self._cache):
            if key[0] == room_id:
                self._cache.pop(key, None)
        self._save_cache_file()

    def _load_cache_file(self) -> None:
        if self._cache_path is None:
            return
        try:
            if not self._cache_path.exists():
                return
            payload = json.loads(self._cache_path.read_text(encoding="utf-8"))
            entries = payload.get("entries", []) if isinstance(payload, dict) else []
            if not isinstance(entries, list):
                return
            now = time.time()
            ttl = max(1, self.adapter.qiwe.identity_cache_ttl_seconds)
            for entry in entries:
                if not isinstance(entry, dict):
                    continue
                chat_id = _text(entry.get("chat_id"))
                user_id = _text(entry.get("user_id"))
                display_name = _display_text(entry.get("display_name"))
                source = _text(entry.get("source") or "persisted")
                updated_at = float(entry.get("updated_at") or 0)
                if not chat_id or not user_id or not display_name or now - updated_at > ttl:
                    continue
                self._cache[(chat_id, user_id)] = (updated_at, QiWeIdentity(user_id=user_id, display_name=display_name, source=source))
        except Exception as exc:
            logger.warning("[qiwe] identity cache load failed: %s", exc)

    def _save_cache_file(self) -> None:
        if self._cache_path is None:
            return
        try:
            now = time.time()
            ttl = max(1, self.adapter.qiwe.identity_cache_ttl_seconds)
            entries = []
            for (chat_id, user_id), (updated_at, identity) in self._cache.items():
                if now - updated_at > ttl or not identity.display_name:
                    continue
                entries.append(
                    {
                        "chat_id": chat_id,
                        "user_id": user_id,
                        "display_name": identity.display_name,
                        "source": identity.source,
                        "updated_at": updated_at,
                    }
                )
            payload = {
                "schema": "qintopia.qiwe.identity_cache.v1",
                "entries": entries,
            }
            self._cache_path.parent.mkdir(parents=True, exist_ok=True)
            tmp_path = self._cache_path.with_suffix(self._cache_path.suffix + f".{os.getpid()}.tmp")
            tmp_path.write_text(json.dumps(payload, ensure_ascii=False, sort_keys=True), encoding="utf-8")
            try:
                tmp_path.chmod(0o600)
            except OSError:
                pass
            tmp_path.replace(self._cache_path)
        except Exception as exc:
            logger.warning("[qiwe] identity cache save failed: %s", exc)

    async def _lookup_room_member(self, room_id: str, sender_id: str) -> QiWeIdentity:
        response = await self.adapter._call_qiwe_api(
            "/room/batchGetRoomDetail",
            {"roomIdList": [str(room_id)]},
            require_send_enabled=False,
        )
        if not response.success or not isinstance(response.raw_response, dict):
            return QiWeIdentity(user_id=sender_id)
        room_list = _first_mapping(response.raw_response.get("data")).get("roomList", [])
        if not isinstance(room_list, list):
            return QiWeIdentity(user_id=sender_id)
        for room in room_list:
            if not isinstance(room, dict):
                continue
            for member in room.get("memberList", []) if isinstance(room.get("memberList"), list) else []:
                if isinstance(member, dict) and _text(member.get("userId")) == sender_id:
                    name = _display_text(member.get("name")) or _display_text(member.get("roomRemarkName"))
                    if name:
                        return QiWeIdentity(user_id=sender_id, display_name=name, source="room_member")
        return QiWeIdentity(user_id=sender_id)

    async def _lookup_contact(self, sender_id: str) -> QiWeIdentity:
        response = await self.adapter._call_qiwe_api(
            "/contact/batchGetUserinfo",
            {"userIdList": [str(sender_id)]},
            require_send_enabled=False,
        )
        if not response.success or not isinstance(response.raw_response, dict):
            return QiWeIdentity(user_id=sender_id)
        contact_list = _first_mapping(response.raw_response.get("data")).get("contactList", [])
        if not isinstance(contact_list, list):
            return QiWeIdentity(user_id=sender_id)
        for contact in contact_list:
            if not isinstance(contact, dict) or _text(contact.get("userId")) != sender_id:
                continue
            name = _display_text(contact.get("nickname")) or _display_text(contact.get("realName"))
            if name:
                return QiWeIdentity(user_id=sender_id, display_name=name, source="contact")
        return QiWeIdentity(user_id=sender_id)


class QiWeAuditor:
    def __init__(self, state_dir: str, enabled: bool):
        self.enabled = enabled and bool(_text(state_dir))
        self.path = Path(state_dir).expanduser() / "audit" / "qiwe.jsonl" if self.enabled else None

    def record(self, parsed: ParsedQiWeMessage, *, decision: str, identity: Optional[QiWeIdentity] = None, outbound: Optional[Dict[str, Any]] = None) -> None:
        if not self.enabled or self.path is None:
            return
        payload = {
            "schema": "qintopia.channel.audit.v1",
            "adapter": "qiwe",
            "profile": "erhua",
            "inbound_event_id": parsed.message_id,
            "conversation_id": parsed.chat_id,
            "conversation_type": parsed.conversation_type,
            "sender_id_hash": _hash_id(parsed.sender_id),
            "sender_display_name": identity.display_name if identity else "",
            "identity_source": identity.source if identity else "",
            "trigger": parsed.reason,
            "policy_decision": decision,
            "outbound": outbound or {},
            "created_at": datetime.now(timezone.utc).isoformat(),
        }
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            with self.path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(payload, ensure_ascii=False) + "\n")
        except Exception as exc:
            logger.warning("[qiwe] audit write failed: %s", exc)


def _hash_id(value: str) -> str:
    text = _text(value)
    if not text:
        return ""
    return "sha256:" + hashlib.sha256(text.encode("utf-8")).hexdigest()


class QiWeAdapter(BasePlatformAdapter):
    def __init__(self, config, content_parser=None):
        super().__init__(config=config, platform=Platform("qiwe"))
        self.qiwe = self._load_config(config)
        self._runner = None
        self._seen_messages: Dict[str, float] = {}
        self._dispatch_tasks: set[asyncio.Task] = set()
        self._contact_guard_cache: Dict[str, Tuple[float, QiWeContactGuardDecision]] = {}
        self._sender_display_names: Dict[Tuple[str, str], str] = {}
        self._identity_resolver = QiWeIdentityResolver(self)
        self._auditor = QiWeAuditor(self.qiwe.state_dir, self.qiwe.audit_enabled)
        self._nats_capture = self._build_nats_capture()
        self._passive_pipeline = PassiveEventPipeline(
            PassivePipelineConfig(
                enabled=self.qiwe.pipeline_enabled,
                passive_enabled=self.qiwe.passive_pipeline_enabled,
                solitaire_enabled=self.qiwe.solitaire_processor_enabled,
                state_dir=self.qiwe.state_dir,
                allowed_groups=self.qiwe.passive_allowed_groups,
            ),
            content_parser=content_parser,
        )
        self._reminder_worker = ReminderWorker(
            ReminderWorkerConfig(
                enabled=self.qiwe.activity_reminder_enabled,
                dry_run=self.qiwe.activity_reminder_dry_run,
                scan_interval_seconds=self.qiwe.activity_reminder_scan_interval_seconds,
                allowed_groups=self.qiwe.activity_reminder_allowed_groups,
            ),
            self._passive_pipeline.activity_service,
            self._send_activity_reminder,
        )

    @staticmethod
    def _load_config(config) -> QiWeConfig:
        extra = getattr(config, "extra", {}) or {}
        names = _csv(os.getenv("QIWE_BOT_NAMES", "")) or _csv(extra.get("bot_names", "")) or ["二花"]
        direct_allowed_users = (
            _csv(os.getenv("QIWE_DIRECT_ALLOWED_USERS", ""))
            or _csv(extra.get("direct_allowed_users", ""))
            or _csv(os.getenv("QIWE_ALLOWED_USERS", ""))
        )
        return QiWeConfig(
            api_url=os.getenv("QIWE_API_URL") or extra.get("api_url", DEFAULT_API_URL),
            token=os.getenv("QIWE_TOKEN") or extra.get("token", ""),
            guid=os.getenv("QIWE_GUID") or extra.get("guid", ""),
            bot_user_id=os.getenv("QIWE_BOT_USER_ID") or os.getenv("QIWE_NODE_USER_ID") or extra.get("bot_user_id", ""),
            bot_names=names,
            webhook_host=os.getenv("QIWE_WEBHOOK_HOST") or extra.get("host", DEFAULT_WEBHOOK_HOST),
            webhook_port=_int(os.getenv("QIWE_WEBHOOK_PORT") or extra.get("port"), DEFAULT_WEBHOOK_PORT),
            webhook_path=os.getenv("QIWE_WEBHOOK_PATH") or extra.get("webhook_path", DEFAULT_WEBHOOK_PATH),
            max_body_bytes=_int(os.getenv("QIWE_MAX_BODY_BYTES") or extra.get("max_body_bytes"), DEFAULT_MAX_BODY_BYTES),
            max_reply_chars=_int(os.getenv("QIWE_MAX_REPLY_CHARS") or extra.get("max_reply_chars"), DEFAULT_MAX_REPLY_CHARS),
            dedupe_ttl_seconds=_int(os.getenv("QIWE_DEDUPE_TTL_SECONDS") or extra.get("dedupe_ttl_seconds"), DEFAULT_DEDUPE_TTL_SECONDS),
            mention_sender=_bool(os.getenv("QIWE_MENTION_SENDER") if "QIWE_MENTION_SENDER" in os.environ else extra.get("mention_sender"), True),
            is_no_need_read=_bool(os.getenv("QIWE_IS_NO_NEED_READ") if "QIWE_IS_NO_NEED_READ" in os.environ else extra.get("is_no_need_read"), True),
            send_enabled=_bool(os.getenv("QIWE_SEND_ENABLED") if "QIWE_SEND_ENABLED" in os.environ else extra.get("send_enabled"), True),
            direct_enabled=_bool(os.getenv("QIWE_DIRECT_ENABLED") if "QIWE_DIRECT_ENABLED" in os.environ else extra.get("direct_enabled"), True),
            direct_allow_all=_bool(os.getenv("QIWE_DIRECT_ALLOW_ALL") if "QIWE_DIRECT_ALLOW_ALL" in os.environ else extra.get("direct_allow_all"), False),
            direct_allowed_users=direct_allowed_users,
            location_tool_dedupe_ttl_seconds=_int(
                os.getenv("QIWE_LOCATION_DEDUPE_TTL_SECONDS") or extra.get("location_tool_dedupe_ttl_seconds"),
                DEFAULT_LOCATION_TOOL_DEDUPE_TTL_SECONDS,
            ),
            direct_tool_dedupe_ttl_seconds=_int(
                os.getenv("QIWE_DIRECT_TOOL_DEDUPE_TTL_SECONDS") or extra.get("direct_tool_dedupe_ttl_seconds"),
                DEFAULT_DIRECT_TOOL_DEDUPE_TTL_SECONDS,
            ),
            rich_message_tool_dedupe_ttl_seconds=_int(
                os.getenv("QIWE_RICH_MESSAGE_DEDUPE_TTL_SECONDS") or extra.get("rich_message_tool_dedupe_ttl_seconds"),
                DEFAULT_RICH_MESSAGE_TOOL_DEDUPE_TTL_SECONDS,
            ),
            revoke_message_tool_dedupe_ttl_seconds=_int(
                os.getenv("QIWE_REVOKE_MESSAGE_DEDUPE_TTL_SECONDS") or extra.get("revoke_message_tool_dedupe_ttl_seconds"),
                DEFAULT_REVOKE_MESSAGE_TOOL_DEDUPE_TTL_SECONDS,
            ),
            voice_to_text_tool_dedupe_ttl_seconds=_int(
                os.getenv("QIWE_VOICE_TO_TEXT_DEDUPE_TTL_SECONDS") or extra.get("voice_to_text_tool_dedupe_ttl_seconds"),
                DEFAULT_VOICE_TO_TEXT_TOOL_DEDUPE_TTL_SECONDS,
            ),
            human_handoff_tool_dedupe_ttl_seconds=_int(
                os.getenv("QIWE_HUMAN_HANDOFF_DEDUPE_TTL_SECONDS") or extra.get("human_handoff_tool_dedupe_ttl_seconds"),
                DEFAULT_HUMAN_HANDOFF_TOOL_DEDUPE_TTL_SECONDS,
            ),
            human_handoff_enabled=_bool(
                os.getenv("QIWE_HUMAN_HANDOFF_ENABLED") if "QIWE_HUMAN_HANDOFF_ENABLED" in os.environ else extra.get("human_handoff_enabled"),
                False,
            ),
            human_handoff_group_map=(
                _human_handoff_group_map(os.getenv("QIWE_HUMAN_HANDOFF_GROUPS_JSON", ""))
                or _human_handoff_group_map(extra.get("human_handoff_group_map"))
            ),
            human_handoff_user_id=os.getenv("QIWE_HUMAN_HANDOFF_USER_ID") or extra.get("human_handoff_user_id", ""),
            human_handoff_display_name=os.getenv("QIWE_HUMAN_HANDOFF_DISPLAY_NAME") or extra.get("human_handoff_display_name", "秦托邦小客服"),
            contact_request_tool_dedupe_ttl_seconds=_int(
                os.getenv("QIWE_CONTACT_REQUEST_DEDUPE_TTL_SECONDS") or extra.get("contact_request_tool_dedupe_ttl_seconds"),
                DEFAULT_CONTACT_REQUEST_TOOL_DEDUPE_TTL_SECONDS,
            ),
            contact_guard_enabled=_bool(
                os.getenv("QIWE_CONTACT_GUARD_ENABLED") if "QIWE_CONTACT_GUARD_ENABLED" in os.environ else extra.get("contact_guard_enabled"),
                True,
            ),
            contact_guard_cache_ttl_seconds=_int(
                os.getenv("QIWE_CONTACT_GUARD_CACHE_TTL_SECONDS") or extra.get("contact_guard_cache_ttl_seconds"),
                DEFAULT_CONTACT_GUARD_CACHE_TTL_SECONDS,
            ),
            contact_guard_page_limit=_int(
                os.getenv("QIWE_CONTACT_GUARD_PAGE_LIMIT") or extra.get("contact_guard_page_limit"),
                DEFAULT_CONTACT_GUARD_PAGE_LIMIT,
            ),
            contact_guard_max_pages=_int(
                os.getenv("QIWE_CONTACT_GUARD_MAX_PAGES") or extra.get("contact_guard_max_pages"),
                DEFAULT_CONTACT_GUARD_MAX_PAGES,
            ),
            identity_lookup_enabled=_bool(
                os.getenv("QIWE_IDENTITY_LOOKUP_ENABLED") if "QIWE_IDENTITY_LOOKUP_ENABLED" in os.environ else extra.get("identity_lookup_enabled"),
                True,
            ),
            identity_cache_ttl_seconds=_int(
                os.getenv("QIWE_IDENTITY_CACHE_TTL_SECONDS") or extra.get("identity_cache_ttl_seconds"),
                DEFAULT_IDENTITY_CACHE_TTL_SECONDS,
            ),
            state_dir=os.getenv("QIWE_STATE_DIR") or extra.get("state_dir", ""),
            audit_enabled=_bool(os.getenv("QIWE_AUDIT_ENABLED") if "QIWE_AUDIT_ENABLED" in os.environ else extra.get("audit_enabled"), False),
            voice_to_text_enabled=_bool(
                os.getenv("QIWE_VOICE_TO_TEXT_ENABLED") if "QIWE_VOICE_TO_TEXT_ENABLED" in os.environ else extra.get("voice_to_text_enabled"),
                False,
            ),
            voice_to_text_poll_attempts=_int(os.getenv("QIWE_VOICE_TO_TEXT_POLL_ATTEMPTS") or extra.get("voice_to_text_poll_attempts"), 5),
            voice_to_text_poll_interval_seconds=float(
                os.getenv("QIWE_VOICE_TO_TEXT_POLL_INTERVAL_SECONDS") or extra.get("voice_to_text_poll_interval_seconds") or 0.5
            ),
            pipeline_enabled=_bool(
                os.getenv("QIWE_PIPELINE_ENABLED") if "QIWE_PIPELINE_ENABLED" in os.environ else extra.get("pipeline_enabled"),
                False,
            ),
            passive_pipeline_enabled=_bool(
                os.getenv("QIWE_PASSIVE_PIPELINE_ENABLED") if "QIWE_PASSIVE_PIPELINE_ENABLED" in os.environ else extra.get("passive_pipeline_enabled"),
                False,
            ),
            solitaire_processor_enabled=_bool(
                os.getenv("QIWE_SOLITAIRE_PROCESSOR_ENABLED") if "QIWE_SOLITAIRE_PROCESSOR_ENABLED" in os.environ else extra.get("solitaire_processor_enabled"),
                False,
            ),
            passive_allowed_groups=(
                _csv(os.getenv("QIWE_PASSIVE_ALLOWED_GROUPS", ""))
                or _csv(extra.get("passive_allowed_groups", ""))
            ),
            passive_ack_enabled=_bool(
                os.getenv("QIWE_PASSIVE_ACK_ENABLED") if "QIWE_PASSIVE_ACK_ENABLED" in os.environ else extra.get("passive_ack_enabled"),
                False,
            ),
            passive_ack_allowed_groups=(
                _csv(os.getenv("QIWE_PASSIVE_ACK_ALLOWED_GROUPS", ""))
                or _csv(extra.get("passive_ack_allowed_groups", ""))
            ),
            active_attachment_preprocess_enabled=_bool(
                os.getenv("QIWE_ACTIVE_ATTACHMENT_PREPROCESS_ENABLED")
                if "QIWE_ACTIVE_ATTACHMENT_PREPROCESS_ENABLED" in os.environ
                else extra.get("active_attachment_preprocess_enabled"),
                False,
            ),
            activity_reminder_enabled=_bool(
                os.getenv("QIWE_ACTIVITY_REMINDER_ENABLED") if "QIWE_ACTIVITY_REMINDER_ENABLED" in os.environ else extra.get("activity_reminder_enabled"),
                False,
            ),
            activity_reminder_dry_run=_bool(
                os.getenv("QIWE_ACTIVITY_REMINDER_DRY_RUN") if "QIWE_ACTIVITY_REMINDER_DRY_RUN" in os.environ else extra.get("activity_reminder_dry_run"),
                True,
            ),
            activity_reminder_scan_interval_seconds=_int(
                os.getenv("QIWE_ACTIVITY_REMINDER_SCAN_INTERVAL_SECONDS") or extra.get("activity_reminder_scan_interval_seconds"),
                60,
            ),
            activity_reminder_allowed_groups=(
                _csv(os.getenv("QIWE_ACTIVITY_REMINDER_ALLOWED_GROUPS", ""))
                or _csv(extra.get("activity_reminder_allowed_groups", ""))
            ),
            nats_capture_enabled=_bool(
                os.getenv("QIWE_NATS_CAPTURE_ENABLED") if "QIWE_NATS_CAPTURE_ENABLED" in os.environ else extra.get("nats_capture_enabled"),
                False,
            ),
            nats_url=os.getenv("QIWE_NATS_URL") or extra.get("nats_url", "nats://127.0.0.1:4222"),
            nats_raw_subject=os.getenv("QIWE_NATS_RAW_SUBJECT") or extra.get("nats_raw_subject", "qintopia.qiwe.raw"),
            nats_message_subject=os.getenv("QIWE_NATS_MESSAGE_SUBJECT") or extra.get("nats_message_subject", "qintopia.qiwe.message"),
            nats_capture_timeout_seconds=float(
                os.getenv("QIWE_NATS_CAPTURE_TIMEOUT_SECONDS")
                or extra.get("nats_capture_timeout_seconds")
                or 0.5
            ),
            answer_context_prepare_enabled=_bool(
                os.getenv("QIWE_ANSWER_CONTEXT_PREPARE_ENABLED")
                if "QIWE_ANSWER_CONTEXT_PREPARE_ENABLED" in os.environ
                else extra.get("answer_context_prepare_enabled"),
                True,
            ),
            answer_context_mcp_command=os.getenv("QIWE_ANSWER_CONTEXT_MCP_COMMAND")
            or extra.get("answer_context_mcp_command", DEFAULT_ANSWER_CONTEXT_MCP_COMMAND),
            answer_context_prepare_timeout_seconds=float(
                os.getenv("QIWE_ANSWER_CONTEXT_PREPARE_TIMEOUT_SECONDS")
                or extra.get("answer_context_prepare_timeout_seconds")
                or 1.2
            ),
        )

    def _build_nats_capture(self) -> Optional[QiWeNatsPublisher]:
        if not self.qiwe.nats_capture_enabled:
            return None
        try:
            return QiWeNatsPublisher(
                QiWeNatsCaptureConfig(
                    enabled=True,
                    url=self.qiwe.nats_url,
                    raw_subject=self.qiwe.nats_raw_subject,
                    message_subject=self.qiwe.nats_message_subject,
                    timeout_seconds=self.qiwe.nats_capture_timeout_seconds,
                )
            )
        except Exception as exc:
            logger.warning("[qiwe] NATS capture disabled after config error: %s", exc)
            return None

    async def connect(self) -> bool:
        if not AIOHTTP_AVAILABLE:
            logger.error("[qiwe] aiohttp is not installed")
            self._set_fatal_error("missing_dependency", "aiohttp is not installed", retryable=False)
            return False
        if not self.qiwe.token:
            logger.error("[qiwe] QIWE_TOKEN is required")
            self._set_fatal_error("config_missing", "QIWE_TOKEN is required", retryable=False)
            return False

        app = web.Application(client_max_size=self.qiwe.max_body_bytes)
        app.router.add_get("/health", self._handle_health)
        app.router.add_post(self.qiwe.webhook_path, self._handle_webhook)

        self._runner = web.AppRunner(app)
        await self._runner.setup()
        site = web.TCPSite(self._runner, self.qiwe.webhook_host, self.qiwe.webhook_port)
        try:
            await site.start()
        except OSError as exc:
            logger.error("[qiwe] failed to listen on %s:%s: %s", self.qiwe.webhook_host, self.qiwe.webhook_port, exc)
            await self._runner.cleanup()
            self._runner = None
            self._set_fatal_error("listen_failed", str(exc), retryable=True)
            return False

        self._mark_connected()
        self._reminder_worker.start()
        logger.info("[qiwe] listening on %s:%s%s", self.qiwe.webhook_host, self.qiwe.webhook_port, self.qiwe.webhook_path)
        return True

    async def disconnect(self) -> None:
        await self._reminder_worker.stop()
        if self._runner:
            await self._runner.cleanup()
            self._runner = None
        for task in list(self._dispatch_tasks):
            task.cancel()
        self._mark_disconnected()

    async def send(
        self,
        chat_id: str,
        content: str,
        reply_to: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> SendResult:
        metadata = metadata or {}
        if _is_no_reply_sentinel(content):
            logger.info("[qiwe] NO_REPLY sentinel skipped chat_id=%s", chat_id)
            return SendResult(success=True, raw_response={"skipped": "NO_REPLY"})
        if _is_internal_process_message(content):
            logger.warning("[qiwe] internal process message skipped chat_id=%s", chat_id)
            return SendResult(success=True, raw_response={"skipped": INTERNAL_PROCESS_SKIP_REASON})
        sender_id = str(metadata.get("sender_id") or metadata.get("qiwe_sender_id") or "").strip()
        thread_id = str(metadata.get("thread_id") or "").strip()
        if not sender_id and thread_id.startswith("user:"):
            sender_id = thread_id.removeprefix("user:")
        guid = str(metadata.get("guid") or self.qiwe.guid or "").strip()
        location_card = metadata.get("location_card") or metadata.get("qiwe_location_card")
        home_group = str(os.getenv("QIWE_HOME_GROUP") or "").strip()
        is_group = bool(
            sender_id
            or metadata.get("chat_type") == "group"
            or metadata.get("conversation_type") == "group"
            or (home_group and str(chat_id).strip() == home_group)
        )
        sender_display_names = self._sender_display_name_candidates(chat_id, sender_id, metadata)
        if isinstance(location_card, dict):
            return await self._send_location_bundle(
                chat_id,
                content,
                location_card,
                sender_id=sender_id,
                sender_display_names=sender_display_names,
                guid=guid,
                is_group=is_group,
            )
        if not is_group:
            guard_result = await self._ensure_direct_recipient_sendable(chat_id, guid=guid)
            if not guard_result.success:
                return guard_result
        body = self._build_send_body(
            chat_id,
            content,
            sender_id=sender_id,
            sender_display_names=sender_display_names,
            guid=guid,
            is_group=is_group,
            reply_ref=metadata.get("qiwe_reply_ref") if isinstance(metadata.get("qiwe_reply_ref"), dict) else None,
        )
        return await self._post_qiwe_body(body)

    async def send_typing(self, chat_id: str, metadata=None) -> None:
        return None

    async def get_chat_info(self, chat_id: str) -> Dict[str, Any]:
        return {"name": str(chat_id), "type": "chat", "chat_id": str(chat_id)}

    async def _handle_health(self, request: "web.Request") -> "web.Response":
        return web.json_response({"status": "ok", "platform": "qiwe"})

    async def _handle_webhook(self, request: "web.Request") -> "web.Response":
        body = await request.read()
        if len(body) > self.qiwe.max_body_bytes:
            return web.json_response({"ok": False, "reason": "body_too_large"}, status=413)
        try:
            parsed = parse_qiwe_payload(
                body,
                bot_names=self.qiwe.bot_names,
                bot_user_id=self.qiwe.bot_user_id,
                direct_enabled=self.qiwe.direct_enabled,
                direct_allow_all=self.qiwe.direct_allow_all,
                direct_allowed_users=self.qiwe.direct_allowed_users,
                active_attachment_preprocess_enabled=self.qiwe.active_attachment_preprocess_enabled,
            )
        except json.JSONDecodeError as exc:
            logger.warning("[qiwe] invalid JSON webhook body: %s", exc)
            return web.json_response({"ok": False, "reason": "invalid_json"}, status=400)
        except Exception as exc:
            logger.warning("[qiwe] failed to parse webhook body: %s", exc, exc_info=True)
            return web.json_response({"ok": False, "reason": "parse_failed"}, status=400)

        if parsed.group_id_mismatch:
            logger.warning(
                "[qiwe] group_id_mismatch outer_fromGroup=%s inner_fromRoomId=%s message_id=%s",
                parsed.outer_group_id,
                parsed.group_id,
                parsed.message_id,
            )

        self._schedule_nats_capture(parsed, body)

        if not parsed.accepted:
            logger.info("[qiwe] ignored webhook reason=%s message_id=%s", parsed.reason, parsed.message_id)
            self._auditor.record(parsed, decision="ignored")
            return web.json_response({"ok": True, "accepted": False, "reason": parsed.reason})
        _store_recent_message_context(parsed)
        if not parsed.should_trigger:
            logger.debug("[qiwe] accepted non-mention message_id=%s group_id=%s", parsed.message_id, parsed.group_id)
            self._auditor.record(parsed, decision="accepted_no_trigger")
            self._schedule_passive_pipeline(parsed)
            return web.json_response({"ok": True, "accepted": True, "triggered": False, "reason": parsed.reason})
        if self._is_duplicate(parsed.message_id):
            logger.info("[qiwe] duplicate webhook ignored message_id=%s", parsed.message_id)
            self._auditor.record(parsed, decision="duplicate")
            return web.json_response({"ok": True, "accepted": True, "triggered": False, "reason": "duplicate_message"})

        self._auditor.record(parsed, decision="dispatch_scheduled")
        task = asyncio.create_task(self._dispatch_message_safe(parsed))
        self._dispatch_tasks.add(task)
        task.add_done_callback(self._dispatch_tasks.discard)
        return web.json_response({"ok": True, "accepted": True, "triggered": True, "message_id": parsed.message_id})

    def _schedule_nats_capture(self, parsed: ParsedQiWeMessage, body: bytes) -> None:
        if self._nats_capture is None:
            return
        task = asyncio.create_task(self._capture_message_safe(parsed, body))
        self._dispatch_tasks.add(task)
        task.add_done_callback(self._dispatch_tasks.discard)

    async def _capture_message_safe(self, parsed: ParsedQiWeMessage, body: bytes) -> None:
        identity: Optional[QiWeIdentity] = None
        if parsed.sender_id:
            try:
                identity = await self._identity_resolver.resolve(parsed)
            except Exception as exc:
                logger.warning("[qiwe] NATS capture identity resolve failed message_id=%s: %s", parsed.message_id, exc, exc_info=True)
        try:
            raw_event, message_event, message_id = build_capture_events(parsed, body, identity=identity)
        except Exception as exc:
            logger.warning("[qiwe] NATS capture payload build failed message_id=%s: %s", parsed.message_id, exc, exc_info=True)
            return
        await self._publish_nats_capture_safe(raw_event, message_event, message_id)

    async def _publish_nats_capture_safe(
        self,
        raw_event: Dict[str, Any],
        message_event: Dict[str, Any],
        message_id: str,
    ) -> None:
        if self._nats_capture is None:
            return
        try:
            await self._nats_capture.publish_capture(raw_event, message_event, message_id=message_id)
            logger.debug("[qiwe] NATS capture published message_id=%s", message_id)
        except Exception as exc:
            logger.warning("[qiwe] NATS capture publish failed message_id=%s: %s", message_id, exc, exc_info=True)

    async def _dispatch_message_safe(self, parsed: ParsedQiWeMessage) -> None:
        try:
            await self._dispatch_message(parsed)
        except Exception as exc:
            logger.warning("[qiwe] dispatch failed message_id=%s: %s", parsed.message_id, exc, exc_info=True)
            self._auditor.record(parsed, decision="dispatch_failed")

    def _schedule_passive_pipeline(self, parsed: ParsedQiWeMessage) -> None:
        if not self._passive_pipeline.enabled:
            return
        task = asyncio.create_task(self._passive_pipeline_safe(parsed))
        self._dispatch_tasks.add(task)
        task.add_done_callback(self._dispatch_tasks.discard)

    async def _passive_pipeline_safe(self, parsed: ParsedQiWeMessage) -> None:
        try:
            result = await self._passive_pipeline.handle(normalized_event_from_parsed(parsed))
            if result is not None:
                await self._send_passive_ack(parsed, result)
        except Exception as exc:
            logger.warning("[qiwe] passive pipeline failed message_id=%s: %s", parsed.message_id, exc, exc_info=True)

    async def _send_passive_ack(self, parsed: ParsedQiWeMessage, result: Any) -> None:
        if not self.qiwe.passive_ack_enabled or not bool(getattr(result, "handled", False)):
            return
        if not bool(getattr(result, "is_new_activity", False)):
            return
        group_id = _text(parsed.group_id)
        allowed = set(self.qiwe.passive_ack_allowed_groups or self.qiwe.passive_allowed_groups)
        if allowed and group_id not in allowed:
            logger.debug("[qiwe] passive ack skipped group_id=%s message_id=%s", group_id, parsed.message_id)
            return
        text = self._passive_ack_text(result)
        if not text:
            return
        send_result = await self.send(group_id, text, metadata={"conversation_type": "group", "chat_type": "group"})
        if not send_result.success:
            logger.warning("[qiwe] passive ack send failed message_id=%s error=%s", parsed.message_id, send_result.error)
        else:
            logger.info(
                "[qiwe] passive ack sent message_id=%s group_id=%s activity_id=%s",
                parsed.message_id,
                group_id,
                _text(getattr(result, "activity_id", "")),
            )

    def _passive_ack_text(self, result: Any) -> str:
        subject = _text(getattr(result, "activity_subject", "")) or _text(getattr(result, "activity_id", "")) or "未命名活动"
        start_time = _text(getattr(result, "start_time", "")) or "未识别"
        time_note = _text(getattr(result, "time_normalization_note", ""))
        time_line = f"时间二花也记下了：{start_time}。" if start_time != "未识别" else "时间二花先记着，等接龙里补清楚了我再更新。"
        lines = [
            f"二花看到有活动啦：{subject}",
            time_line,
        ]
        if time_note:
            lines.append(time_note)
        if bool(getattr(result, "immediate_reminder", False)):
            lines.append("离开始时间已经不到 30 分钟啦，二花这里先轻轻提醒一下：要参加的朋友可以准备出发或收拾一下。")
        else:
            lines.append("我会帮大家盯着，活动开始前 30 分钟来群里提醒一声。")
        return "\n".join(lines)

    async def _send_activity_reminder(
        self,
        group_id: str,
        text: str,
        *,
        source_message_ref: Optional[Dict[str, Any]] = None,
    ) -> SendResult:
        metadata: Dict[str, Any] = {"conversation_type": "group", "chat_type": "group"}
        if source_message_ref:
            metadata["qiwe_reply_ref"] = source_message_ref
        return await self.send(group_id, text, metadata=metadata)

    async def _dispatch_message(self, parsed: ParsedQiWeMessage) -> None:
        chat_id = parsed.chat_id
        chat_type = "group" if parsed.conversation_type == "group" else "dm"
        parsed.referenced_message = _resolve_referenced_message(parsed)
        identity = await self._identity_resolver.resolve(parsed)
        if parsed.conversation_type == "group" and parsed.sender_id and identity.display_name:
            self._sender_display_names[(chat_id, parsed.sender_id)] = identity.display_name
        if parsed.conversation_type == "group" and parsed.sender_id:
            _remember_qiwe_message_ref(chat_id, parsed.sender_id, _qiwe_reply_ref_from_parsed(parsed))
        answer_context = await self._prepare_answer_context(parsed)
        source = self.build_source(
            chat_id=chat_id,
            chat_name=chat_id,
            chat_type=chat_type,
            user_id=parsed.sender_id or None,
            user_name=identity.display_name or None,
            # Hermes send metadata only carries thread_id, not source.user_id.
            # QiWe has no native thread here; this preserves sender mention
            # routing and keeps concurrent group sessions isolated per user.
            thread_id=f"user:{parsed.sender_id}" if parsed.conversation_type == "group" and parsed.sender_id else None,
            message_id=parsed.message_id,
        )
        dispatch_text = self._active_dispatch_text(parsed)
        event = MessageEvent(
            text=dispatch_text,
            message_type=MessageType.TEXT,
            source=source,
            raw_message={
                "payload": parsed.payload,
                "raw_event": parsed.raw_event,
                "message_kind": parsed.message_kind,
                "attachments": parsed.attachments,
                "referenced_message": parsed.referenced_message,
            },
            message_id=parsed.message_id,
            channel_prompt=_member_context_channel_prompt(parsed, identity, answer_context),
        )
        self._auditor.record(parsed, decision="dispatch", identity=identity)
        await self.handle_message(event)

    async def _prepare_answer_context(self, parsed: ParsedQiWeMessage) -> Optional[Dict[str, Any]]:
        if not self.qiwe.answer_context_prepare_enabled:
            return None
        command = _text(self.qiwe.answer_context_mcp_command)
        if not command:
            return None
        timeout = max(0.2, float(self.qiwe.answer_context_prepare_timeout_seconds or 1.2))
        try:
            process = await asyncio.create_subprocess_exec(
                *shlex.split(command),
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            request = _answer_context_mcp_request(
                chat_id=parsed.chat_id,
                sender_id=parsed.sender_id,
                message_text=_text(parsed.text)[:1000],
                mentioned_member_names=_mentioned_member_names_from_at_list(
                    parsed.at_list,
                    bot_user_id=self.qiwe.bot_user_id,
                    bot_names=self.qiwe.bot_names,
                ),
            )
            stdout, stderr = await asyncio.wait_for(process.communicate(request.encode("utf-8")), timeout=timeout)
        except asyncio.TimeoutError:
            logger.warning("[qiwe] answer context prepare timed out")
            return None
        except Exception as exc:
            logger.warning("[qiwe] answer context prepare failed error=%s", exc)
            return None
        if process.returncode != 0:
            logger.warning(
                "[qiwe] answer context prepare exited code=%s stderr=%s",
                process.returncode,
                stderr.decode("utf-8", errors="replace")[:300],
            )
            return None
        return _answer_context_from_mcp_stdout(stdout.decode("utf-8", errors="replace"))

    def _active_dispatch_text(self, parsed: ParsedQiWeMessage) -> str:
        reference_text = _referenced_message_text(parsed.referenced_message)
        if parsed.message_kind == "text":
            if reference_text:
                return f"{reference_text}\n\n当前消息：{parsed.text}"
            return parsed.text
        if not self.qiwe.active_attachment_preprocess_enabled:
            return parsed.text
        if parsed.message_kind == "solitaire":
            return parsed.text or "用户发送了一条接龙消息，但当前无法解析接龙内容。"
        if parsed.message_kind in {"voice", "image", "quote", "mixed"}:
            return f"用户发送了一条{parsed.message_kind}消息，但当前这个消息类型的识别能力尚未启用。请提示用户补充文字说明。"
        return parsed.text

    def _is_duplicate(self, message_id: str) -> bool:
        now = time.time()
        ttl = max(1, self.qiwe.dedupe_ttl_seconds)
        expired = [key for key, seen_at in self._seen_messages.items() if now - seen_at > ttl]
        for key in expired:
            self._seen_messages.pop(key, None)
        if not message_id:
            return False
        if message_id in self._seen_messages:
            return True
        self._seen_messages[message_id] = now
        return False

    def _sender_display_name_candidates(self, chat_id: str, sender_id: str, metadata: Dict[str, Any]) -> List[str]:
        candidates = [
            metadata.get("sender_display_name"),
            metadata.get("user_name"),
            metadata.get("qiwe_sender_name"),
            metadata.get("sender_name"),
        ]
        cached = self._sender_display_names.get((str(chat_id), str(sender_id))) if sender_id else ""
        if cached:
            candidates.append(cached)
        return _dedupe_texts(candidates)

    def _build_send_body(
        self,
        chat_id: str,
        content: str,
        *,
        sender_id: str = "",
        mention_user_ids: Optional[Iterable[str]] = None,
        sender_display_names: Optional[Iterable[str]] = None,
        guid: str = "",
        is_group: bool = True,
        reply_ref: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        reply = self._clamp_reply(content)
        if not is_group:
            body: Dict[str, Any] = {
                "method": "/msg/sendText",
                "params": {
                    "guid": guid,
                    "toId": str(chat_id),
                    "content": reply,
                    "isNoNeedRead": self.qiwe.is_no_need_read,
                },
            }
            if not guid:
                body["params"].pop("guid", None)
            return body
        body: Dict[str, Any] = {
            "method": "/msg/sendHyperText",
            "params": {
                "guid": guid,
                "toId": str(chat_id),
                "isNoNeedRead": self.qiwe.is_no_need_read,
                "content": [],
            },
        }
        segments = body["params"]["content"]
        if self.qiwe.mention_sender and sender_id:
            segments.append({"subtype": 1, "text": str(sender_id)})
            reply = _strip_redundant_sender_prefix(reply, sender_id=sender_id, sender_display_names=sender_display_names)
            reply = f" {reply}"
        for user_id in _dedupe_texts(mention_user_ids or []):
            if user_id and user_id != sender_id:
                segments.append({"subtype": 1, "text": str(user_id)})
                if not reply.startswith((" ", "\n")):
                    reply = f" {reply}"
        segments.append({"subtype": 0, "text": reply})
        reply_payload = _build_qiwe_reply_payload(reply_ref)
        if reply_payload:
            body["params"]["reply"] = reply_payload
        if not guid:
            body["params"].pop("guid", None)
        return body

    def _build_location_body(self, chat_id: str, location_card: Dict[str, Any], *, guid: str = "") -> Dict[str, Any]:
        title = _text(location_card.get("title") or location_card.get("name"))
        address = _text(location_card.get("address") or title)
        latitude = location_card.get("latitude") if location_card.get("latitude") is not None else location_card.get("lat")
        longitude = location_card.get("longitude") if location_card.get("longitude") is not None else location_card.get("lng")
        if not title or latitude is None or longitude is None:
            raise ValueError("location_card requires title/name, latitude, and longitude")
        body: Dict[str, Any] = {
            "method": "/msg/sendLocation",
            "params": {
                "guid": guid,
                "toId": str(chat_id),
                "title": title,
                "address": address,
                "latitude": latitude,
                "longitude": longitude,
            },
        }
        if not guid:
            body["params"].pop("guid", None)
        return body

    def _build_rich_message_body(self, message_type: str, chat_id: str, payload: Dict[str, Any], *, guid: str = "") -> Dict[str, Any]:
        kind = _normalize_rich_message_type(message_type)
        spec = QIWE_RICH_MESSAGE_SPECS.get(kind)
        if not spec:
            raise ValueError(f"unsupported rich message type: {message_type}")
        params: Dict[str, Any] = {"guid": guid or self.qiwe.guid, "toId": str(chat_id)}
        for api_name, aliases, required, field_type in spec["fields"]:
            value = _first_present(payload, aliases)
            if field_type == "text":
                value = _text(value)
                if required and not value:
                    raise ValueError(f"{aliases[0]} is required for {kind}")
                if value:
                    params[api_name] = value
            elif field_type == "int":
                if value in (None, ""):
                    if required:
                        raise ValueError(f"{aliases[0]} is required for {kind}")
                    continue
                try:
                    params[api_name] = int(value)
                except (TypeError, ValueError):
                    raise ValueError(f"{aliases[0]} must be numeric") from None
        if not params.get("guid"):
            params.pop("guid", None)
        return {"method": str(spec["method"]), "params": params}

    def _build_revoke_message_body(self, chat_id: str, msg_server_id: Any, *, guid: str = "") -> Dict[str, Any]:
        target_chat = _text(chat_id)
        if not target_chat:
            raise ValueError("chat_id is required")
        try:
            numeric_msg_id = int(msg_server_id)
        except (TypeError, ValueError):
            raise ValueError("msgServerId must be numeric") from None
        params: Dict[str, Any] = {
            "guid": guid or self.qiwe.guid,
            "chatId": target_chat,
            "msgServerId": numeric_msg_id,
        }
        if not params.get("guid"):
            params.pop("guid", None)
        return {"method": "/msg/revokeMsg", "params": params}

    async def _send_rich_message(
        self,
        message_type: str,
        chat_id: str,
        payload: Dict[str, Any],
        *,
        guid: str = "",
        is_group: bool = True,
    ) -> SendResult:
        if not is_group:
            guard_result = await self._ensure_direct_recipient_sendable(chat_id, guid=guid)
            if not guard_result.success:
                return guard_result
        try:
            body = self._build_rich_message_body(message_type, chat_id, payload, guid=guid)
        except ValueError as exc:
            return SendResult(success=False, error=str(exc))
        return await self._post_qiwe_body(body)

    async def _send_location_bundle(
        self,
        chat_id: str,
        content: str,
        location_card: Dict[str, Any],
        *,
        sender_id: str = "",
        sender_display_names: Optional[Iterable[str]] = None,
        guid: str = "",
        is_group: bool = True,
    ) -> SendResult:
        if not is_group:
            guard_result = await self._ensure_direct_recipient_sendable(chat_id, guid=guid)
            if not guard_result.success:
                return guard_result
        try:
            location_body = self._build_location_body(chat_id, location_card, guid=guid)
        except ValueError as exc:
            fallback = self._location_text_fallback(content, location_card)
            body = self._build_send_body(
                chat_id,
                fallback,
                sender_id=sender_id,
                sender_display_names=sender_display_names,
                guid=guid,
                is_group=is_group,
            )
            result = await self._post_qiwe_body(body)
            result.error = result.error or str(exc)
            return result

        location_result = await self._post_qiwe_body(location_body)
        if not location_result.success:
            fallback = self._location_text_fallback(content, location_card)
            body = self._build_send_body(
                chat_id,
                fallback,
                sender_id=sender_id,
                sender_display_names=sender_display_names,
                guid=guid,
                is_group=is_group,
            )
            fallback_result = await self._post_qiwe_body(body)
            fallback_result.raw_response = {
                "location": location_result.raw_response,
                "fallback": fallback_result.raw_response,
            }
            if not fallback_result.error:
                fallback_result.error = location_result.error
            return fallback_result

        text = _text(content)
        if text:
            text_body = self._build_send_body(
                chat_id,
                text,
                sender_id=sender_id,
                sender_display_names=sender_display_names,
                guid=guid,
                is_group=is_group,
            )
            text_result = await self._post_qiwe_body(text_body)
            return SendResult(
                success=location_result.success and text_result.success,
                error=text_result.error,
                raw_response={"location": location_result.raw_response, "text": text_result.raw_response},
                retryable=location_result.retryable or text_result.retryable,
            )
        return location_result

    def _location_text_fallback(self, content: str, location_card: Dict[str, Any]) -> str:
        text = _text(content)
        title = _text(location_card.get("title") or location_card.get("name") or "这个位置")
        address = _text(location_card.get("address"))
        latitude = location_card.get("latitude") if location_card.get("latitude") is not None else location_card.get("lat")
        longitude = location_card.get("longitude") if location_card.get("longitude") is not None else location_card.get("lng")
        parts = [text] if text else [f"我找到了「{title}」。"]
        if address:
            parts.append(f"地址：{address}")
        if latitude is not None and longitude is not None:
            parts.append(f"坐标：{latitude},{longitude}")
        return "\n".join(parts)

    def _clamp_reply(self, text: str) -> str:
        text = _text(text)
        if len(text) <= self.qiwe.max_reply_chars:
            return text
        return f"{text[: self.qiwe.max_reply_chars - 15]}\n...(truncated)"

    async def _post_qiwe_body(self, body: Dict[str, Any]) -> SendResult:
        if not self.qiwe.send_enabled:
            logger.info("[qiwe] send disabled toId=%s", body.get("params", {}).get("toId"))
            return SendResult(success=True, raw_response={"dryRun": True})
        return await self._call_qiwe_api(str(body.get("method") or ""), body.get("params") if isinstance(body.get("params"), dict) else {})

    async def _call_qiwe_api(self, method: str, params: Dict[str, Any], *, require_send_enabled: bool = True) -> SendResult:
        if require_send_enabled and not self.qiwe.send_enabled:
            logger.info("[qiwe] send disabled method=%s toId=%s", method, params.get("toId"))
            return SendResult(success=True, raw_response={"dryRun": True})
        if not self.qiwe.token:
            return SendResult(success=False, error="QIWE_TOKEN is not configured")

        body: Dict[str, Any] = {"method": method, "params": dict(params)}
        if self.qiwe.guid and not body["params"].get("guid"):
            body["params"]["guid"] = self.qiwe.guid
        if not body["params"].get("guid"):
            body["params"].pop("guid", None)

        timeout = ClientTimeout(total=15)
        try:
            async with ClientSession(timeout=timeout) as session:
                async with session.post(
                    self.qiwe.api_url,
                    headers={"content-type": "application/json", "x-qiwei-token": self.qiwe.token},
                    data=json.dumps(body, ensure_ascii=False),
                ) as response:
                    text = await response.text()
                    try:
                        parsed: Any = json.loads(text)
                    except json.JSONDecodeError:
                        parsed = text
                    if response.status < 200 or response.status >= 300:
                        return SendResult(success=False, error=f"QiWe HTTP {response.status}: {text}", raw_response=parsed, retryable=response.status >= 500)
                    if isinstance(parsed, dict) and parsed.get("code") not in (None, 0, 200):
                        return SendResult(success=False, error=f"QiWe business error: {parsed}", raw_response=parsed)
                    return SendResult(success=True, raw_response=parsed)
        except asyncio.TimeoutError:
            return SendResult(success=False, error="QiWe send timed out", retryable=True)
        except Exception as exc:
            logger.warning("[qiwe] send failed: %s", exc, exc_info=True)
            return SendResult(success=False, error=str(exc), retryable=True)

    async def _ensure_direct_recipient_sendable(self, user_id: str, *, guid: str = "") -> SendResult:
        recipient = _text(user_id)
        if not recipient:
            return SendResult(success=False, error="direct recipient userId is required")
        if not self.qiwe.contact_guard_enabled:
            return SendResult(success=True, raw_response={"contactGuard": "disabled"})
        if not self.qiwe.send_enabled:
            return SendResult(success=True, raw_response={"contactGuard": "dryRun"})

        now = time.time()
        ttl = max(1, self.qiwe.contact_guard_cache_ttl_seconds)
        cached = self._contact_guard_cache.get(recipient)
        if cached and now - cached[0] <= ttl:
            return self._contact_guard_decision_to_send_result(cached[1], cached=True)

        decision = await self._lookup_direct_recipient_contact_status(recipient, guid=guid)
        if not decision.retryable:
            self._contact_guard_cache[recipient] = (now, decision)
        return self._contact_guard_decision_to_send_result(decision, cached=False)

    def _contact_guard_decision_to_send_result(self, decision: QiWeContactGuardDecision, *, cached: bool) -> SendResult:
        raw = {
            "contactGuard": {
                "userId": decision.user_id,
                "allowed": decision.allowed,
                "contactType": decision.contact_type,
                "reason": decision.reason,
                "cached": cached,
            }
        }
        if decision.allowed:
            return SendResult(success=True, raw_response=raw)
        return SendResult(
            success=False,
            error=decision.reason or "QiWe direct recipient is not sendable",
            raw_response=raw,
            retryable=decision.retryable,
        )

    async def _lookup_direct_recipient_contact_status(self, user_id: str, *, guid: str = "") -> QiWeContactGuardDecision:
        current_seq = 0
        limit = max(1, self.qiwe.contact_guard_page_limit)
        max_pages = max(1, self.qiwe.contact_guard_max_pages)

        for _ in range(max_pages):
            result = await self._call_qiwe_api(
                "/contact/getWxContactList",
                {
                    "guid": guid or self.qiwe.guid,
                    "currentSeq": current_seq,
                    "limit": limit,
                    "bizType": 1,
                },
                require_send_enabled=False,
            )
            if not result.success:
                return QiWeContactGuardDecision(
                    allowed=False,
                    user_id=user_id,
                    reason=f"QiWe contact guard failed: {result.error or 'contact list request failed'}",
                    retryable=result.retryable,
                )
            data = _first_mapping(_first_mapping(result.raw_response).get("data")) if isinstance(result.raw_response, dict) else {}
            contact_list = data.get("contactList", [])
            if isinstance(contact_list, list):
                for contact in contact_list:
                    if not isinstance(contact, dict) or _text(contact.get("userId")) != user_id:
                        continue
                    contact_type = _int(contact.get("contactType"), -1)
                    if contact_type == QIWE_NORMAL_FRIEND_CONTACT_TYPE:
                        return QiWeContactGuardDecision(
                            allowed=True,
                            user_id=user_id,
                            contact_type=contact_type,
                            reason="normal_friend",
                        )
                    return QiWeContactGuardDecision(
                        allowed=False,
                        user_id=user_id,
                        contact_type=contact_type,
                        reason=f"QiWe direct recipient is not a normal friend contactType={contact_type}",
                    )

            if not data.get("hasMore"):
                break
            next_seq = data.get("currentSeq")
            if next_seq in (None, "") or next_seq == current_seq:
                break
            current_seq = next_seq

        return QiWeContactGuardDecision(
            allowed=False,
            user_id=user_id,
            reason="QiWe direct recipient was not found in external contacts",
        )

    async def _voice_to_text(self, msg_server_id: str, *, guid: str = "") -> SendResult:
        if not self.qiwe.voice_to_text_enabled:
            return SendResult(success=False, error="QiWe voice transcription is disabled")
        msg_id = _text(msg_server_id)
        if not msg_id:
            return SendResult(success=False, error="msgServerId is required")
        try:
            numeric_msg_id = int(msg_id)
        except ValueError:
            return SendResult(success=False, error="msgServerId must be numeric")

        apply_result = await self._call_qiwe_api(
            "/msg/voiceToTextApply",
            {"guid": guid or self.qiwe.guid, "msgServerId": numeric_msg_id},
            require_send_enabled=False,
        )
        if not apply_result.success:
            return apply_result
        apply_data = _first_mapping(_first_mapping(apply_result.raw_response).get("data")) if isinstance(apply_result.raw_response, dict) else {}
        voice_id = _text(apply_data.get("voiceId"))
        if not voice_id:
            return SendResult(success=False, error="QiWe voiceToTextApply did not return voiceId", raw_response=apply_result.raw_response)

        attempts = max(1, self.qiwe.voice_to_text_poll_attempts)
        interval = max(0.0, self.qiwe.voice_to_text_poll_interval_seconds)
        last_response: Any = None
        for attempt in range(attempts):
            if attempt and interval:
                await asyncio.sleep(interval)
            query_result = await self._call_qiwe_api(
                "/msg/voiceToTextQuery",
                {"guid": guid or self.qiwe.guid, "msgServerId": numeric_msg_id, "voiceId": voice_id},
                require_send_enabled=False,
            )
            if not query_result.success:
                return query_result
            last_response = query_result.raw_response
            data = _first_mapping(_first_mapping(query_result.raw_response).get("data")) if isinstance(query_result.raw_response, dict) else {}
            if data.get("isEnd"):
                return SendResult(success=True, raw_response={"voiceId": voice_id, "text": _text(data.get("text")), "raw_response": query_result.raw_response})
        return SendResult(success=False, error="QiWe voice transcription did not finish", raw_response=last_response, retryable=True)


def check_requirements() -> bool:
    return AIOHTTP_AVAILABLE


def check_tool_available() -> bool:
    return AIOHTTP_AVAILABLE and bool(os.getenv("QIWE_TOKEN", "").strip())


def validate_config(config) -> bool:
    extra = getattr(config, "extra", {}) or {}
    return AIOHTTP_AVAILABLE and bool(os.getenv("QIWE_TOKEN") or extra.get("token"))


def is_connected(config) -> bool:
    return validate_config(config)


def _env_enablement() -> Optional[Dict[str, Any]]:
    token = os.getenv("QIWE_TOKEN", "").strip()
    names = os.getenv("QIWE_BOT_NAMES", "").strip()
    if not token:
        return None
    seed: Dict[str, Any] = {"token": token}
    if names:
        seed["bot_names"] = names
    mappings = {
        "QIWE_API_URL": ("api_url", str),
        "QIWE_GUID": ("guid", str),
        "QIWE_BOT_USER_ID": ("bot_user_id", str),
        "QIWE_WEBHOOK_HOST": ("host", str),
        "QIWE_WEBHOOK_PATH": ("webhook_path", str),
        "QIWE_WEBHOOK_PORT": ("port", int),
        "QIWE_DEDUPE_TTL_SECONDS": ("dedupe_ttl_seconds", int),
        "QIWE_IDENTITY_CACHE_TTL_SECONDS": ("identity_cache_ttl_seconds", int),
        "QIWE_STATE_DIR": ("state_dir", str),
        "QIWE_LOCATION_DEDUPE_TTL_SECONDS": ("location_tool_dedupe_ttl_seconds", int),
        "QIWE_DIRECT_TOOL_DEDUPE_TTL_SECONDS": ("direct_tool_dedupe_ttl_seconds", int),
        "QIWE_RICH_MESSAGE_DEDUPE_TTL_SECONDS": ("rich_message_tool_dedupe_ttl_seconds", int),
        "QIWE_REVOKE_MESSAGE_DEDUPE_TTL_SECONDS": ("revoke_message_tool_dedupe_ttl_seconds", int),
        "QIWE_VOICE_TO_TEXT_DEDUPE_TTL_SECONDS": ("voice_to_text_tool_dedupe_ttl_seconds", int),
        "QIWE_HUMAN_HANDOFF_DEDUPE_TTL_SECONDS": ("human_handoff_tool_dedupe_ttl_seconds", int),
        "QIWE_HUMAN_HANDOFF_USER_ID": ("human_handoff_user_id", str),
        "QIWE_HUMAN_HANDOFF_DISPLAY_NAME": ("human_handoff_display_name", str),
        "QIWE_CONTACT_REQUEST_DEDUPE_TTL_SECONDS": ("contact_request_tool_dedupe_ttl_seconds", int),
        "QIWE_CONTACT_GUARD_CACHE_TTL_SECONDS": ("contact_guard_cache_ttl_seconds", int),
        "QIWE_CONTACT_GUARD_PAGE_LIMIT": ("contact_guard_page_limit", int),
        "QIWE_CONTACT_GUARD_MAX_PAGES": ("contact_guard_max_pages", int),
        "QIWE_VOICE_TO_TEXT_POLL_ATTEMPTS": ("voice_to_text_poll_attempts", int),
        "QIWE_ACTIVITY_REMINDER_SCAN_INTERVAL_SECONDS": ("activity_reminder_scan_interval_seconds", int),
    }
    for env_name, (key, caster) in mappings.items():
        value = os.getenv(env_name, "").strip()
        if not value:
            continue
        try:
            seed[key] = caster(value)
        except ValueError:
            logger.warning("[qiwe] ignoring invalid %s=%r", env_name, value)
    for env_name, key in {
        "QIWE_DIRECT_ENABLED": "direct_enabled",
        "QIWE_SEND_ENABLED": "send_enabled",
        "QIWE_MENTION_SENDER": "mention_sender",
        "QIWE_HUMAN_HANDOFF_ENABLED": "human_handoff_enabled",
        "QIWE_CONTACT_GUARD_ENABLED": "contact_guard_enabled",
        "QIWE_IDENTITY_LOOKUP_ENABLED": "identity_lookup_enabled",
        "QIWE_AUDIT_ENABLED": "audit_enabled",
        "QIWE_VOICE_TO_TEXT_ENABLED": "voice_to_text_enabled",
        "QIWE_PIPELINE_ENABLED": "pipeline_enabled",
        "QIWE_PASSIVE_PIPELINE_ENABLED": "passive_pipeline_enabled",
        "QIWE_SOLITAIRE_PROCESSOR_ENABLED": "solitaire_processor_enabled",
        "QIWE_PASSIVE_ACK_ENABLED": "passive_ack_enabled",
        "QIWE_ACTIVE_ATTACHMENT_PREPROCESS_ENABLED": "active_attachment_preprocess_enabled",
        "QIWE_ACTIVITY_REMINDER_ENABLED": "activity_reminder_enabled",
        "QIWE_ACTIVITY_REMINDER_DRY_RUN": "activity_reminder_dry_run",
    }.items():
        if env_name in os.environ:
            seed[key] = _bool(os.getenv(env_name), True)
    voice_interval = os.getenv("QIWE_VOICE_TO_TEXT_POLL_INTERVAL_SECONDS", "").strip()
    if voice_interval:
        try:
            seed["voice_to_text_poll_interval_seconds"] = float(voice_interval)
        except ValueError:
            logger.warning("[qiwe] ignoring invalid QIWE_VOICE_TO_TEXT_POLL_INTERVAL_SECONDS=%r", voice_interval)
    handoff_groups = os.getenv("QIWE_HUMAN_HANDOFF_GROUPS_JSON", "").strip()
    if handoff_groups:
        seed["human_handoff_group_map"] = _human_handoff_group_map(handoff_groups)
    direct_allowed = os.getenv("QIWE_DIRECT_ALLOWED_USERS", "").strip()
    if direct_allowed:
        seed["direct_allowed_users"] = direct_allowed
    passive_groups = os.getenv("QIWE_PASSIVE_ALLOWED_GROUPS", "").strip()
    if passive_groups:
        seed["passive_allowed_groups"] = passive_groups
    passive_ack_groups = os.getenv("QIWE_PASSIVE_ACK_ALLOWED_GROUPS", "").strip()
    if passive_ack_groups:
        seed["passive_ack_allowed_groups"] = passive_ack_groups
    reminder_groups = os.getenv("QIWE_ACTIVITY_REMINDER_ALLOWED_GROUPS", "").strip()
    if reminder_groups:
        seed["activity_reminder_allowed_groups"] = reminder_groups
    home = os.getenv("QIWE_HOME_GROUP", "").strip()
    if home:
        seed["home_channel"] = {"chat_id": home, "name": "QiWe Home"}
    return seed


async def _standalone_send(
    pconfig,
    chat_id: str,
    message: str,
    *,
    thread_id: Optional[str] = None,
    media_files: Optional[List[str]] = None,
    force_document: bool = False,
) -> Dict[str, Any]:
    adapter = QiWeAdapter(pconfig)
    metadata: Dict[str, Any] = {}
    if _text(chat_id) == _text(os.getenv("QIWE_HOME_GROUP")):
        metadata["conversation_type"] = "group"
    content = _strip_cron_delivery_wrapper(message)
    if _is_no_reply_sentinel(content) or _is_silent_sentinel(content):
        return {"success": True, "raw_response": {"skipped": content}}
    result = await adapter.send(chat_id=chat_id, content=content, metadata=metadata)
    if result.success:
        return {"success": True, "message_id": result.message_id, "raw_response": result.raw_response}
    return {"error": result.error or "QiWe standalone send failed", "raw_response": result.raw_response}


QIWE_SEND_LOCATION_CARD_SCHEMA = {
    "description": (
        "Send an approved structured location result as a native QiWe location "
        "card. Use only after Hermes business logic/GIS has selected a concrete "
        "location and confirmed the channel scope."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "chat_id": {
                "type": "string",
                "description": (
                    "QiWe group room id or direct sender id to receive the card. "
                    "Defaults to the current Hermes gateway chat when omitted."
                ),
            },
            "title": {"type": "string", "description": "Location card title."},
            "address": {"type": "string", "description": "Human-readable address or short place description."},
            "latitude": {"type": "number", "description": "WGS/GCJ latitude value accepted by QiWe."},
            "longitude": {"type": "number", "description": "WGS/GCJ longitude value accepted by QiWe."},
            "message": {
                "type": "string",
                "description": "Optional short text to send after the card, or fallback text if card delivery fails.",
            },
            "sender_id": {
                "type": "string",
                "description": "Original group sender id to mention in a group text bundle/fallback.",
            },
            "conversation_type": {
                "type": "string",
                "enum": ["group", "direct"],
                "description": "Channel type. Defaults to group when sender_id is present, otherwise direct.",
            },
            "guid": {"type": "string", "description": "Optional QiWe device guid override."},
            "idempotency_key": {
                "type": "string",
                "description": "Stable request key from the orchestrating skill to prevent duplicate sends.",
            },
        },
        "required": ["title", "latitude", "longitude"],
        "additionalProperties": False,
    },
}


QIWE_SEND_DIRECT_MESSAGE_SCHEMA = {
    "description": (
        "Send an approved private QiWe text message to one user. This is a "
        "controlled direct-message tool for workflows such as complaint intake "
        "detail collection and approved follow-up; it is not a group send API. "
        "If this returns a contactGuard failure for a group complaint follow-up, "
        "the approved next channel action is qiwe_request_direct_contact."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "recipient_user_id": {
                "type": "string",
                "description": "QiWe user id to receive the private message.",
            },
            "message": {
                "type": "string",
                "description": "Approved private message body.",
            },
            "guid": {"type": "string", "description": "Optional QiWe device guid override."},
            "idempotency_key": {
                "type": "string",
                "description": "Required stable key from the approved workflow for caller-side audit/dedupe.",
            },
            "purpose": {
                "type": "string",
                "description": "Required approved workflow purpose for this private message.",
            },
        },
        "required": ["recipient_user_id", "message", "idempotency_key", "purpose"],
        "additionalProperties": False,
    },
}


QIWE_HANDOFF_TO_HUMAN_SCHEMA = {
    "description": (
        "Quote the current QiWe group question and mention the configured human "
        "support account for manual answer. Use only when Erhua cannot answer "
        "with an authoritative source or the question requires live operations "
        "confirmation. This is not for complaints, refunds, contracts, privacy, "
        "or other high-risk workflows."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "chat_id": {
                "type": "string",
                "description": "QiWe group room id. Defaults to the current Hermes session chat.",
            },
            "message": {
                "type": "string",
                "description": "Short handoff text after mentioning human support.",
            },
            "original_sender_id": {
                "type": "string",
                "description": "QiWe sender id for the quoted source question. Defaults to the current Hermes session user.",
            },
            "original_content": {
                "type": "string",
                "description": "Original question text for the QiWe reply quote.",
            },
            "original_timestamp": {
                "type": "string",
                "description": "Original QiWe message timestamp for the reply quote.",
            },
            "original_msg_unique_identifier": {
                "type": "string",
                "description": "Original QiWe msgUniqueIdentifier for the reply quote.",
            },
            "guid": {"type": "string", "description": "Optional QiWe device guid override."},
            "idempotency_key": {
                "type": "string",
                "description": "Stable request key to prevent duplicate human handoff sends.",
            },
            "purpose": {
                "type": "string",
                "description": "Required handoff reason for audit and guardrails.",
            },
        },
        "required": ["message", "purpose"],
        "additionalProperties": False,
    },
}


QIWE_REQUEST_DIRECT_CONTACT_SCHEMA = {
    "description": (
        "Request direct QiWe contact with one approved user. This controlled "
        "channel tool only sends a friend verification request chosen by an "
        "orchestrating workflow; it does not search contacts or decide approval. "
        "In a current group session, room_member mode can resolve user_id from "
        "the current sender and room_id from the current group when omitted."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["room_member", "deleted_contact"],
                "description": (
                    "room_member uses /contact/addRoomContact; deleted_contact uses "
                    "/contact/addDeletedContact. Defaults to room_member only when "
                    "current session has distinct group and sender ids."
                ),
            },
            "user_id": {
                "type": "string",
                "description": "QiWe user id to request as a direct contact. Defaults to the current sender in a group session.",
            },
            "sender_id": {
                "type": "string",
                "description": "Optional alias for user_id when forwarding the current group sender id.",
            },
            "recipient_user_id": {
                "type": "string",
                "description": "Optional alias for user_id when following up after a direct-message guard failure.",
            },
            "verify_text": {
                "type": "string",
                "description": "Approved friend verification text from the orchestrating workflow.",
            },
            "purpose": {
                "type": "string",
                "description": "Required approved workflow purpose for audit and guardrails.",
            },
            "idempotency_key": {
                "type": "string",
                "description": "Required stable key from the approved workflow for caller-side audit/dedupe.",
            },
            "room_id": {
                "type": "string",
                "description": "QiWe room id. Required when mode is room_member; defaults to the current group in a group session.",
            },
            "chat_id": {
                "type": "string",
                "description": "Optional alias for room_id when forwarding the current group chat id.",
            },
            "guid": {"type": "string", "description": "Optional QiWe device guid override."},
        },
        "required": ["verify_text", "purpose", "idempotency_key"],
        "additionalProperties": False,
    },
}


QIWE_SEND_RICH_MESSAGE_SCHEMA = {
    "description": (
        "Send an approved QiWe rich/media/card message using the documented "
        "message endpoints for image, gif, file, voice, link, mini-program, "
        "or personal-card payloads. This is a whitelisted channel executor, "
        "not a generic QiWe API passthrough."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "chat_id": {
                "type": "string",
                "description": "QiWe group room id or direct user id to receive the message. Defaults to the current Hermes chat.",
            },
            "message_type": {
                "type": "string",
                "enum": ["image", "gif", "file", "voice", "link", "weapp", "personal_card"],
                "description": "Whitelisted QiWe rich message type.",
            },
            "payload": {
                "type": "object",
                "description": (
                    "Type-specific payload. image/file/voice use uploaded file credentials; "
                    "gif uses wxFileUrl; link uses title/linkUrl or link_url; weapp uses "
                    "appId/coverFile*/pagePath/thumbUrl/title/username; personal_card uses sharedId."
                ),
                "additionalProperties": True,
            },
            "conversation_type": {
                "type": "string",
                "enum": ["group", "direct"],
                "description": "Channel type. Defaults to group when current chat and sender differ, otherwise direct.",
            },
            "guid": {"type": "string", "description": "Optional QiWe device guid override."},
            "idempotency_key": {
                "type": "string",
                "description": "Required stable key from the approved workflow to prevent duplicate rich sends.",
            },
            "purpose": {
                "type": "string",
                "description": "Required approved workflow purpose for audit and guardrails.",
            },
        },
        "required": ["message_type", "payload", "idempotency_key", "purpose"],
        "additionalProperties": False,
    },
}


QIWE_REVOKE_MESSAGE_SCHEMA = {
    "description": (
        "Revoke one previously sent QiWe message using /msg/revokeMsg. Use only "
        "for approved correction/moderation flows and within QiWe's revoke window."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "chat_id": {
                "type": "string",
                "description": "QiWe group room id or direct user id containing the message. Defaults to the current Hermes chat.",
            },
            "msg_server_id": {
                "type": "integer",
                "description": "QiWe msgServerId of the message to revoke.",
            },
            "guid": {"type": "string", "description": "Optional QiWe device guid override."},
            "idempotency_key": {
                "type": "string",
                "description": "Required stable key from the approved workflow to prevent duplicate revoke calls.",
            },
            "purpose": {
                "type": "string",
                "description": "Required approved correction/moderation purpose.",
            },
        },
        "required": ["msg_server_id", "idempotency_key", "purpose"],
        "additionalProperties": False,
    },
}


QIWE_VOICE_TO_TEXT_SCHEMA = {
    "description": (
        "Run the controlled QiWe voice-to-text apply/query flow for one voice "
        "message. Requires QIWE_VOICE_TO_TEXT_ENABLED=true and returns only "
        "voiceId/text/completion state."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "msg_server_id": {
                "type": "integer",
                "description": "QiWe msgServerId of the voice message.",
            },
            "guid": {"type": "string", "description": "Optional QiWe device guid override."},
            "idempotency_key": {
                "type": "string",
                "description": "Required stable key from the approved workflow to prevent duplicate transcription calls.",
            },
            "purpose": {
                "type": "string",
                "description": "Required approved workflow purpose.",
            },
        },
        "required": ["msg_server_id", "idempotency_key", "purpose"],
        "additionalProperties": False,
    },
}


def _tool_json(data: Dict[str, Any]) -> str:
    return json.dumps(data, ensure_ascii=False)


def _tool_error(message: str, **extra: Any) -> str:
    payload = {"error": message}
    payload.update(extra)
    return _tool_json(payload)


def _tool_result(**kwargs: Any) -> str:
    return _tool_json(kwargs)


def _session_env(name: str) -> str:
    try:
        from gateway.session_context import get_session_env

        return _text(get_session_env(name, ""))
    except Exception:
        return _text(os.getenv(name, ""))


def _resolve_location_tool_args(args: Dict[str, Any]) -> Dict[str, Any]:
    resolved = dict(args)
    if not _text(resolved.get("chat_id")):
        resolved["chat_id"] = _session_env("HERMES_SESSION_CHAT_ID")
    if not _text(resolved.get("sender_id")):
        resolved["sender_id"] = _session_env("HERMES_SESSION_USER_ID")
    if not _text(resolved.get("conversation_type")):
        chat_id = _text(resolved.get("chat_id"))
        sender_id = _text(resolved.get("sender_id"))
        resolved["conversation_type"] = "group" if sender_id and chat_id and sender_id != chat_id else "direct"
    return resolved


def _resolve_channel_tool_args(args: Dict[str, Any]) -> Dict[str, Any]:
    resolved = dict(args)
    session_chat_id = _session_env("HERMES_SESSION_CHAT_ID")
    session_user_id = _session_env("HERMES_SESSION_USER_ID")
    if not _text(resolved.get("chat_id")):
        resolved["chat_id"] = session_chat_id
    if not _text(resolved.get("conversation_type")):
        chat_id = _text(resolved.get("chat_id"))
        resolved["conversation_type"] = "group" if session_user_id and chat_id and session_user_id != chat_id else "direct"
    if _text(resolved.get("conversation_type")) == "group":
        chat_id = _text(resolved.get("chat_id"))
        home_group = _text(os.getenv("QIWE_HOME_GROUP"))
        if home_group and (not chat_id or chat_id == session_user_id):
            resolved["chat_id"] = home_group
    return resolved


def _resolve_contact_request_tool_args(args: Dict[str, Any]) -> Dict[str, Any]:
    resolved = dict(args)
    if not _text(resolved.get("user_id")):
        resolved["user_id"] = (
            _text(resolved.get("sender_id"))
            or _text(resolved.get("recipient_user_id"))
            or _session_env("HERMES_SESSION_USER_ID")
        )
    if not _text(resolved.get("room_id")):
        resolved["room_id"] = _text(resolved.get("chat_id")) or _session_env("HERMES_SESSION_CHAT_ID")
    if not _text(resolved.get("mode")):
        user_id = _text(resolved.get("user_id"))
        room_id = _text(resolved.get("room_id"))
        if user_id and room_id and user_id != room_id:
            resolved["mode"] = "room_member"
    return resolved


def _session_raw_message() -> Dict[str, Any]:
    raw = _session_env("HERMES_SESSION_RAW_MESSAGE")
    if not raw:
        return {}
    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def _session_qiwe_raw_event() -> Dict[str, Any]:
    raw_message = _session_raw_message()
    raw_event = raw_message.get("raw_event")
    return raw_event if isinstance(raw_event, dict) else {}


def _qiwe_reply_ref_from_parsed(parsed: ParsedQiWeMessage) -> Dict[str, Any]:
    raw_event = parsed.raw_event if isinstance(parsed.raw_event, dict) else {}
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    content = _text(msg_data.get("content") or msg_data.get("title") or parsed.content or parsed.text)
    if not content:
        return {}
    ref: Dict[str, Any] = {
        "userId": parsed.sender_id or _text(raw_event.get("senderId")),
        "timeStamp": raw_event.get("timestamp"),
        "msgUniqueIdentifier": parsed.message_id or _text(raw_event.get("msgUniqueIdentifier")),
        "msgData": {"content": content},
    }
    return {key: value for key, value in ref.items() if value not in ("", None)}


def _remember_qiwe_message_ref(chat_id: str, sender_id: str, reply_ref: Dict[str, Any]) -> None:
    chat = _text(chat_id)
    sender = _text(sender_id)
    if not chat or not sender or not reply_ref:
        return
    now = time.time()
    expired = [key for key, (seen_at, _) in _RECENT_QIWE_MESSAGE_REFS.items() if now - seen_at > DEFAULT_RECENT_MESSAGE_REF_TTL_SECONDS]
    for key in expired:
        _RECENT_QIWE_MESSAGE_REFS.pop(key, None)
    _RECENT_QIWE_MESSAGE_REFS[(chat, sender)] = (now, dict(reply_ref))


def _recent_qiwe_message_ref(chat_id: str, sender_id: str) -> Dict[str, Any]:
    key = (_text(chat_id), _text(sender_id))
    if not key[0] or not key[1]:
        return {}
    entry = _RECENT_QIWE_MESSAGE_REFS.get(key)
    if not entry:
        return {}
    seen_at, reply_ref = entry
    if time.time() - seen_at > DEFAULT_RECENT_MESSAGE_REF_TTL_SECONDS:
        _RECENT_QIWE_MESSAGE_REFS.pop(key, None)
        return {}
    return dict(reply_ref)


def _resolve_human_handoff_tool_args(args: Dict[str, Any]) -> Dict[str, Any]:
    resolved = dict(args)
    raw_event = _session_qiwe_raw_event()
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    if not _text(resolved.get("chat_id")):
        resolved["chat_id"] = _session_env("HERMES_SESSION_CHAT_ID") or _text(raw_event.get("fromRoomId"))
    if not _text(resolved.get("original_sender_id")):
        resolved["original_sender_id"] = _session_env("HERMES_SESSION_USER_ID") or _text(raw_event.get("senderId"))
    recent_ref = _recent_qiwe_message_ref(_text(resolved.get("chat_id")), _text(resolved.get("original_sender_id")))
    recent_msg_data = recent_ref.get("msgData") if isinstance(recent_ref.get("msgData"), dict) else {}
    if not _text(resolved.get("original_content")):
        resolved["original_content"] = _text(recent_msg_data.get("content") or msg_data.get("content") or msg_data.get("title"))
    if not _text(resolved.get("original_timestamp")):
        resolved["original_timestamp"] = recent_ref.get("timeStamp") or raw_event.get("timestamp")
    if not _text(resolved.get("original_msg_unique_identifier")):
        resolved["original_msg_unique_identifier"] = _text(recent_ref.get("msgUniqueIdentifier") or raw_event.get("msgUniqueIdentifier"))
    return resolved


def _location_tool_key(args: Dict[str, Any]) -> str:
    provided = _text(args.get("idempotency_key"))
    if provided:
        return provided
    return "|".join(
        [
            _text(args.get("chat_id")),
            _text(args.get("title") or args.get("name")),
            _text(args.get("address")),
            _text(args.get("latitude")),
            _text(args.get("longitude")),
            _text(args.get("message")),
        ]
    )


def _rich_message_tool_key(args: Dict[str, Any]) -> str:
    provided = _text(args.get("idempotency_key"))
    if provided:
        return provided
    payload = args.get("payload") if isinstance(args.get("payload"), dict) else {}
    return "|".join(
        [
            _text(args.get("chat_id")),
            _text(args.get("message_type")),
            json.dumps(payload, sort_keys=True, ensure_ascii=False),
        ]
    )


def _revoke_message_tool_key(args: Dict[str, Any]) -> str:
    provided = _text(args.get("idempotency_key"))
    if provided:
        return provided
    return "|".join([_text(args.get("chat_id")), _text(args.get("msg_server_id") or args.get("msgServerId"))])


def _voice_to_text_tool_key(args: Dict[str, Any]) -> str:
    provided = _text(args.get("idempotency_key"))
    if provided:
        return provided
    return _text(args.get("msg_server_id") or args.get("msgServerId"))


def _human_handoff_tool_key(args: Dict[str, Any]) -> str:
    provided = _text(args.get("idempotency_key"))
    if provided:
        return provided
    return "|".join(
        [
            _text(args.get("chat_id")),
            _text(args.get("support_user_id")),
            _text(args.get("original_msg_unique_identifier")),
            _text(args.get("original_sender_id")),
            _text(args.get("original_content")),
            _text(args.get("message")),
        ]
    )


def _resolve_human_handoff_target(config: QiWeConfig, chat_id: str) -> Dict[str, str]:
    group = _text(chat_id)
    if group and group in config.human_handoff_group_map:
        target = config.human_handoff_group_map[group]
        return {
            "user_id": _text(target.get("user_id")),
            "display_name": _text(target.get("display_name")) or _text(config.human_handoff_display_name),
            "source": "group_map",
        }
    if config.human_handoff_user_id:
        return {
            "user_id": _text(config.human_handoff_user_id),
            "display_name": _text(config.human_handoff_display_name),
            "source": "fallback",
        }
    return {}


def _is_location_tool_duplicate(key: str, ttl_seconds: int) -> bool:
    if not key:
        return False
    now = time.time()
    ttl = max(1, ttl_seconds)
    expired = [item for item, seen_at in _LOCATION_TOOL_SEEN.items() if now - seen_at > ttl]
    for item in expired:
        _LOCATION_TOOL_SEEN.pop(item, None)
    if key in _LOCATION_TOOL_SEEN:
        return True
    _LOCATION_TOOL_SEEN[key] = now
    return False


def _is_direct_tool_duplicate(key: str, ttl_seconds: int) -> bool:
    if not key:
        return False
    now = time.time()
    ttl = max(1, ttl_seconds)
    expired = [item for item, seen_at in _DIRECT_TOOL_SEEN.items() if now - seen_at > ttl]
    for item in expired:
        _DIRECT_TOOL_SEEN.pop(item, None)
    if key in _DIRECT_TOOL_SEEN:
        return True
    _DIRECT_TOOL_SEEN[key] = now
    return False


def _is_rich_message_tool_duplicate(key: str, ttl_seconds: int) -> bool:
    if not key:
        return False
    now = time.time()
    ttl = max(1, ttl_seconds)
    expired = [item for item, seen_at in _RICH_MESSAGE_TOOL_SEEN.items() if now - seen_at > ttl]
    for item in expired:
        _RICH_MESSAGE_TOOL_SEEN.pop(item, None)
    if key in _RICH_MESSAGE_TOOL_SEEN:
        return True
    _RICH_MESSAGE_TOOL_SEEN[key] = now
    return False


def _is_revoke_message_tool_duplicate(key: str, ttl_seconds: int) -> bool:
    if not key:
        return False
    now = time.time()
    ttl = max(1, ttl_seconds)
    expired = [item for item, seen_at in _REVOKE_MESSAGE_TOOL_SEEN.items() if now - seen_at > ttl]
    for item in expired:
        _REVOKE_MESSAGE_TOOL_SEEN.pop(item, None)
    if key in _REVOKE_MESSAGE_TOOL_SEEN:
        return True
    _REVOKE_MESSAGE_TOOL_SEEN[key] = now
    return False


def _is_voice_to_text_tool_duplicate(key: str, ttl_seconds: int) -> bool:
    if not key:
        return False
    now = time.time()
    ttl = max(1, ttl_seconds)
    expired = [item for item, seen_at in _VOICE_TO_TEXT_TOOL_SEEN.items() if now - seen_at > ttl]
    for item in expired:
        _VOICE_TO_TEXT_TOOL_SEEN.pop(item, None)
    if key in _VOICE_TO_TEXT_TOOL_SEEN:
        return True
    _VOICE_TO_TEXT_TOOL_SEEN[key] = now
    return False


def _is_human_handoff_tool_duplicate(key: str, ttl_seconds: int) -> bool:
    if not key:
        return False
    now = time.time()
    ttl = max(1, ttl_seconds)
    expired = [item for item, seen_at in _HUMAN_HANDOFF_TOOL_SEEN.items() if now - seen_at > ttl]
    for item in expired:
        _HUMAN_HANDOFF_TOOL_SEEN.pop(item, None)
    if key in _HUMAN_HANDOFF_TOOL_SEEN:
        return True
    _HUMAN_HANDOFF_TOOL_SEEN[key] = now
    return False


def _is_contact_request_tool_duplicate(key: str, ttl_seconds: int) -> bool:
    if not key:
        return False
    now = time.time()
    ttl = max(1, ttl_seconds)
    expired = [item for item, seen_at in _CONTACT_REQUEST_TOOL_SEEN.items() if now - seen_at > ttl]
    for item in expired:
        _CONTACT_REQUEST_TOOL_SEEN.pop(item, None)
    if key in _CONTACT_REQUEST_TOOL_SEEN:
        return True
    _CONTACT_REQUEST_TOOL_SEEN[key] = now
    return False


def _validate_location_tool_args(args: Dict[str, Any]) -> Optional[str]:
    if not _text(args.get("chat_id")):
        return "chat_id is required"
    if not _text(args.get("title") or args.get("name")):
        return "title is required"
    if args.get("latitude") is None:
        return "latitude is required"
    if args.get("longitude") is None:
        return "longitude is required"
    try:
        float(args["latitude"])
        float(args["longitude"])
    except (TypeError, ValueError):
        return "latitude and longitude must be numbers"
    conversation_type = _text(args.get("conversation_type"))
    if conversation_type and conversation_type not in {"group", "direct"}:
        return "conversation_type must be group or direct"
    return None


def _validate_rich_message_tool_args(args: Dict[str, Any]) -> Optional[str]:
    if not _text(args.get("chat_id")):
        return "chat_id is required"
    kind = _normalize_rich_message_type(args.get("message_type"))
    if kind not in QIWE_RICH_MESSAGE_SPECS:
        return "message_type must be one of image, gif, file, voice, link, weapp, personal_card"
    if not isinstance(args.get("payload"), dict):
        return "payload must be an object"
    if not _text(args.get("idempotency_key")):
        return "idempotency_key is required"
    if not _text(args.get("purpose")):
        return "purpose is required"
    conversation_type = _text(args.get("conversation_type"))
    if conversation_type and conversation_type not in {"group", "direct"}:
        return "conversation_type must be group or direct"
    return None


async def _handle_qiwe_send_location_card(args: Dict[str, Any], **kwargs: Any) -> str:
    resolved_args = _resolve_location_tool_args(args)
    error = _validate_location_tool_args(resolved_args)
    if error:
        return _tool_error(error, success=False)

    adapter = QiWeAdapter(type("Config", (), {"extra": {}})())
    if adapter.qiwe.send_enabled and not AIOHTTP_AVAILABLE:
        return _tool_error("aiohttp is not installed", success=False)
    key = _location_tool_key(resolved_args)
    if _is_location_tool_duplicate(key, adapter.qiwe.location_tool_dedupe_ttl_seconds):
        return _tool_result(success=True, duplicate=True, idempotency_key=key)

    chat_id = _text(resolved_args.get("chat_id"))
    conversation_type = _text(resolved_args.get("conversation_type"))
    sender_id = _text(resolved_args.get("sender_id"))
    if conversation_type == "direct":
        sender_id = ""
    is_group = conversation_type != "direct"

    location_card = {
        "title": _text(resolved_args.get("title") or resolved_args.get("name")),
        "address": _text(resolved_args.get("address")),
        "latitude": float(resolved_args["latitude"]),
        "longitude": float(resolved_args["longitude"]),
    }
    result = await adapter._send_location_bundle(
        chat_id,
        _text(resolved_args.get("message")),
        location_card,
        sender_id=sender_id,
        guid=_text(resolved_args.get("guid") or adapter.qiwe.guid),
        is_group=is_group,
    )
    if result.success:
        return _tool_result(
            success=True,
            duplicate=False,
            idempotency_key=key,
            raw_response=result.raw_response,
        )
    return _tool_error(
        result.error or "QiWe location card send failed",
        success=False,
        retryable=result.retryable,
        raw_response=result.raw_response,
    )


async def _handle_qiwe_send_rich_message(args: Dict[str, Any], **kwargs: Any) -> str:
    resolved_args = _resolve_channel_tool_args(args)
    error = _validate_rich_message_tool_args(resolved_args)
    if error:
        return _tool_error(error, success=False)

    adapter = QiWeAdapter(type("Config", (), {"extra": {}})())
    if adapter.qiwe.send_enabled and not AIOHTTP_AVAILABLE:
        return _tool_error("aiohttp is not installed", success=False)
    chat_id = _text(resolved_args.get("chat_id"))
    kind = _normalize_rich_message_type(resolved_args.get("message_type"))
    conversation_type = _text(resolved_args.get("conversation_type"))
    is_group = conversation_type != "direct"
    try:
        adapter._build_rich_message_body(
            kind,
            chat_id,
            resolved_args.get("payload") if isinstance(resolved_args.get("payload"), dict) else {},
            guid=_text(resolved_args.get("guid") or adapter.qiwe.guid),
        )
    except ValueError as exc:
        return _tool_error(str(exc), success=False)
    key = _rich_message_tool_key(resolved_args)
    if _is_rich_message_tool_duplicate(key, adapter.qiwe.rich_message_tool_dedupe_ttl_seconds):
        return _tool_result(success=True, duplicate=True, idempotency_key=key)

    result = await adapter._send_rich_message(
        kind,
        chat_id,
        resolved_args.get("payload") if isinstance(resolved_args.get("payload"), dict) else {},
        guid=_text(resolved_args.get("guid") or adapter.qiwe.guid),
        is_group=is_group,
    )
    if result.success:
        spec = QIWE_RICH_MESSAGE_SPECS[kind]
        return _tool_result(
            success=True,
            duplicate=False,
            method=spec["method"],
            message_type=kind,
            chat_id=chat_id,
            conversation_type="group" if is_group else "direct",
            idempotency_key=key,
            purpose=_text(resolved_args.get("purpose")),
            **_safe_qiwe_status(result.raw_response),
        )
    return _tool_error(
        result.error or "QiWe rich message send failed",
        success=False,
        retryable=result.retryable,
        message_type=kind,
        chat_id=chat_id,
        idempotency_key=key,
        purpose=_text(resolved_args.get("purpose")),
        raw_response=_safe_qiwe_status(result.raw_response),
    )


async def _handle_qiwe_revoke_message(args: Dict[str, Any], **kwargs: Any) -> str:
    resolved_args = _resolve_channel_tool_args(args)
    chat_id = _text(resolved_args.get("chat_id"))
    msg_server_id = resolved_args.get("msg_server_id") if "msg_server_id" in resolved_args else resolved_args.get("msgServerId")
    idempotency_key = _text(resolved_args.get("idempotency_key"))
    purpose = _text(resolved_args.get("purpose"))
    if not chat_id:
        return _tool_error("chat_id is required", success=False)
    if msg_server_id in (None, ""):
        return _tool_error("msg_server_id is required", success=False)
    if not idempotency_key:
        return _tool_error("idempotency_key is required", success=False)
    if not purpose:
        return _tool_error("purpose is required", success=False)

    adapter = QiWeAdapter(type("Config", (), {"extra": {}})())
    if adapter.qiwe.send_enabled and not AIOHTTP_AVAILABLE:
        return _tool_error("aiohttp is not installed", success=False)
    try:
        body = adapter._build_revoke_message_body(
            chat_id,
            msg_server_id,
            guid=_text(resolved_args.get("guid") or adapter.qiwe.guid),
        )
    except ValueError as exc:
        return _tool_error(str(exc), success=False)
    key = _revoke_message_tool_key(resolved_args)
    if _is_revoke_message_tool_duplicate(key, adapter.qiwe.revoke_message_tool_dedupe_ttl_seconds):
        return _tool_result(success=True, duplicate=True, method="/msg/revokeMsg", idempotency_key=key, chat_id=chat_id, purpose=purpose)

    result = await adapter._post_qiwe_body(body)
    if result.success:
        return _tool_result(
            success=True,
            duplicate=False,
            method="/msg/revokeMsg",
            chat_id=chat_id,
            msg_server_id=int(msg_server_id),
            idempotency_key=key,
            purpose=purpose,
            **_safe_qiwe_status(result.raw_response),
        )
    return _tool_error(
        result.error or "QiWe revoke message failed",
        success=False,
        retryable=result.retryable,
        method="/msg/revokeMsg",
        chat_id=chat_id,
        msg_server_id=_text(msg_server_id),
        idempotency_key=key,
        purpose=purpose,
        raw_response=_safe_qiwe_status(result.raw_response),
    )


async def _handle_qiwe_voice_to_text(args: Dict[str, Any], **kwargs: Any) -> str:
    msg_server_id = args.get("msg_server_id") if "msg_server_id" in args else args.get("msgServerId")
    idempotency_key = _text(args.get("idempotency_key"))
    purpose = _text(args.get("purpose"))
    if msg_server_id in (None, ""):
        return _tool_error("msg_server_id is required", success=False)
    if not idempotency_key:
        return _tool_error("idempotency_key is required", success=False)
    if not purpose:
        return _tool_error("purpose is required", success=False)

    adapter = QiWeAdapter(type("Config", (), {"extra": {}})())
    if adapter.qiwe.send_enabled and not AIOHTTP_AVAILABLE:
        return _tool_error("aiohttp is not installed", success=False)
    key = _voice_to_text_tool_key(args)
    if _is_voice_to_text_tool_duplicate(key, adapter.qiwe.voice_to_text_tool_dedupe_ttl_seconds):
        return _tool_result(success=True, duplicate=True, idempotency_key=key, purpose=purpose)

    result = await adapter._voice_to_text(
        _text(msg_server_id),
        guid=_text(args.get("guid") or adapter.qiwe.guid),
    )
    if result.success:
        raw = _first_mapping(result.raw_response)
        return _tool_result(
            success=True,
            duplicate=False,
            idempotency_key=key,
            purpose=purpose,
            msg_server_id=int(msg_server_id),
            voice_id=_text(raw.get("voiceId")),
            text=_text(raw.get("text")),
        )
    return _tool_error(
        result.error or "QiWe voice transcription failed",
        success=False,
        retryable=result.retryable,
        idempotency_key=key,
        purpose=purpose,
        msg_server_id=_text(msg_server_id),
    )


async def _handle_qiwe_send_direct_message(args: Dict[str, Any], **kwargs: Any) -> str:
    recipient_user_id = _text(args.get("recipient_user_id") or args.get("chat_id"))
    message = _text(args.get("message"))
    idempotency_key = _text(args.get("idempotency_key"))
    purpose = _text(args.get("purpose"))
    if not recipient_user_id:
        return _tool_error("recipient_user_id is required", success=False)
    if not message:
        return _tool_error("message is required", success=False)
    if not idempotency_key:
        return _tool_error("idempotency_key is required", success=False)
    if not purpose:
        return _tool_error("purpose is required", success=False)

    adapter = QiWeAdapter(type("Config", (), {"extra": {}})())
    if adapter.qiwe.send_enabled and not AIOHTTP_AVAILABLE:
        return _tool_error("aiohttp is not installed", success=False)
    if _is_direct_tool_duplicate(idempotency_key, adapter.qiwe.direct_tool_dedupe_ttl_seconds):
        return _tool_result(success=True, duplicate=True, idempotency_key=idempotency_key)
    guard_result = await adapter._ensure_direct_recipient_sendable(
        recipient_user_id,
        guid=_text(args.get("guid") or adapter.qiwe.guid),
    )
    if not guard_result.success:
        raw_response = _first_mapping(guard_result.raw_response) if isinstance(guard_result.raw_response, dict) else {}
        session_chat_id = _session_env("HERMES_SESSION_CHAT_ID")
        session_user_id = _session_env("HERMES_SESSION_USER_ID")
        if session_chat_id and session_user_id and session_chat_id != session_user_id:
            raw_response = dict(raw_response)
            raw_response["suggestedNextTool"] = {
                "name": "qiwe_request_direct_contact",
                "mode": "room_member",
                "user_id": recipient_user_id,
                "room_id": session_chat_id,
                "purpose": purpose,
                "requiresApproval": True,
                "reason": "direct_recipient_not_normal_friend",
            }
        return _tool_error(
            guard_result.error or "QiWe direct recipient is not sendable",
            success=False,
            retryable=guard_result.retryable,
            raw_response=raw_response,
        )
    body = adapter._build_send_body(
        recipient_user_id,
        message,
        sender_id="",
        guid=_text(args.get("guid") or adapter.qiwe.guid),
        is_group=False,
    )
    result = await adapter._post_qiwe_body(body)
    if result.success:
        return _tool_result(
            success=True,
            recipient_user_id=recipient_user_id,
            conversation_type="direct",
            method="/msg/sendText",
            idempotency_key=idempotency_key,
            purpose=purpose,
            duplicate=False,
            raw_response=result.raw_response,
        )
    return _tool_error(
        result.error or "QiWe direct message send failed",
        success=False,
        retryable=result.retryable,
        raw_response=result.raw_response,
    )


async def _handle_qiwe_handoff_to_human(args: Dict[str, Any], **kwargs: Any) -> str:
    resolved_args = _resolve_human_handoff_tool_args(args)
    message = _text(resolved_args.get("message"))
    purpose = _text(resolved_args.get("purpose"))
    chat_id = _text(resolved_args.get("chat_id"))
    original_sender_id = _text(resolved_args.get("original_sender_id"))
    original_content = _text(resolved_args.get("original_content"))

    if not chat_id:
        return _tool_error("chat_id is required", success=False)
    if not message:
        return _tool_error("message is required", success=False)
    if not purpose:
        return _tool_error("purpose is required", success=False)

    adapter = QiWeAdapter(type("Config", (), {"extra": {}})())
    if adapter.qiwe.send_enabled and not AIOHTTP_AVAILABLE:
        return _tool_error("aiohttp is not installed", success=False)
    if not adapter.qiwe.human_handoff_enabled:
        return _tool_error("QiWe human handoff is disabled", success=False, retryable=False)

    target = _resolve_human_handoff_target(adapter.qiwe, chat_id)
    support_user_id = _text(target.get("user_id"))
    support_display_name = _text(target.get("display_name"))
    target_source = _text(target.get("source"))
    if not support_user_id:
        return _tool_error(
            "no human handoff target configured for this group",
            success=False,
            retryable=False,
            chat_id=chat_id,
        )

    resolved_args["support_user_id"] = support_user_id
    key = _human_handoff_tool_key(resolved_args)
    if _is_human_handoff_tool_duplicate(key, adapter.qiwe.human_handoff_tool_dedupe_ttl_seconds):
        return _tool_result(
            success=True,
            duplicate=True,
            idempotency_key=key,
            chat_id=chat_id,
            support_user_id=support_user_id,
            support_display_name=support_display_name,
            target_source=target_source,
            purpose=purpose,
        )

    reply_ref: Dict[str, Any] = {
        "userId": original_sender_id,
        "timeStamp": resolved_args.get("original_timestamp"),
        "msgUniqueIdentifier": _text(resolved_args.get("original_msg_unique_identifier")),
        "msgData": {"content": original_content},
    }
    body = adapter._build_send_body(
        chat_id,
        message,
        sender_id="",
        mention_user_ids=[support_user_id],
        guid=_text(resolved_args.get("guid") or adapter.qiwe.guid),
        is_group=True,
        reply_ref=reply_ref,
    )
    result = await adapter._post_qiwe_body(body)
    if result.success:
        return _tool_result(
            success=True,
            duplicate=False,
            method="/msg/sendHyperText",
            chat_id=chat_id,
            support_user_id=support_user_id,
            support_display_name=support_display_name,
            target_source=target_source,
            original_sender_id=original_sender_id,
            original_msg_unique_identifier=_text(resolved_args.get("original_msg_unique_identifier")),
            idempotency_key=key,
            purpose=purpose,
            raw_response=result.raw_response,
        )
    return _tool_error(
        result.error or "QiWe human handoff send failed",
        success=False,
        retryable=result.retryable,
        chat_id=chat_id,
        support_user_id=support_user_id,
        support_display_name=support_display_name,
        target_source=target_source,
        idempotency_key=key,
        purpose=purpose,
        raw_response=result.raw_response,
    )


async def _handle_qiwe_request_direct_contact(args: Dict[str, Any], **kwargs: Any) -> str:
    resolved_args = _resolve_contact_request_tool_args(args)
    mode = _text(resolved_args.get("mode"))
    user_id = _text(resolved_args.get("user_id"))
    verify_text = _text(resolved_args.get("verify_text"))
    purpose = _text(resolved_args.get("purpose"))
    idempotency_key = _text(resolved_args.get("idempotency_key"))
    room_id = _text(resolved_args.get("room_id"))

    if mode not in {"room_member", "deleted_contact"}:
        return _tool_error("mode must be room_member or deleted_contact", success=False)
    if not user_id:
        return _tool_error("user_id is required", success=False)
    if not verify_text:
        return _tool_error("verify_text is required", success=False)
    if not purpose:
        return _tool_error("purpose is required", success=False)
    if not idempotency_key:
        return _tool_error("idempotency_key is required", success=False)
    if mode == "room_member" and not room_id:
        return _tool_error("room_id is required for room_member mode", success=False)

    adapter = QiWeAdapter(type("Config", (), {"extra": {}})())
    if adapter.qiwe.send_enabled and not AIOHTTP_AVAILABLE:
        return _tool_error("aiohttp is not installed", success=False)
    guid = _text(resolved_args.get("guid") or adapter.qiwe.guid)
    method = "/contact/addRoomContact" if mode == "room_member" else "/contact/addDeletedContact"
    if _is_contact_request_tool_duplicate(idempotency_key, adapter.qiwe.contact_request_tool_dedupe_ttl_seconds):
        return _tool_result(
            success=True,
            duplicate=True,
            method=method,
            mode=mode,
            user_id=user_id,
            room_id=room_id,
            purpose=purpose,
            idempotency_key=idempotency_key,
        )

    params = {
        "guid": guid,
        "userId": user_id,
        "verifyText": verify_text,
    }
    if mode == "room_member":
        params["roomId"] = room_id

    result = await adapter._call_qiwe_api(method, params)
    response = _first_mapping(result.raw_response) if isinstance(result.raw_response, dict) else {}
    if result.success:
        return _tool_result(
            success=True,
            duplicate=False,
            method=method,
            mode=mode,
            user_id=user_id,
            room_id=room_id,
            purpose=purpose,
            idempotency_key=idempotency_key,
            qiwe_code=response.get("code"),
            qiwe_msg=_text(response.get("msg")),
        )
    return _tool_error(
        "QiWe direct contact request failed",
        success=False,
        retryable=result.retryable,
        method=method,
        mode=mode,
        user_id=user_id,
        room_id=room_id,
        purpose=purpose,
        idempotency_key=idempotency_key,
        qiwe_code=response.get("code"),
        qiwe_msg=_text(response.get("msg")),
    )


def register(ctx) -> None:
    ctx.register_platform(
        name="qiwe",
        label="QiWe",
        adapter_factory=lambda cfg: QiWeAdapter(cfg, content_parser=parser_from_context(ctx)),
        check_fn=check_requirements,
        validate_config=validate_config,
        is_connected=is_connected,
        required_env=["QIWE_TOKEN"],
        install_hint="pip install aiohttp",
        env_enablement_fn=_env_enablement,
        cron_deliver_env_var="QIWE_HOME_GROUP",
        standalone_sender_fn=_standalone_send,
        allowed_users_env="QIWE_ALLOWED_USERS",
        allow_all_env="QIWE_ALLOW_ALL_USERS",
        max_message_length=DEFAULT_MAX_REPLY_CHARS,
        emoji="💬",
        pii_safe=False,
        platform_hint=(
            "You are chatting on QiWe. Group replies mention the sender; "
            "direct chats reply privately. For approved location results, the "
            "adapter can send a native QiWe location card from structured metadata."
        ),
    )
    ctx.register_tool(
        name="qiwe_send_location_card",
        toolset="qiwe",
        schema=QIWE_SEND_LOCATION_CARD_SCHEMA,
        handler=_handle_qiwe_send_location_card,
        check_fn=check_tool_available,
        requires_env=["QIWE_TOKEN"],
        is_async=True,
        description=QIWE_SEND_LOCATION_CARD_SCHEMA["description"],
        emoji="📍",
    )
    ctx.register_tool(
        name="qiwe_send_direct_message",
        toolset="qiwe",
        schema=QIWE_SEND_DIRECT_MESSAGE_SCHEMA,
        handler=_handle_qiwe_send_direct_message,
        check_fn=check_tool_available,
        requires_env=["QIWE_TOKEN"],
        is_async=True,
        description=QIWE_SEND_DIRECT_MESSAGE_SCHEMA["description"],
        emoji="✉️",
    )
    ctx.register_tool(
        name="qiwe_send_rich_message",
        toolset="qiwe",
        schema=QIWE_SEND_RICH_MESSAGE_SCHEMA,
        handler=_handle_qiwe_send_rich_message,
        check_fn=check_tool_available,
        requires_env=["QIWE_TOKEN"],
        is_async=True,
        description=QIWE_SEND_RICH_MESSAGE_SCHEMA["description"],
        emoji="🧩",
    )
    ctx.register_tool(
        name="qiwe_revoke_message",
        toolset="qiwe",
        schema=QIWE_REVOKE_MESSAGE_SCHEMA,
        handler=_handle_qiwe_revoke_message,
        check_fn=check_tool_available,
        requires_env=["QIWE_TOKEN"],
        is_async=True,
        description=QIWE_REVOKE_MESSAGE_SCHEMA["description"],
        emoji="↩️",
    )
    ctx.register_tool(
        name="qiwe_voice_to_text",
        toolset="qiwe",
        schema=QIWE_VOICE_TO_TEXT_SCHEMA,
        handler=_handle_qiwe_voice_to_text,
        check_fn=check_tool_available,
        requires_env=["QIWE_TOKEN"],
        is_async=True,
        description=QIWE_VOICE_TO_TEXT_SCHEMA["description"],
        emoji="🎙️",
    )
    ctx.register_tool(
        name="qiwe_handoff_to_human",
        toolset="qiwe",
        schema=QIWE_HANDOFF_TO_HUMAN_SCHEMA,
        handler=_handle_qiwe_handoff_to_human,
        check_fn=check_tool_available,
        requires_env=["QIWE_TOKEN"],
        is_async=True,
        description=QIWE_HANDOFF_TO_HUMAN_SCHEMA["description"],
        emoji="🙋",
    )
    ctx.register_tool(
        name="qiwe_request_direct_contact",
        toolset="qiwe",
        schema=QIWE_REQUEST_DIRECT_CONTACT_SCHEMA,
        handler=_handle_qiwe_request_direct_contact,
        check_fn=check_tool_available,
        requires_env=["QIWE_TOKEN"],
        is_async=True,
        description=QIWE_REQUEST_DIRECT_CONTACT_SCHEMA["description"],
        emoji="🤝",
    )
