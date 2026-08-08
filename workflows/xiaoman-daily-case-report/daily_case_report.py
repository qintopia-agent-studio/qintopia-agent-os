#!/usr/bin/env python3
"""Xiaoman daily community case-file report generator.

Reads the last 24 hours of QiWe group messages from the Qintopia message store,
aggregates them into a playful "群聊案件档案" (group chat case file), renders a
PNG poster, and prints a human-review draft. It never sends. A human must
confirm before any group post.

The script is deterministic for a given input window: the same chat/date always
yields the same report (modulo rendering timestamps). It fails closed if the
message store is unreachable or the required runtime flags are not set.
"""
from __future__ import annotations

import argparse
import html
import json
import os
import re
import sys
import tempfile
from collections import Counter
from dataclasses import dataclass
from datetime import datetime, time, timedelta
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError


DEFAULT_GROUP_NAME = "秦托邦的小伙伴（新）"
DEFAULT_REPORT_TITLE = "群聊案件档案"
CHAT_ID_ENV = "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
DEFAULT_TIMEZONE = "Asia/Shanghai"
DEFAULT_OUTPUT_WIDTH = 750
DEFAULT_CASE_LIMIT = 6
DEFAULT_SUSPECT_LIMIT = 5
DEFAULT_HOURLY_BUCKETS = 24
DEFAULT_WINDOW_HOURS = 24
DEFAULT_MIN_CASE_MESSAGES = 3
DEFAULT_TOP_KEYWORDS = 18

STOP_WORDS: set[str] = {
    "这个", "那个", "然后", "就是", "什么", "怎么", "还是", "可以", "今天",
    "明天", "现在", "已经", "没有", "但是", "因为", "所以", "一下", "大家",
    "我们", "你们", "他们", "自己", "这里", "那里", "这样", "那样", "一个",
    "不是", "不用", "不要", "应该", "可能", "需要", "觉得", "看看", "一下",
    "哈哈", "嘿嘿", "嗯嗯", "好的", "收到", "谢谢", "请问", "知道", "真的",
    "一下", "一直", "一下", "时候", "过来", "过去", "为了", "作为", "关于",
    "还是", "或者", "以及", "并且", "虽然", "尽管", "不过", "只是", "而且",
}

CASE_CARD_COLORS = [
    ("#fef3c7", "#92400e"),  # amber
    ("#fee2e2", "#991b1b"),  # red
    ("#dbeafe", "#1e40af"),  # blue
    ("#dcfce7", "#166534"),  # green
    ("#f3e8ff", "#6b21a8"),  # purple
    ("#ffedd5", "#9a3412"),  # orange
]


@dataclass
class ReportMessage:
    id: str
    sender_id: str
    sender_name: str
    text: str
    sent_at: datetime | None
    message_kind: str


@dataclass
class CaseCard:
    case_no: str
    title: str
    time_label: str
    summary: str
    bullets: list[str]
    message_count: int
    participant_count: int
    color_bg: str
    color_text: str
    top_speaker: str


@dataclass
class Suspect:
    rank: int
    name: str
    message_count: int
    word_count: int
    avatar_emoji: str


@dataclass
class ReportData:
    group_name: str
    report_title: str
    report_date: str
    time_range: str
    member_count: int
    message_count: int
    participant_count: int
    case_count: int
    suspect_count: int
    hourly_counts: list[int]
    cases: list[CaseCard]
    suspects: list[Suspect]
    quote: str
    highlight: str


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Xiaoman daily community case-file report")
    parser.add_argument(
        "--date",
        help="Backfill one calendar day (YYYY-MM-DD). Omit for the latest rolling 24-hour window.",
    )
    parser.add_argument(
        "--chat-id",
        default=os.environ.get(CHAT_ID_ENV),
        help=(
            "QiWe chat id to report on. Required for database mode; "
            f"may be set with {CHAT_ID_ENV}."
        ),
    )
    parser.add_argument("--group-name", default=DEFAULT_GROUP_NAME)
    parser.add_argument("--report-title", default=DEFAULT_REPORT_TITLE)
    parser.add_argument("--timezone", default=DEFAULT_TIMEZONE)
    parser.add_argument("--output-dir", default="/tmp/xiaoman-daily-case-report")
    parser.add_argument("--output-width", type=int, default=DEFAULT_OUTPUT_WIDTH)
    parser.add_argument("--fixture", help="Path to JSON fixture with pre-canned messages.")
    parser.add_argument(
        "--render",
        choices=["auto", "png", "html"],
        default="png",
        help="Render mode. png (default) produces the shareable image; html keeps the raw page for debugging.",
    )
    parser.add_argument(
        "--keep-html",
        action="store_true",
        help="Keep the intermediate HTML file (debug only; the PNG is the deliverable).",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print full JSON instead of just the operator review message.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Generate from fixture or empty stub; do not read the database.",
    )
    return parser.parse_args()


def _report_timezone(timezone_name: str) -> ZoneInfo:
    try:
        return ZoneInfo(timezone_name)
    except ZoneInfoNotFoundError as exc:
        raise RuntimeError(f"unsupported daily case report timezone: {timezone_name}") from exc


def _report_date_at(
    args: argparse.Namespace,
    now: datetime,
) -> tuple[datetime, datetime, str]:
    report_zone = _report_timezone(args.timezone)
    if args.date:
        base_date = datetime.strptime(args.date, "%Y-%m-%d").date()
        start = datetime.combine(base_date, time.min, tzinfo=report_zone)
        end = start + timedelta(days=1)
        display = start.strftime("%Y年%m月%d日")
        return start, end, display

    local_now = now.astimezone(report_zone) if now.tzinfo else now.replace(tzinfo=report_zone)
    end = local_now.replace(microsecond=0)
    start = end - timedelta(hours=DEFAULT_WINDOW_HOURS)
    display = f"过去{DEFAULT_WINDOW_HOURS}小时（截至 {end.strftime('%Y年%m月%d日 %H:%M')}）"
    return start, end, display


def _report_date(args: argparse.Namespace) -> tuple[datetime, datetime, str]:
    """Return report start (inclusive), end (exclusive), and display date string."""
    report_zone = _report_timezone(args.timezone)
    return _report_date_at(args, datetime.now(report_zone))


def _time_range_label(start: datetime, end: datetime) -> str:
    """Return a human-readable time range like 08/07 07:45–08/08 07:44."""
    end_display = end - timedelta(seconds=1)
    if start.date() == end_display.date():
        return f"{start.strftime('%H:%M')}–{end_display.strftime('%H:%M')}"
    return f"{start.strftime('%m/%d %H:%M')}–{end_display.strftime('%m/%d %H:%M')}"


def _normalize_message_times(
    messages: list[ReportMessage],
    report_zone: ZoneInfo,
) -> list[ReportMessage]:
    normalized: list[ReportMessage] = []
    for msg in messages:
        sent_at = msg.sent_at
        if sent_at is not None:
            if sent_at.tzinfo is None:
                sent_at = sent_at.replace(tzinfo=report_zone)
            else:
                sent_at = sent_at.astimezone(report_zone)
        normalized.append(
            ReportMessage(
                id=msg.id,
                sender_id=msg.sender_id,
                sender_name=msg.sender_name,
                text=msg.text,
                sent_at=sent_at,
                message_kind=msg.message_kind,
            )
        )
    return normalized


def _database_url() -> str | None:
    return (
        os.environ.get("QINTOPIA_MESSAGE_STORE_DATABASE_URL")
        or os.environ.get("QINTOPIA_SIDECAR_DATABASE_URL")
    )


def _require_read_through() -> bool:
    return os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE") == "1"


def _load_fixture(path: str) -> list[ReportMessage]:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    messages = []
    for item in data.get("messages", []):
        sent_at = None
        raw_sent = item.get("sent_at") or item.get("received_at")
        if raw_sent:
            try:
                sent_at = datetime.fromisoformat(raw_sent)
            except ValueError:
                pass
        messages.append(
            ReportMessage(
                id=str(item.get("id", "")),
                sender_id=str(item.get("sender_id", "")),
                sender_name=str(item.get("sender_name", "匿名")),
                text=(item.get("text") or ""),
                sent_at=sent_at,
                message_kind=str(item.get("message_kind", "text")),
            )
        )
    return messages


def _fetch_messages(
    chat_id: str | None,
    start: datetime,
    end: datetime,
) -> list[ReportMessage]:
    db_url = _database_url()
    if not db_url:
        raise RuntimeError(
            "message store database URL not configured; "
            "set QINTOPIA_MESSAGE_STORE_DATABASE_URL or run with --fixture/--dry-run"
        )

    try:
        import psycopg
    except ImportError as exc:
        raise RuntimeError(
            "psycopg is required for database reads; install it or use --fixture/--dry-run"
        ) from exc

    sql = """
        SELECT
            m.id::text AS id,
            m.sender_id,
            m.sender_name,
            m.text,
            m.message_kind,
            COALESCE(m.sent_at, m.received_at) AS report_time
        FROM qintopia_messages.messages m
        WHERE m.platform = 'qiwe'
          AND m.chat_type = 'group'
          AND m.message_kind = 'text'
          AND NULLIF(BTRIM(m.text), '') IS NOT NULL
          AND COALESCE(m.sent_at, m.received_at) >= %s
          AND COALESCE(m.sent_at, m.received_at) < %s
    """
    params: list[Any] = [start, end]
    if chat_id:
        sql += " AND m.chat_id = %s"
        params.append(chat_id)

    sql += " ORDER BY COALESCE(m.sent_at, m.received_at) ASC"

    messages: list[ReportMessage] = []
    with psycopg.connect(db_url) as conn:
        with conn.cursor() as cur:
            cur.execute(sql, params)
            for row in cur.fetchall():
                messages.append(
                    ReportMessage(
                        id=row[0],
                        sender_id=row[1] or "",
                        sender_name=row[2] or "匿名",
                        text=row[3] or "",
                        sent_at=row[5],
                        message_kind=row[4] or "text",
                    )
                )
    return messages


def _uses_real_messages(args: argparse.Namespace) -> bool:
    return not args.dry_run and not args.fixture


def _validate_production_boundaries(args: argparse.Namespace) -> None:
    if not args.chat_id:
        raise RuntimeError(
            f"production read-through requires --chat-id or {CHAT_ID_ENV}; "
            "do not run an unscoped group-message query"
        )


def _prepare_output_dir(path: str) -> Path:
    output_dir = Path(path)
    output_dir.mkdir(parents=True, exist_ok=True)
    output_dir.chmod(0o700)
    return output_dir


def _write_private_text(path: Path, content: str) -> None:
    with tempfile.NamedTemporaryFile(
        "w",
        encoding="utf-8",
        dir=path.parent,
        prefix=f".{path.stem}-",
        suffix=path.suffix,
        delete=False,
    ) as handle:
        tmp_path = Path(handle.name)
        os.chmod(tmp_path, 0o600)
        handle.write(content)
    tmp_path.replace(path)
    os.chmod(path, 0o600)


def _clean_text(text: str) -> str:
    text = text or ""
    text = re.sub(r"https?://\S+", "", text)
    text = re.sub(r"(?<!\S)@(?:[A-Za-z0-9_.-]{1,64}|[\u4e00-\u9fff]{1,6})(?=\s|$)", "", text)
    text = re.sub(r"\s+", " ", text)
    return text.strip()


def _tokenize(text: str) -> list[str]:
    """Tokenize text for keyword clustering. Uses jieba when available."""
    text = _clean_text(text).lower()
    try:
        import jieba

        tokens = list(jieba.lcut(text))
    except ImportError:
        # Fallback: extract 2-4 character Chinese chunks and English words.
        chinese = re.findall(r"[\u4e00-\u9fa5]{2,4}", text)
        english = re.findall(r"[a-z]{3,}", text)
        tokens = chinese + english
    filtered: list[str] = []
    for token in tokens:
        token = token.strip()
        if not token or token in STOP_WORDS or len(token) < 2:
            continue
        if token.isdigit():
            continue
        filtered.append(token)
    return filtered


def _keyword_scores(messages: list[ReportMessage]) -> Counter:
    counter: Counter = Counter()
    for msg in messages:
        for token in _tokenize(msg.text):
            counter[token] += 1
    return counter


def _is_clean_topic(kw: str) -> bool:
    """Reject noise tokens so case titles stay meaningful.

    Excludes stop words, common English noise (none/null/nan), and any token
    without a Chinese character — a case title should read like a real topic.
    """
    if not kw or kw in STOP_WORDS:
        return False
    if kw.lower() in {"none", "null", "nan", "true", "false"}:
        return False
    if not any("\u4e00" <= c <= "\u9fa5" for c in kw):
        return False
    return True


def _detect_topic_markers(messages: list[ReportMessage]) -> dict[str, list[ReportMessage]]:
    """Group messages under explicit topic markers like 'Topic：'.

    Messages that follow a marker (until the next marker) are folded into the
    same case, mirroring how real group threads flow. 接龙 (WeChat sign-up
    chains) are strong thread starters and get their own case title.
    """
    clusters: dict[str, list[ReportMessage]] = {}
    pattern = re.compile(r"^([^：:\n]{2,30})[：:]\s*")
    current_topic: str | None = None
    for msg in messages:
        cleaned = _clean_text(msg.text)
        if cleaned.startswith("#接龙"):
            # Activity name follows the "#接龙" prefix; stop at whitespace/date.
            body = cleaned[3:].strip()
            m = re.match(r"^([^\s，,0-9]{2,20})", body)
            title = m.group(1) if m else body[:12]
            current_topic = f"接龙 · {title}"
        else:
            match = pattern.match(cleaned)
            if match:
                topic = match.group(1).strip()
                # Reject false markers like "...8月8日，19:30" where the text
                # before the colon is a time fragment ending in a digit.
                if (
                    4 <= len(topic) <= 24
                    and not topic[-1].isdigit()
                    and not topic.endswith(("，", ",", "、"))
                ):
                    current_topic = topic
        if current_topic:
            clusters.setdefault(current_topic, []).append(msg)
    return clusters


def _cluster_cases(
    messages: list[ReportMessage],
    limit: int = DEFAULT_CASE_LIMIT,
) -> list[CaseCard]:
    """Group messages into topical case cards.

    Strategy:
    1. Use explicit topic markers (text before a colon) when available.
    2. Fall back to keyword clustering for unassigned messages.
    """
    if not messages:
        return []

    clusters = _detect_topic_markers(messages)
    assigned_ids = {id(m) for cluster in clusters.values() for m in cluster}
    unassigned = [m for m in messages if id(m) not in assigned_ids]

    keyword_scores = _keyword_scores(unassigned)
    top_keywords = [
        kw for kw, _ in keyword_scores.most_common(DEFAULT_TOP_KEYWORDS) if _is_clean_topic(kw)
    ]

    for msg in unassigned:
        tokens = set(_tokenize(msg.text))
        best_keyword = ""
        best_score = 0
        for kw in top_keywords:
            if kw in tokens and keyword_scores[kw] > best_score:
                best_keyword = kw
                best_score = keyword_scores[kw]
        if not best_keyword:
            continue
        clusters.setdefault(f"关于「{best_keyword}」的讨论", []).append(msg)

    # Keep clusters above the minimum size, sorted by message count.
    sorted_clusters = sorted(
        clusters.items(),
        key=lambda item: (-len(item[1]), -_keyword_scores(messages).get(item[0], 0)),
    )

    cases: list[CaseCard] = []
    for index, (keyword, cluster) in enumerate(sorted_clusters[:limit], start=1):
        if len(cluster) < DEFAULT_MIN_CASE_MESSAGES:
            continue
        times = [m.sent_at for m in cluster if m.sent_at]
        if times:
            start_t, end_t = min(times), max(times)
            if start_t.date() == end_t.date():
                time_label = f"{start_t.strftime('%H:%M')}–{end_t.strftime('%H:%M')}"
            else:
                time_label = f"{start_t.strftime('%m/%d %H:%M')}–{end_t.strftime('%m/%d %H:%M')}"
        else:
            time_label = "时间未知"
        participants = {m.sender_name for m in cluster}
        speaker_counts: Counter = Counter()
        for m in cluster:
            name = m.sender_name or "匿名"
            if name != "匿名":
                speaker_counts[name] += 1
        top_speaker = speaker_counts.most_common(1)[0][0] if speaker_counts else "群友"
        # Build bullets from representative messages: longest first, then earliest.
        sorted_by_length = sorted(cluster, key=lambda m: (-len(m.text), m.sent_at or datetime.min))[:3]
        bullets = []
        for m in sorted_by_length:
            snippet = _clean_text(m.text)[:70]
            if snippet and snippet not in bullets:
                bullets.append(snippet)
        if not bullets:
            bullets = ["群友围绕该话题展开了讨论。"]

        color_bg, color_text = CASE_CARD_COLORS[(index - 1) % len(CASE_CARD_COLORS)]
        cases.append(
            CaseCard(
                case_no=f"CASE {index:02d}",
                title=keyword,
                time_label=time_label,
                summary=f"{len(cluster)} 条消息，{len(participants)} 人参与",
                bullets=bullets,
                message_count=len(cluster),
                participant_count=len(participants),
                top_speaker=top_speaker,
                color_bg=color_bg,
                color_text=color_text,
            )
        )
    return cases


def _compute_suspects(messages: list[ReportMessage], limit: int = DEFAULT_SUSPECT_LIMIT) -> list[Suspect]:
    counts: Counter = Counter()
    words: Counter = Counter()
    for msg in messages:
        name = msg.sender_name or "匿名"
        counts[name] += 1
        words[name] += len(_clean_text(msg.text))

    suspects = []
    avatars = ["🕵️", "🕵️‍♀️", "🥷", "🦹", "🧙"]
    for rank, (name, msg_count) in enumerate(counts.most_common(limit), start=1):
        suspects.append(
            Suspect(
                rank=rank,
                name=name,
                message_count=msg_count,
                word_count=words[name],
                avatar_emoji=avatars[(rank - 1) % len(avatars)],
            )
        )
    return suspects


def _extract_highlight(messages: list[ReportMessage]) -> str:
    """Pick one real, quotable group message for the '今日高亮' block."""
    candidates = []
    for m in messages:
        text = _clean_text(m.text)
        if len(text) < 20:
            continue
        if "接龙" in text or text.startswith("打卡"):
            continue
        candidates.append((len(text), text))
    if not candidates:
        return "今日群聊暂无特别亮眼的发言，但每一次交流都在悄悄累积。"
    candidates.sort(reverse=True)
    best = candidates[0][1]
    return best[:80] + ("…" if len(best) > 80 else "")


def _hourly_timeline(messages: list[ReportMessage], start: datetime, buckets: int = DEFAULT_HOURLY_BUCKETS) -> list[int]:
    counts = [0] * buckets
    for msg in messages:
        t = msg.sent_at
        if not t:
            continue
        delta = t - start
        hour = int(delta.total_seconds() // 3600)
        if 0 <= hour < buckets:
            counts[hour] += 1
    return counts


def _build_report(args: argparse.Namespace) -> ReportData:
    start, end, display_date = _report_date(args)
    report_zone = _report_timezone(args.timezone)

    if args.fixture:
        messages = _load_fixture(args.fixture)
    elif args.dry_run:
        messages = _sample_messages(start)
    else:
        if not _require_read_through():
            raise RuntimeError(
                "database read-through is disabled; set "
                "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1 or use --fixture/--dry-run"
            )
        messages = _fetch_messages(args.chat_id, start, end)
    messages = _normalize_message_times(messages, report_zone)

    if not messages and not args.dry_run and not args.fixture:
        # Empty day is a normal result, not an error.
        pass

    unique_senders = {m.sender_id for m in messages}
    cases = _cluster_cases(messages)
    suspects = _compute_suspects(messages)
    hourly = _hourly_timeline(messages, start)
    max_hourly = max(hourly) if hourly else 1

    time_range = _time_range_label(start, end)
    quote = os.environ.get(
        "QINTOPIA_DAILY_CASE_REPORT_QUOTE",
        "我们最大的光荣不在于从不跌倒，而在于每次跌倒后都能爬起来。",
    )

    return ReportData(
        group_name=args.group_name,
        report_title=args.report_title,
        report_date=display_date,
        time_range=time_range,
        member_count=int(os.environ.get("QINTOPIA_DAILY_CASE_REPORT_MEMBER_COUNT", len(unique_senders) or 1)),
        message_count=len(messages),
        participant_count=len(unique_senders),
        case_count=len(cases),
        suspect_count=min(len(suspects), DEFAULT_SUSPECT_LIMIT),
        hourly_counts=hourly,
        cases=cases,
        suspects=suspects,
        quote=quote,
        highlight=_extract_highlight(messages),
    )


def _sample_messages(start: datetime) -> list[ReportMessage]:
    """Return deterministic demo messages for dry-run previews.

    These are intentionally generic community-group chatter (check-ins,
    resource sharing, Q&A, event previews, market small talk, new-member
    welcomes). They exist only to exercise the template and clustering logic;
    they carry NO project-internal / development-process content. In production
    the report is built purely from real group messages.
    """
    demos = [
        ("08:05", "阿杰", "每日共学打卡：今天第 8 天，把 Solidity 函数修饰符啃完了，合约编译通过 ✅"),
        ("08:12", "小雨", "打卡+1，我还在学事件和日志那块，有点绕。"),
        ("08:18", "阿杰", "事件是给前端监听用的，多写两个 demo 就懂了。"),
        ("09:30", "娜娜", "资源分享：发现一个讲 MEV 的系列长文，讲得特别清楚，链接发群里了。"),
        ("09:36", "老王", "收藏了，正好想补这块。"),
        ("09:42", "娜娜", "还有个交互式沙盒，可以直接改参数看套利路径。"),
        ("10:15", "Tom", "技术求助：hardhat 本地节点一直连不上，有没有人遇到过？"),
        ("10:20", "Mia", "检查下 8545 端口是不是被占了，或者 .env 里 RPC 写错了。"),
        ("10:25", "Tom", "果然是端口冲突，谢谢！"),
        ("11:00", "小雨", "本周活动预告：周六晚 8 点有个 AMA，嘉宾聊 RWA 赛道。"),
        ("11:06", "阿杰", "报名+1，想问 RWA 的合规边界。"),
        ("11:10", "小雨", "我把问题收集表发群里了，大家填一下。"),
        ("14:20", "Mia", "行情闲聊：今天大盘又跌了，大家稳住别恐慌。"),
        ("14:26", "老王", "跌下来正好定投，长期看没问题。"),
        ("14:31", "娜娜", "哈哈，每次跌都有人说定投，每次涨都说踏空。"),
        ("16:45", "小林", "新人报到：刚进群，跟着大家从零学 web3，请多关照～"),
        ("16:50", "阿杰", "欢迎欢迎，置顶文档先看一遍。"),
        ("16:55", "Mia", "有问题随时问，群里氛围很友好。"),
        ("20:10", "小雨", "今日收尾：今天收获满满，明天继续打卡。"),
        ("20:15", "阿杰", "+1，一起加油。"),
    ]
    messages = []
    for idx, (time_str, name, text) in enumerate(demos, start=1):
        hour, minute = map(int, time_str.split(":"))
        sent_at = start + timedelta(hours=hour, minutes=minute)
        messages.append(
            ReportMessage(
                id=f"demo-{idx}",
                sender_id=f"user-{name}",
                sender_name=name,
                text=text,
                sent_at=sent_at,
                message_kind="text",
            )
        )
    return messages


def _bar_svg(counts: list[int], max_count: int, width: int, height: int) -> str:
    if not counts or max_count == 0:
        return ""
    bar_width = width // len(counts)
    gap = 2
    effective_width = max(1, bar_width - gap)
    bars = []
    for idx, count in enumerate(counts):
        h = int((count / max_count) * height) if max_count else 0
        x = idx * bar_width + gap // 2
        y = height - h
        bars.append(
            f'<rect x="{x}" y="{y}" width="{effective_width}" height="{h}" rx="2" fill="#1a2744"/>'
        )
    return "\n".join(bars)


def _render_html(report: ReportData, width: int) -> str:
    max_hourly = max(report.hourly_counts) if report.hourly_counts else 1
    timeline_svg = _bar_svg(report.hourly_counts, max_hourly, width - 80, 80)
    timeline_labels = []
    peak_idx = report.hourly_counts.index(max_hourly) if report.hourly_counts else -1
    for idx in range(0, 25, 4):
        x = 40 + int((idx / 24) * (width - 80))
        is_peak = idx == peak_idx
        color = "#c45c48" if is_peak else "#9ca3af"
        weight = "700" if is_peak else "400"
        timeline_labels.append(
            f'<text x="{x}" y="105" font-size="10" fill="{color}" font-weight="{weight}" text-anchor="middle">{idx:02d}</text>'
        )
    peak_svg = ""
    if peak_idx >= 0 and max_hourly > 0:
        bar_w = (width - 80) // 24
        peak_x = peak_idx * bar_w + bar_w // 2
        peak_svg = f'<text x="{peak_x}" y="16" font-size="11" fill="#c45c48" font-weight="700" text-anchor="middle">{max_hourly}</text>'

    def _topic_emoji(title: str) -> str:
        if any(k in title for k in ("打卡", "共学", "学习")):
            return "\U0001F4DA"
        if any(k in title for k in ("资源", "分享", "链接", "文章")):
            return "\U0001F517"
        if any(k in title for k in ("求助", "问题", "问", "报错", "bug", "BUG")):
            return "\u2753"
        if any(k in title for k in ("活动", "预告", "AMA", "报名", "直播")):
            return "\U0001F4C5"
        if any(k in title for k in ("行情", "闲聊", "大盘", "跌", "涨", "市场")):
            return "\U0001F4AC"
        if any(k in title for k in ("新人", "报到", "欢迎", "进群")):
            return "\U0001F44B"
        return "\U0001F4CC"

    kpi_specs = [
        (report.message_count, "消息总量", "\U0001F4AC", "#2563eb"),
        (report.participant_count, "活跃人数", "\U0001F525", "#0891b2"),
        (report.case_count, "今日话题", "\U0001F9E9", "#7c3aed"),
        (report.suspect_count, "活跃之星", "\u2B50", "#d97706"),
    ]
    stats_html = "\n".join(
        f"""
        <div style="background:linear-gradient(160deg,{accent} 0%,{accent}cc 100%);border-radius:16px;padding:16px 8px;text-align:center;box-shadow:0 6px 16px rgba(0,0,0,0.12);">
          <div style="font-size:20px;margin-bottom:4px;">{icon}</div>
          <div style="font-size:26px;font-weight:900;color:#ffffff;line-height:1;">{value}</div>
          <div style="font-size:11px;color:rgba(255,255,255,0.85);margin-top:4px;">{label}</div>
        </div>"""
        for value, label, icon, accent in kpi_specs
    )

    case_cards = []
    for case in report.cases:
        bullets = "".join(
            f'<li style="margin:0 0 6px 16px;font-size:13px;line-height:1.5;color:{case.color_text}">{html.escape(b)}</li>'
            for b in case.bullets
        )
        tag = _topic_emoji(case.title)
        card = f"""
        <div style="background:{case.color_bg};border-radius:16px;padding:16px 16px 16px 18px;margin-bottom:14px;break-inside:avoid;border-left:5px solid {case.color_text};">
          <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px;">
            <span style="font-size:11px;font-weight:700;color:{case.color_text};opacity:.65;letter-spacing:1px">{case.case_no}</span>
            <span style="font-size:10px;background:rgba(0,0,0,0.06);color:{case.color_text};padding:2px 8px;border-radius:999px">{html.escape(case.time_label)}</span>
          </div>
          <div style="font-size:16px;font-weight:800;color:{case.color_text};margin-bottom:6px;line-height:1.4">{tag} {html.escape(case.title)}</div>
          <div style="font-size:12px;color:{case.color_text};opacity:.8;margin-bottom:10px">{html.escape(case.summary)}</div>
          <ul style="padding:0;list-style:disc">{bullets}</ul>
        </div>
        """
        case_cards.append(card)

    suspects_html = []
    rank_colors = ["#c45c48", "#d97706", "#ca8a04", "#65a30d", "#0891b2"]
    for suspect in report.suspects:
        badge = rank_colors[(suspect.rank - 1) % len(rank_colors)]
        suspects_html.append(f"""
        <div style="display:flex;align-items:center;gap:12px;background:#fff;border-radius:14px;padding:12px 14px;border:1px solid #e5e7eb;box-shadow:0 1px 2px rgba(0,0,0,0.03);">
          <div style="width:44px;height:44px;border-radius:50%;background:linear-gradient(135deg,#1a2744,#2d3a5f);color:#fff;display:flex;align-items:center;justify-content:center;font-size:20px">{suspect.avatar_emoji}</div>
          <div style="flex:1">
            <div style="font-size:14px;font-weight:700;color:#1f2937">{html.escape(suspect.name)}</div>
            <div style="font-size:12px;color:#6b7280">{suspect.message_count} 条消息 · {suspect.word_count} 字</div>
          </div>
          <div style="width:38px;height:38px;border-radius:12px;background:{badge};color:#fff;display:flex;align-items:center;justify-content:center;font-size:15px;font-weight:800;">{suspect.rank}</div>
        </div>
        """)

    cases_grid = "\n".join(case_cards)
    suspects_grid = "\n".join(suspects_html)

    return f"""<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; background:#eef1f6; color:#1f2937; }}
</style>
</head>
<body>
<div style="width:{width}px;background:linear-gradient(180deg,#1a2744 0%,#22304f 230px,#eef1f6 230px,#eef1f6 100%);padding:30px 24px 28px;position:relative;overflow:hidden;">
  <div style="text-align:center;margin-bottom:18px;">
    <div style="font-size:11px;letter-spacing:3px;color:#aab4c8;margin-bottom:6px;">DAILY COMMUNITY REPORT</div>
    <div style="font-family:'Songti SC','STSong','Noto Serif CJK SC',serif;font-size:25px;font-weight:700;color:#ffffff;margin-bottom:6px;">{html.escape(report.group_name)}</div>
    <div style="font-size:16px;font-weight:800;color:#f0b69a;">{html.escape(report.report_title)}</div>
  </div>

  <div style="display:flex;justify-content:center;gap:14px;font-size:11px;color:#cbd5e1;margin-bottom:20px;">
    <span>\U0001F4C5 {report.report_date}</span>
    <span>⏱ {report.time_range}</span>
    <span>\U0001F465 {report.member_count} 名成员</span>
  </div>

  <div style="display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:22px;">
    {stats_html}
  </div>

  <div style="background:#fff;border-radius:18px;padding:16px 18px;margin-bottom:22px;border:1px solid #e5e7eb;box-shadow:0 4px 14px rgba(20,30,60,0.05);">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:6px;">
      <div style="font-size:13px;font-weight:700;color:#1a2744;">24H 活跃节奏</div>
      <div style="font-size:11px;color:#9ca3af;">峰值 {max_hourly} 条 / {peak_idx:02d}:00</div>
    </div>
    <svg width="{width - 56}" height="120" viewBox="0 0 {width - 56} 120">
      {timeline_svg}
      {peak_svg}
      {"".join(timeline_labels)}
    </svg>
  </div>

  <div style="margin-bottom:22px;">
    <div style="font-size:15px;font-weight:800;color:#1a2744;margin-bottom:12px;">今日话题 · 便签墙</div>
    {cases_grid}
  </div>

  <div style="margin-bottom:22px;">
    <div style="font-size:15px;font-weight:800;color:#1a2744;margin-bottom:12px;">活跃之星榜</div>
    <div style="display:grid;grid-template-columns:1fr;gap:10px;">
      {suspects_grid}
    </div>
  </div>

  <div style="text-align:center;padding-top:14px;border-top:1px dashed #cbd5e1;">
    <div style="font-family:'Songti SC','STSong','Noto Serif CJK SC',serif;font-size:14px;color:#475569;line-height:1.7;font-style:italic;">"{html.escape(report.quote)}"</div>
    <div style="font-size:10px;color:#94a3b8;margin-top:8px;">本报告由小满自动整理群聊生成 · 仅草稿，未发送</div>
  </div>
</div>
</body>
</html>"""


def _file_url(path: Path) -> str:
    return path.resolve().as_uri()


def _render_png(html_path: Path, output_path: Path, width: int) -> None:
    try:
        from playwright.sync_api import sync_playwright
    except ImportError as exc:
        raise RuntimeError(
            "playwright is required for PNG rendering; install it or use --render html"
        ) from exc

    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": width, "height": 100}, device_scale_factor=2)
        page.route(
            "**/*",
            lambda route: route.abort()
            if route.request.url.startswith(("http://", "https://"))
            else route.continue_(),
        )
        page.goto(_file_url(html_path), wait_until="load")
        height = page.evaluate("document.body.scrollHeight")
        page.set_viewport_size({"width": width, "height": height})
        page.screenshot(path=str(output_path), full_page=True)
        browser.close()


def _operator_review_message(
    report: ReportData,
    html_path: Path,
    png_path: Path | None,
    include_html: bool = False,
) -> str:
    lines = [
        f"【{report.group_name}｜{report.report_title}】",
        f"档案日期：{report.report_date}（{report.time_range}）",
        f"消息 {report.message_count} 条 / 活跃 {report.participant_count} 人 / 案件 {report.case_count} 起 / 嫌疑人 {report.suspect_count} 名",
        "",
    ]
    for case in report.cases:
        lines.append(f"• {case.case_no}：{case.title}（{case.summary}）")
    lines.append("")
    if png_path:
        lines.append(f"图片（可直接发群）：{png_path}")
    if include_html and html_path.exists():
        label = "HTML 预览（仅调试用）" if png_path else "HTML 预览"
        lines.append(f"{label}：{html_path}")
    lines.append("")
    lines.append("本报告仅生成草稿，未发送到任何群聊。确认无误后请回复「发」再执行外发。")
    return "\n".join(lines)


def _result_json(
    report: ReportData,
    deliverable_path: Path,
    png_path: Path | None,
    html_path: Path | None = None,
) -> dict[str, Any]:
    html_exists = html_path is not None and html_path.exists()
    return {
        "success": True,
        "skill": "xiaoman_daily_case_report",
        "external_send_executed": False,
        "requires_human_confirmation": True,
        "group_name": report.group_name,
        "report_date": report.report_date,
        "time_range": report.time_range,
        "message_count": report.message_count,
        "participant_count": report.participant_count,
        "case_count": report.case_count,
        "suspect_count": report.suspect_count,
        "deliverable_path": str(deliverable_path),
        "png_path": str(png_path) if png_path else None,
        "html_path": str(html_path) if html_exists else None,
        "operator_review_message": _operator_review_message(
            report, html_path or deliverable_path, png_path, html_exists
        ),
    }


def main() -> int:
    args = _parse_args()

    real_messages = _uses_real_messages(args)
    if real_messages and (args.keep_html or args.render == "html"):
        print(
            "ERROR: production read-through cannot retain HTML because it contains real group content; "
            "use --render png without --keep-html",
            file=sys.stderr,
        )
        return 2
    if real_messages:
        try:
            _validate_production_boundaries(args)
        except RuntimeError as exc:
            print(f"ERROR: {exc}", file=sys.stderr)
            return 2

    try:
        report = _build_report(args)
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    output_dir = _prepare_output_dir(args.output_dir)
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    html_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.html"
    png_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.png"

    html_content = _render_html(report, args.output_width)
    _write_private_text(html_path, html_content)

    png_generated = False
    try:
        if args.render in ("auto", "png"):
            try:
                _render_png(html_path, png_path, args.output_width)
                png_generated = True
            except RuntimeError as exc:
                print(f"WARN: PNG rendering skipped: {exc}", file=sys.stderr)
                if args.render == "png" or real_messages:
                    return 2

        html_is_deliverable = not png_generated

        deliverable = png_path if png_generated else html_path
        result = _result_json(
            report,
            deliverable,
            png_path if png_generated else None,
            None if real_messages else html_path if html_path.exists() else None,
        )

        if args.json:
            print(json.dumps(result, ensure_ascii=False, indent=2))
        else:
            print(result["operator_review_message"])
        return 0
    finally:
        html_is_deliverable = not png_generated
        should_remove_html = real_messages or (not args.keep_html and not html_is_deliverable)
        if should_remove_html and html_path.exists():
            try:
                html_path.unlink()
            except OSError:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
