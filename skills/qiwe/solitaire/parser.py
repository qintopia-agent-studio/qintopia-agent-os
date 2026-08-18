from __future__ import annotations

import hashlib
import calendar
import re
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
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


_CN_NUM = {"零": 0, "一": 1, "二": 2, "两": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7, "八": 8, "九": 9, "十": 10}

# Day-offset keywords for Chinese relative dates. Longest match wins.
_DAY_OFFSETS = [
    ("大后天", 3),
    ("后天", 2),
    ("明天", 1),
    ("明日", 1),
    ("今晚", 0),
    ("明晚", 1),
    ("今天", 0),
    ("今日", 0),
]

_WEEKDAY_CN = {"一": 0, "二": 1, "三": 2, "四": 3, "五": 4, "六": 5, "日": 6, "天": 6}


def _cn_or_arabic_int(text: str) -> int | None:
    """Parse a small Chinese or Arabic integer (e.g. 三/3/十二/十五/二十)."""
    text = _text(text)
    if not text:
        return None
    if text.isdigit():
        return int(text)
    if text in _CN_NUM:
        return _CN_NUM[text]
    # 十X / X十 / X十Y forms for 10..59.
    m = re.fullmatch(r"([一二两三四五六七八九])?十([一二两三四五六七八九])?", text)
    if m:
        tens = _CN_NUM.get(m.group(1), 1) if m.group(1) else 1
        ones = _CN_NUM.get(m.group(2), 0) if m.group(2) else 0
        return tens * 10 + ones
    m = re.fullmatch(r"([一二两三四五六七八九])十([一二两三四五六七八九])", text)
    if m:
        return _CN_NUM[m.group(1)] * 10 + _CN_NUM[m.group(2)]
    return None


def _parse_time_of_day(text: str) -> tuple[int, int] | None:
    """Parse a time-of-day from Chinese text. Returns (hour, minute) or None.

    Handles: 下午六点/下午6点/晚上8点半/8点/18:00/中午12点/凌晨2点 etc.
    Daypart words (早上/上午/中午/下午/晚上/今晚/明晚/凌晨) adjust the hour.
    """
    t = _text(text)
    if not t:
        return None
    daypart = ""
    for kw in ("凌晨", "早上", "早晨", "上午", "中午", "下午", "傍晚", "晚上", "今晚", "明晚", "午间"):
        if kw in t:
            daypart = kw
            break

    hour: int | None = None
    minute = 0
    m = re.search(r"([01]?\d|2[0-3])[:：]([0-5]\d)", t)
    if m:
        hour = int(m.group(1))
        minute = int(m.group(2))
    else:
        m = re.search(r"([零一二两三四五六七八九十]{1,3}|\d{1,2})点(半|[零一二两三四五六七八九十]{1,3}|\d{1,2})?", t)
        if not m:
            return None
        hour = _cn_or_arabic_int(m.group(1))
        if hour is None:
            return None
        tail = m.group(2)
        if tail == "半":
            minute = 30
        elif tail:
            minute = _cn_or_arabic_int(tail) or 0

    if daypart in ("下午", "傍晚", "晚上", "今晚", "明晚") and hour < 12:
        hour += 12
    elif daypart == "中午" and hour < 12:
        # 中午1点 -> 13:00; 中午12点 stays 12.
        if hour != 12:
            hour += 12
    elif daypart == "凌晨" and hour == 12:
        hour = 0
    if not (0 <= hour <= 23 and 0 <= minute <= 59):
        return None
    return hour, minute


def _resolve_cn_relative_date(text: str, anchor_local: datetime) -> datetime | None:
    """Resolve a Chinese relative date to a concrete local date using anchor.

    anchor_local is the solitaire's first-creation time in the activity timezone.
    Returns a tz-aware datetime (date possibly combined with a parsed time), or
    None when the text carries no recognisable relative date.
    """
    t = _text(text)
    if not t:
        return None

    base_date = anchor_local.date()
    # Explicit weekday: 下周三 / 周五 / 星期三 (this-or-next occurrence).
    wm = re.search(r"(下?)(?:周|星期)([一二三四五六日天])", t)
    weekday_target: int | None = None
    if wm:
        weekday_target = _WEEKDAY_CN[wm.group(2)]
        next_week = wm.group(1) == "下"

    day_offset: int | None = None
    for kw, off in _DAY_OFFSETS:
        if kw in t:
            day_offset = off
            break

    target_date = None
    if weekday_target is not None:
        days_ahead = (weekday_target - anchor_local.weekday()) % 7
        if next_week:
            days_ahead = days_ahead + 7 if days_ahead else 7
        elif days_ahead == 0:
            days_ahead = 7  # "周五" on Friday means the coming Friday.
        target_date = base_date + timedelta(days=days_ahead)
    elif day_offset is not None:
        target_date = base_date + timedelta(days=day_offset)

    if target_date is None:
        return None

    tod = _parse_time_of_day(t)
    if tod is not None:
        hour, minute = tod
        return anchor_local.replace(
            year=target_date.year, month=target_date.month, day=target_date.day,
            hour=hour, minute=minute, second=0, microsecond=0,
        )
    # Date only: keep date, no specific time.
    return anchor_local.replace(
        year=target_date.year, month=target_date.month, day=target_date.day,
        hour=0, minute=0, second=0, microsecond=0,
    )


def resolve_relative_start_time(phrase: Any, event: Any) -> tuple[str, str]:
    """Deterministically resolve a Chinese relative-time phrase to start_time.

    Anchored to the solitaire's FIRST creation time (not the parse/forward time),
    so "明天" is computed from when the solitaire was created. Returns
    (start_time, note); both empty when the phrase is not a recognisable
    relative time. Output format matches the downstream contract
    ("%Y-%m-%d %H:%M" or "%Y-%m-%d").
    """
    text = _text(phrase)
    if not text:
        return "", ""
    anchor_iso = solitaire_created_at_from_event(event)
    anchor_dt: datetime | None = None
    if anchor_iso:
        try:
            anchor_dt = datetime.fromisoformat(anchor_iso)
        except ValueError:
            anchor_dt = None
    if anchor_dt is None:
        anchor_dt = getattr(event, "timestamp", None)
    if not isinstance(anchor_dt, datetime):
        return "", ""
    zone = _activity_timezone()
    anchor_local = anchor_dt.astimezone(zone) if anchor_dt.tzinfo else anchor_dt.replace(tzinfo=zone)
    resolved = _resolve_cn_relative_date(text, anchor_local)
    if resolved is None:
        return "", ""
    has_time = _parse_time_of_day(text) is not None
    rendered = _format_activity_datetime(resolved, has_time=has_time)
    note = f"相对时间已按接龙首次发起时间换算为 {rendered}。"
    return rendered, note


def normalize_start_time_from_event(start_time: Any, event: Any) -> tuple[str, str]:
    normalized = _text(start_time)
    if not normalized:
        return "", ""
    # Relative-time phrases (明天/今晚/周五...) are resolved deterministically
    # against the solitaire's first-creation time, so the LLM never has to do
    # date arithmetic (which it anchors to the wrong day). Only kicks in when
    # the value is not already an absolute date.
    if _parse_activity_datetime(normalized) is None:
        resolved, rel_note = resolve_relative_start_time(normalized, event)
        if resolved:
            return resolved, rel_note
        return normalized, ""
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
