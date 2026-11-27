"""Source-grounded times, with clock-only dates anchored to first creation."""
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


_DATE = re.compile(r"(?<!\d)(?<!\d:)(?<!\d：)(?:(\d{4})[-/.年])?(\d{1,2})[-/.月](\d{1,2})(?:日|号)?(?![\d:：])")
_RELATIVE = re.compile(r"大后天|后天|明天|明日|明晚|今晚|今天|今日|下周[一二三四五六日天]|(?:本周|这周|周|星期)[一二三四五六日天]")
_CLOCK = re.compile(r"(?<!\d)(?:(凌晨|早上|早晨|上午|中午|下午|傍晚|晚上|今晚|明晚)\s*)?(?:([01]?\d|2[0-3])[:：]([0-5]\d)|([零一二两三四五六七八九十]{1,3}|\d{1,2})点(半|[零一二两三四五六七八九十]{1,3}|\d{1,2})?)(?!\d)")
_DOTTED_CLOCK = re.compile(r"((?:凌晨|早上|早晨|上午|中午|下午|傍晚|晚上|今晚|明晚)\s*\d{1,2})[.．]([0-9]{2})(?![\d.．日号])")
# An unparsed date is not an absent date. Do not silently turn these expressions
# into the creation day when the model extracts only their clock fragment.
_UNSUPPORTED_DATE = re.compile(
    r"昨天|昨日|昨晚|前天|前日|明早|明晨|(?:上|下|这|本)(?:周|星期)|"
    r"(?:上|下|这|本)个(?:星期|周|月)|(?:周|星期)末|(?:上|下|本|这)月|"
    r"月[底末初中]|[上下]旬|[今明去后]年|\d{1,2}月|\d{1,2}[日号](?!楼|房|门|线)|"
    r"(?:日期|时间|日程)\s*[:：]?\s*(?:待定|待确认|另行通知)"
)
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


def _clock_notation(text):
    # Only an explicit daypart disambiguates 7.30 as a clock. Keep bare month/day
    # and dotted calendar dates intact. Replacement preserves source span offsets.
    return _DOTTED_CLOCK.sub(r"\1:\2", text)


def _relative_matches(text, anchor):
    days = {}
    for match in _RELATIVE.finditer(text):
        label = match.group(0)
        key = _relative_day(label, anchor).date() if anchor else _DAY_OFFSETS.get(label, label)
        # Preserve evening evidence when synonymous labels share the same day.
        if key not in days or label in {"今晚", "明晚"}:
            days[key] = match
    return list(days.values())


def resolve_time(expression, *, anchor: datetime | None, timezone: str,
                 source: str = "", facts: dict | None = None) -> TimeResolution:
    """Facts are verbatim source spans, not model-generated calendar arithmetic.

    Missing year or date uses the anchor's year or day, never the next future
    occurrence. Ambiguous clocks and multiple days/occurrences need clarification.
    """
    expression = str(expression or "").strip()
    base = dict(expression=expression, anchor=anchor.isoformat() if anchor else "", timezone=timezone)

    def pending(reason, **parts):
        return TimeResolution(**base, reason=reason, **parts)

    zone = ZoneInfo(timezone)
    local_anchor = (anchor.astimezone(zone) if anchor.tzinfo else anchor.replace(tzinfo=zone)) if anchor else None
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
        date_part, time_part = parts
        text = " ".join(v for v in [date_part if date_part not in expression else "", expression,
                                   time_part if time_part not in expression else ""] if v)
    if facts is not None:
        source_dates = {m.group(0) for m in _DATE.finditer(_clock_notation(source))}
        source_relative_dates = _relative_matches(source, local_anchor)
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
    clock_text = _clock_notation(text)
    dates = list(_DATE.finditer(clock_text))
    relatives = _relative_matches(text, local_anchor)
    evidence = source if facts is not None else text
    # Source dates outrank the default even if the model omitted them. Source
    # context also distinguishes a fragment like "7.30" in "下午7.30" from a date.
    if facts is not None:
        dates = list(_DATE.finditer(_clock_notation(source)))
        relatives = _relative_matches(source, local_anchor)
    dates = list({m.group(0): m for m in dates}.values())
    if len(dates) > 1 or len(relatives) > 1:
        return pending("multiple_dates")
    creation_day = not dates and not relatives
    # A recognized month/day or weekday must not swallow an unsupported qualifier
    # such as "明年" or "昨天". Mask complete supported spans, then inspect the rest.
    date_remainder = _RELATIVE.sub(" ", _DATE.sub(" ", _clock_notation(evidence)))
    if _UNSUPPORTED_DATE.search(date_remainder):
        return pending("unsupported_date")
    date_text = (dates or relatives)[0].group(0) if not creation_day else ""
    try:
        if creation_day:
            if local_anchor is None:
                return pending("missing_anchor")
            day_value = local_anchor
        elif dates:
            year, month, day = dates[0].groups()
            if not year and local_anchor is None:
                return pending("missing_anchor", date_text=date_text)
            day_value = datetime(int(year) if year else local_anchor.year, int(month), int(day), tzinfo=zone)
        else:
            if local_anchor is None:
                return pending("missing_anchor", date_text=date_text)
            day_value = _relative_day(date_text, local_anchor)
        # Validate all original constraints, including source evidence omitted
        # by the model. Same-day grouping must not erase a conflicting "today"
        # beside an explicit date with a matching weekday in another week.
        for match in _DATE.finditer(_clock_notation(evidence)):
            year, month, day = match.groups()
            if not year and local_anchor is None:
                return pending("missing_anchor", date_text=date_text)
            evidence_day = datetime(int(year) if year else local_anchor.year, int(month), int(day), tzinfo=zone)
            if evidence_day.date() != day_value.date():
                return pending("conflicting_dates", date_text=date_text)
        for match in _RELATIVE.finditer(evidence):
            relative_text = match.group(0)
            if relative_text.startswith(("周", "星期")):
                agrees = _WEEKDAYS[relative_text[-1]] == day_value.weekday()
            else:
                agrees = local_anchor is not None and _relative_day(relative_text, local_anchor).date() == day_value.date()
            if not agrees:
                return pending("conflicting_dates", date_text=date_text)

    except ValueError:
        return pending("invalid_date", date_text=date_text)
    # Defaulting a date must not let a model-selected clock hide another source
    # occurrence, or choose a range's end as its start.
    clock_source = source if creation_day and facts is not None else text
    clocks = list(_CLOCK.finditer(_clock_notation(clock_source)))
    clocks = list({m.group(0): m for m in clocks}.values())
    if not clocks:
        return pending("missing_clock", date_text=date_text)
    if len(clocks) > 2:
        return pending("multiple_times", date_text=date_text)
    if len(clocks) == 2:
        between = clock_source[clocks[0].end():clocks[1].start()].strip()
        if between not in {"-", "–", "—", "‑", "~", "～", "至", "到"}:
            return pending("multiple_times", date_text=date_text)
    clock = clocks[0]
    time_text = clock_source[clock.start():clock.end()]
    daypart, hh, mm, cn_hour, cn_minute = clock.groups()
    hour = int(hh) if hh is not None else _number(cn_hour)
    minute = int(mm) if mm is not None else (30 if cn_minute == "半" else _number(cn_minute) if cn_minute else 0)
    if hour is None or minute is None or not 0 <= hour <= 23 or not 0 <= minute <= 59:
        return pending("invalid_clock", date_text=date_text, time_text=time_text)
    daypart = daypart or (date_text if date_text in {"今晚", "明晚"} else "")
    if hh is None and not daypart and 1 <= hour <= 12:
        return pending("ambiguous_daypart", date_text=date_text, time_text=time_text)
    if daypart in {"下午", "傍晚", "晚上", "今晚", "明晚", "中午"} and hour < 12:
        hour += 12
    if daypart in {"凌晨", "早上", "早晨", "上午"} and hour == 12:
        hour = 0
    resolved = day_value.replace(hour=hour, minute=minute, second=0, microsecond=0)
    return TimeResolution(**base, date_text=date_text, time_text=time_text,
                          state="resolved", reason="resolved_from_creation_day" if creation_day else "resolved",
                          start_time=resolved.strftime("%Y-%m-%d %H:%M"))
