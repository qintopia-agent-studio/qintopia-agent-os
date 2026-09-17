"""Source-grounded activity time facts; never infer a missing day or daypart."""
from __future__ import annotations

import re
from dataclasses import asdict, dataclass
from datetime import datetime, timedelta
from zoneinfo import ZoneInfo


@dataclass(frozen=True)
class TimeResolution:
    expression: str
    date_text: str = ""
    time_text: str = ""
    anchor: str = ""
    timezone: str = "Asia/Shanghai"
    state: str = "needs_confirmation"
    reason: str = "missing_time"
    start_time: str = ""

    def to_dict(self):
        return asdict(self)


_DATE = re.compile(r"(?<![\d:：])(?:(\d{4})[-/.年])?(\d{1,2})[-/.月](\d{1,2})(?:日|号)?(?![\d:：])")
_RELATIVE = re.compile(r"大后天|后天|明天|明日|明晚|今晚|今天|今日|下周[一二三四五六日天]|(?:本周|这周|周|星期)[一二三四五六日天]")
_CLOCK = re.compile(r"(?<!\d)(?:(凌晨|早上|早晨|上午|中午|下午|傍晚|晚上|今晚|明晚)\s*)?(?:([01]?\d|2[0-3])[:：]([0-5]\d)|([零一二两三四五六七八九十]{1,3}|\d{1,2})点(半|[零一二两三四五六七八九十]{1,3}|\d{1,2})?)(?!\d)")
_DAY_OFFSETS = {"今天": 0, "今日": 0, "今晚": 0, "明天": 1, "明日": 1, "明晚": 1, "后天": 2, "大后天": 3}
_WEEKDAYS = {v: i for i, v in enumerate("一二三四五六日")}
_WEEKDAYS["天"] = 6


def _number(text):
    if text.isdigit():
        return int(text)
    digits = dict(zip("零一二三四五六七八九", range(10)))
    digits["两"] = 2
    if text in digits:
        return digits[text]
    if "十" in text and text.count("十") == 1:
        left, right = text.split("十")
        if (not left or left in digits) and (not right or right in digits):
            return digits.get(left, 1) * 10 + digits.get(right, 0)
    return None


def _relative_day(text, anchor):
    if text in _DAY_OFFSETS:
        delta = _DAY_OFFSETS[text]
    else:
        delta = _WEEKDAYS[text[-1]] - anchor.weekday()
        if text.startswith("下周"):
            delta += 7
        elif not text.startswith(("本周", "这周")):
            delta %= 7
    return anchor + timedelta(days=delta)


def resolve_time(expression, *, anchor: datetime | None, timezone: str,
                 source: str = "", facts: dict | None = None) -> TimeResolution:
    """Facts are verbatim source spans, not model-generated calendar arithmetic.

    Missing year means the anchor's year, not the next future occurrence. Clock-only
    expressions stay unresolved. Multiple days/occurrences need explicit clarification.
    """
    expression = str(expression or "").strip()
    base = dict(expression=expression, anchor=anchor.isoformat() if anchor else "", timezone=timezone)

    def pending(reason, **parts):
        return TimeResolution(**base, reason=reason, **parts)

    text = expression
    if facts is not None:
        if expression and expression not in source:
            return pending("ungrounded_time_expression")
        if not isinstance(facts, dict) or set(facts) - {"date_text", "time_text"}:
            return pending("invalid_time_facts")
        parts = [facts.get(k, "") for k in ("date_text", "time_text")]
        if any(not isinstance(v, str) or len(v) > 160 or (v and v not in source) for v in parts):
            return pending("ungrounded_time_facts")
        # Facts may provide a date from another line, but never override explicit
        # evidence in the extracted expression (e.g. a multiple-day range).
        text = " ".join([expression] + [v for v in parts if v and v not in expression])
    if facts is not None:
        source_dates = {m.group(0) for m in _DATE.finditer(source)}
        source_relative_dates = {m.group(0) for m in _RELATIVE.finditer(source)}
        if len(source_dates) > 1 or len(source_relative_dates) > 1:
            return pending("multiple_dates")
        if re.search(r"\d\s*(?:到|至|[-~～—–])\s*\d+\s*(?:日|号)|每天|每日|每周", source):
            return pending("multiple_occurrences")
    if not text:
        return pending("missing_time")
    if len(text) > 512:
        return pending("unsupported_time")
    if re.search(r"\d\s*(?:到|至|[-~～—–])\s*\d+\s*(?:日|号)|每天|每日|每周", text):
        return pending("multiple_occurrences")
    dates = list(_DATE.finditer(text))
    relatives = list(_RELATIVE.finditer(text))
    dates = list({m.group(0): m for m in dates}.values())
    relatives = list({m.group(0): m for m in relatives}.values())
    if len(dates) > 1 or len(relatives) > 1:
        return pending("multiple_dates")
    if not dates and not relatives:
        return pending("missing_date")
    zone = ZoneInfo(timezone)
    local_anchor = (anchor.astimezone(zone) if anchor.tzinfo else anchor.replace(tzinfo=zone)) if anchor else None
    date_text = (dates or relatives)[0].group(0)
    try:
        if dates:
            year, month, day = dates[0].groups()
            if not year and local_anchor is None:
                return pending("missing_anchor", date_text=date_text)
            day_value = datetime(int(year) if year else local_anchor.year, int(month), int(day), tzinfo=zone)
        else:
            if local_anchor is None:
                return pending("missing_anchor", date_text=date_text)
            day_value = _relative_day(date_text, local_anchor)
        if dates and relatives:
            relative_text = relatives[0].group(0)
            if relative_text.startswith(("周", "星期")):
                agrees = _WEEKDAYS[relative_text[-1]] == day_value.weekday()
            else:
                agrees = local_anchor is not None and _relative_day(relative_text, local_anchor).date() == day_value.date()
            if not agrees:
                return pending("conflicting_dates", date_text=date_text)

    except ValueError:
        return pending("invalid_date", date_text=date_text)
    clocks = list(_CLOCK.finditer(text))
    clocks = list({m.group(0): m for m in clocks}.values())
    if not clocks:
        return pending("missing_clock", date_text=date_text)
    if len(clocks) > 2:
        return pending("multiple_times", date_text=date_text)
    if len(clocks) == 2:
        between = text[clocks[0].end():clocks[1].start()].strip()
        if between not in {"-", "–", "—", "‑", "~", "～", "至", "到"}:
            return pending("multiple_times", date_text=date_text)
    clock = clocks[0]
    daypart, hh, mm, cn_hour, cn_minute = clock.groups()
    hour = int(hh) if hh is not None else _number(cn_hour)
    minute = int(mm) if mm is not None else (30 if cn_minute == "半" else _number(cn_minute) if cn_minute else 0)
    if hour is None or minute is None or not 0 <= hour <= 23 or not 0 <= minute <= 59:
        return pending("invalid_clock", date_text=date_text, time_text=clock.group(0))
    daypart = daypart or (date_text if date_text in {"今晚", "明晚"} else "")
    if hh is None and not daypart and 1 <= hour <= 12:
        return pending("ambiguous_daypart", date_text=date_text, time_text=clock.group(0))
    if daypart in {"下午", "傍晚", "晚上", "今晚", "明晚", "中午"} and hour < 12:
        hour += 12
    if daypart in {"凌晨", "早上", "早晨", "上午"} and hour == 12:
        hour = 0
    resolved = day_value.replace(hour=hour, minute=minute, second=0, microsecond=0)
    return TimeResolution(**base, date_text=date_text, time_text=clock.group(0),
                          state="resolved", reason="resolved", start_time=resolved.strftime("%Y-%m-%d %H:%M"))
