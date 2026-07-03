from __future__ import annotations

import hashlib
import calendar
import re
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from zoneinfo import ZoneInfo


@dataclass
class ActivityRecord:
    activity_id: str
    source_group_id: str
    source_message_id: str
    source_sender_id: str
    activity_subject: str
    source_message_ref: Dict[str, Any] = field(default_factory=dict)
    activity_identity: str = ""
    stable_body_fingerprint: str = ""
    activity_type: str = ""
    activity_detail: str = ""
    start_time: str = ""
    solitaire_created_at: str = ""
    time_normalization_note: str = ""
    participant_names: List[str] = field(default_factory=list)
    participant_count: int = 0
    promo_text: str = ""
    status: str = "active"
    raw_summary: str = ""
    first_seen_at: str = ""
    last_seen_at: str = ""

    def to_internal_fields(self) -> Dict[str, Any]:
        return {
            "activity_id": self.activity_id,
            "source_group_id": self.source_group_id,
            "source_message_id": self.source_message_id,
            "source_sender_id": self.source_sender_id,
            "source_message_ref": dict(self.source_message_ref),
            "activity_subject": self.activity_subject,
            "activity_identity": self.activity_identity,
            "stable_body_fingerprint": self.stable_body_fingerprint,
            "activity_type": self.activity_type,
            "activity_detail": self.activity_detail,
            "start_time": self.start_time,
            "solitaire_created_at": self.solitaire_created_at,
            "time_normalization_note": self.time_normalization_note,
            "participant_names": list(self.participant_names),
            "participant_count": self.participant_count,
            "promo_text": self.promo_text,
            "status": self.status,
            "raw_summary": self.raw_summary,
            "first_seen_at": self.first_seen_at,
            "last_seen_at": self.last_seen_at,
        }


def _text(value: Any) -> str:
    return str(value if value is not None else "").replace("\r\n", "\n").strip()


def _compact(value: str) -> str:
    return "".join(_text(value).split()).lower()


def _hash(value: str, length: int = 16) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()[:length]


def strip_locale_header(title: str) -> str:
    lines = _text(title).splitlines()
    while lines and not lines[0].strip():
        lines.pop(0)
    if lines and lines[0].lstrip().startswith("#"):
        lines.pop(0)
    return "\n".join(lines).strip()


_PARTICIPANT_LINE_RE = re.compile(r"^\s*\d+[\.\)、)]\s+.+$")


def stable_activity_body(title: str) -> str:
    body = strip_locale_header(title)
    lines = body.splitlines()
    end = len(lines)
    saw_participant = False
    while end > 0:
        line = lines[end - 1]
        if not line.strip():
            end -= 1
            continue
        if _PARTICIPANT_LINE_RE.match(line):
            saw_participant = True
            end -= 1
            continue
        break
    if saw_participant:
        while end > 0 and not lines[end - 1].strip():
            end -= 1
        stable = "\n".join(lines[:end]).strip()
    else:
        stable = body
    return stable or body


def _activity_id(group_id: str, stable_fingerprint: str, start_time: str, title: str, subject: str) -> str:
    if stable_fingerprint:
        key = f"{group_id}|body:{stable_fingerprint}|{_compact(start_time)}"
    elif start_time:
        key = f"{group_id}|subject:{_compact(subject)}|{_compact(start_time)}"
    else:
        key = f"{group_id}|subject:{_compact(subject)}|{_hash(strip_locale_header(title))}"
    return "act_" + _hash(key, 20)


def _promo_text(subject: str, detail: str, start_time: str, count: int) -> str:
    subject = subject or "这场活动"
    parts = [f"一起参加「{subject}」"]
    if start_time:
        parts.append(f"时间：{start_time}")
    if detail:
        parts.append(f"地点/详情：{detail}")
    parts.append(f"当前已有 {count} 人接龙")
    return "，".join(parts) + "。"


async def parse_activity_record(event: Any, content_parser: Any | None = None) -> Optional[ActivityRecord]:
    if _text(getattr(event, "message_kind", "")) != "solitaire":
        return None
    if content_parser is None:
        return None
    return await content_parser.parse(event)


def build_activity_record_from_fields(
    event: Any,
    title: str,
    *,
    activity_subject: str,
    activity_type: str = "",
    activity_identity: str = "",
    activity_detail: str = "",
    start_time: str = "",
    participant_names: List[str] | None = None,
    promo_text: str = "",
) -> Optional[ActivityRecord]:
    subject = _text(activity_subject)
    participants = [_text(name) for name in (participant_names or []) if _text(name)]
    if not subject or not participants:
        return None
    seen_at = getattr(event, "timestamp", None)
    if isinstance(seen_at, datetime):
        last_seen_at = seen_at.astimezone(timezone.utc).isoformat()
    else:
        last_seen_at = datetime.now(timezone.utc).isoformat()
    solitaire_created_at = solitaire_created_at_from_event(event, fallback=last_seen_at)
    detail = _text(activity_detail)
    normalized_start_time, time_normalization_note = normalize_start_time_from_event(start_time, event)
    stable_body = stable_activity_body(title)
    stable_body_fingerprint = _hash(_compact(stable_body), 20) if stable_body else ""
    identity = _text(activity_identity) or _first_nonempty_line(stable_body) or subject
    activity_id = _activity_id(_text(getattr(event, "group_id", "")), stable_body_fingerprint, normalized_start_time, title, subject)
    raw_summary = "\n".join(strip_locale_header(title).splitlines()[:12])
    sender_display = _text(getattr(event, "sender_name", "")) or (participants[0] if participants else "") or _text(getattr(event, "sender_id", ""))
    source_message_ref = build_source_message_ref(event)
    return ActivityRecord(
        activity_id=activity_id,
        source_group_id=_text(getattr(event, "group_id", "")),
        source_message_id=_text(getattr(event, "event_id", "")),
        source_sender_id=sender_display,
        source_message_ref=source_message_ref,
        activity_subject=subject,
        activity_identity=identity,
        stable_body_fingerprint=stable_body_fingerprint,
        activity_type=_text(activity_type),
        activity_detail=detail,
        start_time=normalized_start_time,
        solitaire_created_at=solitaire_created_at,
        time_normalization_note=time_normalization_note,
        participant_names=participants,
        participant_count=len(participants),
        promo_text=_text(promo_text) or _promo_text(subject, detail, normalized_start_time, len(participants)),
        raw_summary=raw_summary,
        first_seen_at=last_seen_at,
        last_seen_at=last_seen_at,
    )


def solitaire_created_at_from_event(event: Any, *, fallback: str = "") -> str:
    raw_event = getattr(event, "raw_event_ref", {})
    candidates: List[datetime] = []
    if isinstance(raw_event, dict):
        msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
        solitaire_info = msg_data.get("solitaireInfo") if isinstance(msg_data.get("solitaireInfo"), dict) else {}
        for item in solitaire_info.get("items", []) if isinstance(solitaire_info.get("items"), list) else []:
            if not isinstance(item, dict):
                continue
            parsed = _epoch_datetime(item.get("timestamp"))
            if parsed is not None:
                candidates.append(parsed)
        for key in ("timestamp", "createTime", "createdAt"):
            parsed = _epoch_datetime(solitaire_info.get(key))
            if parsed is not None:
                candidates.append(parsed)
        parsed = _epoch_datetime(raw_event.get("timestamp"))
        if parsed is not None:
            candidates.append(parsed)

    sent_at = getattr(event, "timestamp", None)
    if isinstance(sent_at, datetime):
        candidates.append(sent_at.astimezone(timezone.utc))
    if candidates:
        return min(candidates).isoformat()
    return fallback


def _epoch_datetime(value: Any) -> datetime | None:
    if value in (None, ""):
        return None
    try:
        timestamp = float(value)
    except (TypeError, ValueError):
        return None
    if timestamp > 10_000_000_000:
        timestamp = timestamp / 1000
    try:
        return datetime.fromtimestamp(timestamp, tz=timezone.utc)
    except (OverflowError, OSError, ValueError):
        return None


def normalize_start_time_from_event(start_time: Any, event: Any) -> tuple[str, str]:
    normalized = _text(start_time)
    if not normalized:
        return "", ""
    parsed = _parse_activity_datetime(normalized)
    sent_at = getattr(event, "timestamp", None)
    if parsed is None or not isinstance(sent_at, datetime):
        return normalized, ""
    zone = _activity_timezone()
    sent_local = sent_at.astimezone(zone) if sent_at.tzinfo else sent_at.replace(tzinfo=zone)
    parsed_local = parsed.replace(tzinfo=zone)
    if parsed_local >= sent_local:
        return normalized, ""
    day = min(parsed_local.day, calendar.monthrange(sent_local.year, sent_local.month)[1])
    corrected = parsed_local.replace(year=sent_local.year, month=sent_local.month, day=day)
    corrected_text = _format_activity_datetime(corrected, has_time=_start_time_has_time(normalized))
    note = f"接龙里的时间像是写错了月份；二花已按当前月份记录为 {corrected_text}。"
    return corrected_text, note


def _activity_timezone() -> ZoneInfo:
    try:
        import os

        return ZoneInfo(os.getenv("QIWE_ACTIVITY_TIMEZONE", "Asia/Shanghai"))
    except Exception:
        return ZoneInfo("Asia/Shanghai")


def _parse_activity_datetime(value: str) -> datetime | None:
    text = _text(value)
    if not text:
        return None
    try:
        return datetime.fromisoformat(text)
    except ValueError:
        pass
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%d", "%Y/%m/%d %H:%M:%S", "%Y/%m/%d %H:%M", "%Y/%m/%d"):
        try:
            return datetime.strptime(text, fmt)
        except ValueError:
            continue
    return None


def _start_time_has_time(value: str) -> bool:
    text = _text(value)
    return bool(re.search(r"\d{1,2}:\d{2}", text) or "T" in text)


def _format_activity_datetime(value: datetime, *, has_time: bool) -> str:
    if has_time:
        return value.strftime("%Y-%m-%d %H:%M")
    return value.strftime("%Y-%m-%d")


def _first_nonempty_line(value: str) -> str:
    for line in _text(value).splitlines():
        if line.strip():
            return line.strip()
    return ""


def build_source_message_ref(event: Any) -> Dict[str, Any]:
    raw_event = getattr(event, "raw_event_ref", {})
    if not isinstance(raw_event, dict):
        return {}
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    msg_server_id = _text(raw_event.get("msgServerId"))
    if not msg_server_id or not msg_data:
        return {}
    ref = {
        "msgServerId": msg_server_id,
        "msgUniqueIdentifier": _text(raw_event.get("msgUniqueIdentifier")),
        "userId": _text(raw_event.get("senderId")),
        "showName": _text(getattr(event, "sender_name", "")),
        "timeStamp": raw_event.get("timestamp"),
        "msgType": raw_event.get("msgType"),
        "newMsgType": _text(raw_event.get("newMsgType")),
        "msgData": msg_data,
    }
    solitaire_info = msg_data.get("solitaireInfo") if isinstance(msg_data.get("solitaireInfo"), dict) else {}
    if solitaire_info:
        ref["solitaireAuthorId"] = _text(solitaire_info.get("authorId"))
    return {key: value for key, value in ref.items() if value not in ("", None)}


def summarize_activity_for_agent(activity: ActivityRecord) -> str:
    names = "、".join(activity.participant_names) if activity.participant_names else "暂无"
    return (
        "用户发送了一条群接龙消息。\n\n"
        f"活动主题：{activity.activity_subject}\n"
        f"活动类型：{activity.activity_type or '未分类'}\n"
        f"活动详情：{activity.activity_detail or '未提供'}\n"
        f"开始时间：{activity.start_time or '未识别'}\n"
        f"当前参与人数：{activity.participant_count}\n"
        f"参与人：{names}\n"
        f"宣传语草稿：{activity.promo_text}"
    )
