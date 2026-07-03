from __future__ import annotations

import json
import logging
import os
import re
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Dict, List, Optional, Protocol
from zoneinfo import ZoneInfo

from .parser import ActivityRecord, build_activity_record_from_fields, strip_locale_header

logger = logging.getLogger(__name__)


class SolitaireContentParser(Protocol):
    async def parse(self, event: Any) -> Optional[ActivityRecord]:
        ...


ACTIVITY_SCHEMA: Dict[str, Any] = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "is_activity": {"type": "boolean"},
        "activity_subject": {"type": "string"},
        "activity_identity": {"type": "string"},
        "activity_type": {"type": "string"},
        "activity_detail": {"type": "string"},
        "start_time": {"type": "string"},
        "participant_names": {"type": "array", "items": {"type": "string"}},
        "promo_text": {"type": "string"},
    },
    "required": ["is_activity", "activity_subject", "activity_type", "activity_detail", "start_time", "participant_names", "promo_text"],
}


@dataclass
class HermesSolitaireContentParser:
    llm: Any
    enabled: bool = True
    timeout_seconds: int = 20
    last_diagnostic: Dict[str, Any] | None = None

    @classmethod
    def from_context(cls, ctx: Any) -> Optional["HermesSolitaireContentParser"]:
        if not _truthy(os.getenv("QIWE_SOLITAIRE_LLM_PARSE_ENABLED")):
            return None
        llm = getattr(ctx, "llm", None)
        if llm is None:
            logger.warning("[qiwe] solitaire LLM parser enabled but ctx.llm is unavailable")
            return None
        return cls(llm=llm, timeout_seconds=_int(os.getenv("QIWE_SOLITAIRE_LLM_TIMEOUT_SECONDS"), 20))

    async def parse(self, event: Any) -> Optional[ActivityRecord]:
        self.last_diagnostic = None
        if not self.enabled:
            self.last_diagnostic = _diagnostic("parser_disabled")
            return None
        title = _event_title(event)
        if not title:
            self.last_diagnostic = _diagnostic("missing_title")
            return None
        context = _event_context(event)
        try:
            result = await self.llm.acomplete(
                messages=[
                    {"role": "system", "content": _instructions()},
                    {"role": "user", "content": f"{context}\n\n接龙正文：\n{strip_locale_header(title)}"},
                ],
                temperature=0,
                max_tokens=1200,
                timeout=max(1, self.timeout_seconds),
                purpose="qiwe_solitaire_activity_parse",
            )
        except Exception as exc:
            logger.warning("[qiwe] Hermes solitaire LLM parser failed: %s", exc)
            self.last_diagnostic = _diagnostic("llm_request_failed", error=_safe_error(exc))
            return None
        response_text = _text(getattr(result, "text", ""))
        fields = _parse_json_object(response_text)
        if fields is None:
            logger.warning("[qiwe] Hermes solitaire LLM parser returned invalid JSON")
            self.last_diagnostic = _diagnostic("invalid_json", response_preview=_preview(response_text))
            return None
        if not _truthy(fields.get("is_activity")):
            self.last_diagnostic = _diagnostic("llm_non_activity", response_preview=_preview_json(fields))
            return None
        activity = _activity_from_fields(event, title, fields)
        if activity is None:
            self.last_diagnostic = _diagnostic(_invalid_activity_reason(fields), response_preview=_preview_json(fields))
            return None
        self.last_diagnostic = _diagnostic("activity_parsed")
        return activity


def parser_from_context(ctx: Any) -> Optional[SolitaireContentParser]:
    return HermesSolitaireContentParser.from_context(ctx)


def _instructions() -> str:
    return (
        "你是微信群接龙活动解析器。只输出一个 JSON object，不要输出解释、Markdown、代码块或额外文本。\n"
        "任务：判断接龙正文是否是活动/预报名/报名接龙，并抽取活动事实。\n"
        "必须输出以下字段：\n"
        "{"
        "\"is_activity\": boolean, "
        "\"activity_subject\": string, "
        "\"activity_identity\": string, "
        "\"activity_type\": string, "
        "\"activity_detail\": string, "
        "\"start_time\": string, "
        "\"participant_names\": string[], "
        "\"promo_text\": string"
        "}\n"
        "规则：\n"
        "1. 如果不是活动接龙，is_activity=false，其它字段给空字符串或空数组。\n"
        "2. activity_subject 是用于展示的活动主题。多场系列活动时填写总主题。\n"
        "3. activity_identity 是用于归并同一接龙活动的稳定身份短语，尽量从接龙正文首句/标题原文摘取，"
        "不要润色、不要概括、不要加入参与人名单；没有明确标题时给空字符串。\n"
        "4. activity_type 是简短活动分类，例如运动娱乐、社区活动、手作体验、餐饮聚会、学习分享、志愿服务、其他。\n"
        "5. activity_detail 包含地点、费用、说明、多场活动时间等有用信息。\n"
        "6. start_time 能确定具体日期/时间则输出 YYYY-MM-DD HH:MM 或 YYYY-MM-DD。"
        "如果正文只有“下午3点半”“今晚8点”“明天10点”等相对时间，必须结合消息发送时间和时区补全年月日。"
        "只有完全无法确定时才输出空字符串；多场活动无法确定单一开始时间也输出空字符串。\n"
        "7. participant_names 按接龙编号提取参与人。代报名要展开为占位名，"
        "例如“大羽带三个人”输出“大羽、大羽代报名1、大羽代报名2、大羽代报名3”；"
        "“阿城 2”表示阿城共 2 人，输出“阿城、阿城代报名1”。\n"
        "8. promo_text 是一句简短自然的活动宣传语。\n"
        "9. 不要臆造没有出现在正文里的地点或人员。"
    )


def _activity_from_fields(event: Any, title: str, fields: Dict[str, Any]) -> Optional[ActivityRecord]:
    if not _has_valid_shape(fields):
        logger.warning("[qiwe] Hermes solitaire LLM parser returned JSON with invalid shape")
        return None
    subject = _text(fields.get("activity_subject"))
    participants = _string_list(fields.get("participant_names"))
    if not subject or not participants:
        return None
    return build_activity_record_from_fields(
        event,
        title,
        activity_subject=subject,
        activity_identity=_text(fields.get("activity_identity")),
        activity_type=_text(fields.get("activity_type")),
        activity_detail=_text(fields.get("activity_detail")),
        start_time=_text(fields.get("start_time")),
        participant_names=participants,
        promo_text=_text(fields.get("promo_text")),
    )


def _invalid_activity_reason(fields: Dict[str, Any]) -> str:
    if not _has_valid_shape(fields):
        return "invalid_shape"
    if not _text(fields.get("activity_subject")):
        return "missing_subject"
    if not _string_list(fields.get("participant_names")):
        return "missing_participants"
    return "invalid_activity_fields"


def _event_context(event: Any) -> str:
    timezone_name = os.getenv("QIWE_ACTIVITY_TIMEZONE", "Asia/Shanghai")
    timestamp = getattr(event, "timestamp", None)
    local_timestamp = _format_event_time(timestamp, timezone_name)
    return "\n".join(
        [
            "解析上下文：",
            f"- 消息发送时间：{local_timestamp or '未知'}",
            f"- 时区：{timezone_name}",
            "- 如果接龙正文出现相对时间，请以消息发送时间为基准补全。",
        ]
    )


def _format_event_time(value: Any, timezone_name: str) -> str:
    if not isinstance(value, datetime):
        return ""
    try:
        zone = ZoneInfo(timezone_name)
    except Exception:
        zone = ZoneInfo("Asia/Shanghai")
    local = value.astimezone(zone) if value.tzinfo else value.replace(tzinfo=zone)
    return local.strftime("%Y-%m-%d %H:%M:%S")


def _diagnostic(reason: str, **extra: Any) -> Dict[str, Any]:
    payload: Dict[str, Any] = {"reason": reason}
    for key, value in extra.items():
        text = _text(value)
        if text:
            payload[key] = text
    return payload


def _preview_json(value: Dict[str, Any]) -> str:
    return _preview(json.dumps(value, ensure_ascii=False, sort_keys=True))


def _preview(value: Any, limit: int = 500) -> str:
    text = _text(value)
    if len(text) <= limit:
        return text
    return text[:limit] + "...(truncated)"


def _safe_error(exc: Exception) -> str:
    return _preview(str(exc), limit=300)


def _event_title(event: Any) -> str:
    title = _text(getattr(event, "text", ""))
    if title:
        return title
    for attachment in getattr(event, "attachments", []) or []:
        if isinstance(attachment, dict) and attachment.get("title"):
            return _text(attachment.get("title"))
    return ""


def _string_list(value: Any) -> List[str]:
    if isinstance(value, list):
        return [_text(item) for item in value if _text(item)]
    return []


def _has_valid_shape(fields: Dict[str, Any]) -> bool:
    expected_types = {
        "is_activity": bool,
        "activity_subject": str,
        "activity_type": str,
        "activity_detail": str,
        "start_time": str,
        "participant_names": list,
        "promo_text": str,
    }
    for key, expected_type in expected_types.items():
        if key not in fields or not isinstance(fields.get(key), expected_type):
            return False
    return all(isinstance(item, str) for item in fields["participant_names"])


_FENCE_RE = re.compile(r"```(?:json)?\s*(.*?)```", re.DOTALL | re.IGNORECASE)


def _parse_json_object(text: str) -> Optional[Dict[str, Any]]:
    candidates = []
    fence_match = _FENCE_RE.search(text)
    if fence_match:
        candidates.append(fence_match.group(1).strip())
    candidates.append(text.strip())
    for candidate in candidates:
        if not candidate:
            continue
        try:
            parsed = json.loads(candidate)
        except (TypeError, ValueError):
            continue
        if isinstance(parsed, dict):
            return parsed
    return None


def _truthy(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    return str(value).strip().lower() in {"1", "true", "yes", "on", "是"}


def _text(value: Any) -> str:
    return str(value if value is not None else "").replace("\r\n", "\n").strip()


def _int(value: Any, default: int) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default
