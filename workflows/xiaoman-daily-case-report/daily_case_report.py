#!/usr/bin/env python3
"""Xiaoman daily community case-file report generator.

Reads the last 24 hours of QiWe group messages from the Qintopia message store,
aggregates them into a character-driven daily report, renders a
JPEG poster by default, and prints a local generation report. It does not send by
itself: automatic publishing must be connected through the reviewed AgentOS artifact
and QiWe image-send boundary.

The script is deterministic for a given input window: the same chat/date always
yields the same report (modulo rendering timestamps). It fails closed if the
message store is unreachable or the required runtime flags are not set.
"""
from __future__ import annotations

import argparse
import hashlib
import html
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, time, timedelta
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, unquote, urlparse
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError


DEFAULT_GROUP_NAME = "秦托邦的小伙伴（新）"
DEFAULT_REPORT_TITLE = "小满群聊日报"
CHAT_ID_ENV = "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
DEFAULT_TIMEZONE = "Asia/Shanghai"
DEFAULT_OUTPUT_WIDTH = 750
DEFAULT_CASE_LIMIT = 6
PRODUCTION_PSQL_BIN = "/usr/bin/psql"
PRODUCTION_PSQL_PATH = "/usr/bin:/bin"
DEFAULT_SUSPECT_LIMIT = 5
DEFAULT_CHARACTER_LIMIT = 4
DEFAULT_HOURLY_BUCKETS = 24
DEFAULT_WINDOW_HOURS = 24
DEFAULT_MIN_CASE_MESSAGES = 3
DEFAULT_TOP_KEYWORDS = 18
DEFAULT_HOT_TOPIC_LIMIT = 4
DEFAULT_HOT_TOPIC_MIN_MESSAGES = 2
DEFAULT_HOT_TOPIC_MIN_CHARS = 3
DEFAULT_HOT_TOPIC_MAX_CHARS = 8
DEFAULT_IMAGE_FORMAT = "jpeg"
DEFAULT_JPEG_QUALITY = 92
TEMPLATE_VERSION = "xiaoman-daily-case-report-v3"
MEMORY_LOOKBACK_DAYS = 90
REVIEW_DRAFT_REVIEWED_BY = "xiaoman-daily-case-report-review-draft"

STOP_WORDS: set[str] = {
    "这个", "那个", "然后", "就是", "什么", "怎么", "还是", "可以", "今天",
    "明天", "现在", "已经", "没有", "但是", "因为", "所以", "一下", "大家",
    "我们", "你们", "他们", "自己", "这里", "那里", "这样", "那样", "一个",
    "不是", "不用", "不要", "应该", "可能", "需要", "觉得", "看看", "一下",
    "哈哈", "嘿嘿", "嗯嗯", "好的", "收到", "谢谢", "请问", "知道", "真的",
    "一下", "一直", "一下", "时候", "过来", "过去", "为了", "作为", "关于",
    "还是", "或者", "以及", "并且", "虽然", "尽管", "不过", "只是", "而且",
    "国家", "规定", "词元", "哇喔", "名字", "好帅", "很帅",
    "呲牙", "哈哈", "哈哈哈", "哈哈哈哈", "啧啧", "啧啧啧", "欢迎欢迎",
}

PROMOTIONAL_NOISE_PHRASES: tuple[str, ...] = (
    "复制此条消息",
    "长按复制",
    "快帮我付个款",
    "帮我付款",
    "订单在",
    "分钟内有效",
    "打开抖音",
    "打开淘宝",
    "打开京东",
    "打开拼多多",
    "喜欢的宝贝",
    "查看详情",
)

HIGHLIGHT_SIGNAL_WORDS: tuple[str, ...] = (
    "建议",
    "经验",
    "分享",
    "讨论",
    "问题",
    "风险",
    "策略",
    "学习",
    "可以",
    "觉得",
    "复盘",
    "总结",
)

TOPIC_MARKER_HINTS: tuple[str, ...] = (
    "话题",
    "主题",
    "讨论",
    "复盘",
    "分享",
    "求助",
    "建议",
    "活动",
    "预告",
    "提醒",
    "计划",
    "安排",
)

CHARACTER_ROLE_RULES: tuple[tuple[str, str, str, tuple[str, ...]], ...] = (
    (
        "activity_organizer",
        "活动推进者",
        "把松散聊天推成下一步行动",
        ("活动", "报名", "接龙", "安排", "预告", "提醒", "收集", "表单"),
    ),
    (
        "resource_scout",
        "资料投喂员",
        "把有用线索递到群友手边",
        ("分享", "资料", "链接", "推荐", "文章", "工具", "教程", "收藏"),
    ),
    (
        "question_raiser",
        "问题发射台",
        "把模糊卡点抛到台面上",
        ("求助", "请问", "怎么", "有没有", "为什么", "吗", "？", "?"),
    ),
    (
        "answerer",
        "现场解法师",
        "把经验拆成群里能接住的话",
        ("建议", "可以", "试试", "检查", "经验", "我觉得", "先", "注意"),
    ),
    (
        "atmosphere",
        "气氛承包人",
        "负责让一天的聊天不只是信息流",
        ("欢迎", "哈哈", "加油", "稳住", "笑死", "太好", "厉害"),
    ),
)

MEMORY_FACT_ROLE_LABELS: dict[str, str] = {
    "activity_organizer": "活动推进者",
    "activity_participation": "活动在场者",
    "content_story_lead": "故事线雷达",
    "operation_signal": "规则观察员",
    "resource_scout": "资料投喂员",
    "service_need": "需求提醒人",
    "unresolved_question": "问题发射台",
}

MEMORY_FACT_TYPES: tuple[str, ...] = tuple(MEMORY_FACT_ROLE_LABELS)

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
    person_id: str | None = None


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
class CharacterCard:
    rank: int
    name: str
    role_label: str
    one_liner: str
    evidence: str
    message_count: int
    topic_count: int
    node_key: str = ""
    memory_label: str = ""
    member_fact_memory_used: bool = False
    story_function: str = ""
    callback_hint: str = ""
    arc_label: str = ""
    relationship_hint: str = ""
    relationship_target_key: str = ""
    relationship_topic: str = ""
    meme_seed: str = ""
    memory_weight_label: str = ""
    evidence_anchor: str = ""
    profile_evidence_count: int = 0
    profile_upgrade_status: str = ""
    profile_upgrade_reason: str = ""
    creative_profile_label: str = ""
    creative_profile_status: str = ""


@dataclass
class CharacterMemory:
    person_id: str
    recent_fact_count: int
    lifetime_fact_count: int
    dominant_role_label: str
    recurrence_label: str = ""
    depth_label: str = ""
    memory_weight_label: str = ""
    callback_seed: str = ""


@dataclass
class CreativeProfileMemory:
    person_id: str
    role_label: str
    story_function: str = ""
    daily_arc: str = ""
    memory_weight_label: str = ""
    meme_seed: str = ""
    callback_hint: str = ""
    evidence_anchor: str = ""
    recurrence_evidence_count: int = 0


@dataclass
class HotTopic:
    rank: int
    keyword: str
    message_count: int
    participant_count: int


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
    highlight: str | None
    hot_topics: list[HotTopic] = field(default_factory=list)
    character_count: int = 0
    characters: list[CharacterCard] = field(default_factory=list)
    character_universe: dict[str, Any] = field(default_factory=dict)
    window_start: str = ""
    window_end: str = ""
    timezone: str = DEFAULT_TIMEZONE


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
        choices=["auto", "image", "png", "html"],
        default="image",
        help="Render mode. image produces the poster; png is a legacy alias; html keeps the raw page for debugging.",
    )
    parser.add_argument(
        "--image-format",
        choices=["jpeg", "png"],
        default=None,
        help="Image encoding for rendered output. Defaults to png for --render png, otherwise jpeg.",
    )
    parser.add_argument(
        "--keep-html",
        action="store_true",
        help="Keep the intermediate HTML file (debug only; the image is the deliverable).",
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
    args = parser.parse_args()
    _normalize_render_args(args)
    return args


def _normalize_render_args(args: argparse.Namespace) -> None:
    if args.image_format is None:
        args.image_format = "png" if args.render == "png" else DEFAULT_IMAGE_FORMAT


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
    """Return a human-readable time range like 08/07 08:00–08/08 07:59."""
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
                person_id=msg.person_id,
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
                person_id=str(item.get("sender_person_id") or item.get("person_id") or "") or None,
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
    except ImportError:
        return _fetch_messages_with_psql(db_url, chat_id, start, end)

    sql = """
        SELECT
            m.id::text AS id,
            m.sender_id,
            m.sender_name,
            m.text,
            m.message_kind,
            COALESCE(m.sent_at, m.received_at) AS report_time,
            m.sender_person_id::text AS sender_person_id
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
                        person_id=row[6],
                    )
                )
    return messages


def _psql_env(db_url: str) -> dict[str, str]:
    parsed = urlparse(db_url)
    database = unquote(parsed.path.lstrip("/"))
    if (
        parsed.scheme not in {"postgres", "postgresql"}
        or not parsed.hostname
        or not parsed.username
        or parsed.password is None
        or not database
    ):
        raise RuntimeError("message store database URL shape is not supported by psql fallback")

    env = {
        "PATH": PRODUCTION_PSQL_PATH,
        "PGCONNECT_TIMEOUT": "10",
        "PGDATABASE": database,
        "PGHOST": parsed.hostname,
        "PGPASSWORD": unquote(parsed.password),
        "PGUSER": unquote(parsed.username),
    }
    if parsed.port is not None:
        env["PGPORT"] = str(parsed.port)
    sslmode = parse_qs(parsed.query).get("sslmode", [""])[0]
    if sslmode:
        env["PGSSLMODE"] = sslmode
    return env


def _fetch_messages_with_psql(
    db_url: str,
    chat_id: str | None,
    start: datetime,
    end: datetime,
) -> list[ReportMessage]:
    sql = r"""
        WITH selected AS (
            SELECT
                m.id::text AS id,
                COALESCE(m.sender_id, '') AS sender_id,
                COALESCE(m.sender_name, '匿名') AS sender_name,
                COALESCE(m.text, '') AS text,
                COALESCE(m.message_kind, 'text') AS message_kind,
                COALESCE(m.sent_at, m.received_at) AS report_time,
                m.sender_person_id::text AS sender_person_id
            FROM qintopia_messages.messages m
            WHERE m.platform = 'qiwe'
              AND m.chat_type = 'group'
              AND m.message_kind = 'text'
              AND NULLIF(BTRIM(m.text), '') IS NOT NULL
              AND COALESCE(m.sent_at, m.received_at) >= :'window_start'::timestamptz
              AND COALESCE(m.sent_at, m.received_at) < :'window_end'::timestamptz
              AND (:'chat_id' = '' OR m.chat_id = :'chat_id')
            ORDER BY COALESCE(m.sent_at, m.received_at) ASC
        )
        SELECT COALESCE(json_agg(row_to_json(selected)), '[]'::json)::text
        FROM selected;
    """
    command = [
        PRODUCTION_PSQL_BIN,
        "--no-psqlrc",
        "--no-align",
        "--tuples-only",
        "--quiet",
        "--set",
        "ON_ERROR_STOP=1",
        "--set",
        f"window_start={start.isoformat()}",
        "--set",
        f"window_end={end.isoformat()}",
        "--set",
        f"chat_id={chat_id or ''}",
    ]
    try:
        completed = subprocess.run(
            command,
            input=sql,
            env=_psql_env(db_url),
            text=True,
            capture_output=True,
            timeout=30,
            check=False,
        )
    except FileNotFoundError as exc:
        raise RuntimeError(
            f"database reads require psycopg or executable {PRODUCTION_PSQL_BIN}"
        ) from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError("message store query timed out") from exc
    if completed.returncode != 0:
        raise RuntimeError("message store query failed")
    try:
        rows = json.loads(completed.stdout.strip() or "[]")
    except json.JSONDecodeError as exc:
        raise RuntimeError("message store query returned invalid JSON") from exc
    messages: list[ReportMessage] = []
    for row in rows:
        report_time = None
        raw_time = row.get("report_time")
        if raw_time:
            try:
                report_time = datetime.fromisoformat(str(raw_time))
            except ValueError:
                report_time = None
        messages.append(
            ReportMessage(
                id=str(row.get("id", "")),
                sender_id=str(row.get("sender_id", "")),
                sender_name=str(row.get("sender_name", "匿名") or "匿名"),
                text=str(row.get("text", "")),
                sent_at=report_time,
                message_kind=str(row.get("message_kind", "text") or "text"),
                person_id=str(row.get("sender_person_id") or "") or None,
            )
        )
    return messages


def _fetch_character_memory(
    person_ids: set[str],
    end: datetime,
) -> dict[str, CharacterMemory]:
    clean_ids = sorted(
        {
            person_id
            for person_id in person_ids
            if re.fullmatch(r"[0-9a-fA-F-]{32,36}", person_id or "")
        }
    )
    if not clean_ids:
        return {}
    db_url = _database_url()
    if not db_url:
        return {}

    start = end - timedelta(days=MEMORY_LOOKBACK_DAYS)
    try:
        import psycopg
    except ImportError:
        return _fetch_character_memory_with_psql(db_url, clean_ids, start, end)

    sql = """
        WITH facts AS (
            SELECT
                mf.person_id::text AS person_id,
                mf.fact_type,
                mf.observed_at
            FROM qintopia_identity.member_facts mf
            WHERE mf.person_id::text = ANY(%s::text[])
              AND mf.revoked_at IS NULL
              AND mf.fact_type = ANY(%s::text[])
              AND mf.observed_at < %s
        ),
        role_counts AS (
            SELECT person_id, fact_type, count(*)::int AS fact_count
            FROM facts
            GROUP BY person_id, fact_type
        ),
        dominant AS (
            SELECT DISTINCT ON (person_id) person_id, fact_type
            FROM role_counts
            ORDER BY person_id, fact_count DESC, fact_type ASC
        )
        SELECT
            facts.person_id,
            count(*) FILTER (WHERE facts.observed_at >= %s)::int AS recent_fact_count,
            count(*)::int AS lifetime_fact_count,
            dominant.fact_type AS dominant_fact_type
        FROM facts
        JOIN dominant ON dominant.person_id = facts.person_id
        GROUP BY facts.person_id, dominant.fact_type
    """
    with psycopg.connect(db_url) as conn:
        with conn.cursor() as cur:
            cur.execute(sql, (clean_ids, list(MEMORY_FACT_TYPES), end, start))
            return _character_memory_from_rows(cur.fetchall())


def _fetch_character_memory_with_psql(
    db_url: str,
    person_ids: list[str],
    start: datetime,
    end: datetime,
) -> dict[str, CharacterMemory]:
    sql = r"""
        WITH facts AS (
            SELECT
                mf.person_id::text AS person_id,
                mf.fact_type,
                mf.observed_at
            FROM qintopia_identity.member_facts mf
            WHERE mf.person_id::text = ANY(string_to_array(:'person_ids', ','))
              AND mf.revoked_at IS NULL
              AND mf.fact_type = ANY(ARRAY[
                'activity_organizer',
                'activity_participation',
                'content_story_lead',
                'operation_signal',
                'resource_scout',
                'service_need',
                'unresolved_question'
              ]::text[])
              AND mf.observed_at < :'memory_end'::timestamptz
        ),
        role_counts AS (
            SELECT person_id, fact_type, count(*)::int AS fact_count
            FROM facts
            GROUP BY person_id, fact_type
        ),
        dominant AS (
            SELECT DISTINCT ON (person_id) person_id, fact_type
            FROM role_counts
            ORDER BY person_id, fact_count DESC, fact_type ASC
        ),
        selected AS (
            SELECT
                facts.person_id,
                count(*) FILTER (WHERE facts.observed_at >= :'memory_start'::timestamptz)::int AS recent_fact_count,
                count(*)::int AS lifetime_fact_count,
                dominant.fact_type AS dominant_fact_type
            FROM facts
            JOIN dominant ON dominant.person_id = facts.person_id
            GROUP BY facts.person_id, dominant.fact_type
        )
        SELECT COALESCE(json_agg(row_to_json(selected)), '[]'::json)::text
        FROM selected;
    """
    command = [
        PRODUCTION_PSQL_BIN,
        "--no-psqlrc",
        "--no-align",
        "--tuples-only",
        "--quiet",
        "--set",
        "ON_ERROR_STOP=1",
        "--set",
        f"person_ids={','.join(person_ids)}",
        "--set",
        f"memory_start={start.isoformat()}",
        "--set",
        f"memory_end={end.isoformat()}",
    ]
    try:
        completed = subprocess.run(
            command,
            input=sql,
            env=_psql_env(db_url),
            text=True,
            capture_output=True,
            timeout=30,
            check=False,
        )
    except FileNotFoundError as exc:
        raise RuntimeError(
            f"database reads require psycopg or executable {PRODUCTION_PSQL_BIN}"
        ) from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError("member profile memory query timed out") from exc
    if completed.returncode != 0:
        raise RuntimeError("member profile memory query failed")
    try:
        rows = json.loads(completed.stdout.strip() or "[]")
    except json.JSONDecodeError as exc:
        raise RuntimeError("member profile memory query returned invalid JSON") from exc
    return _character_memory_from_rows(
        (
            row.get("person_id"),
            row.get("recent_fact_count"),
            row.get("lifetime_fact_count"),
            row.get("dominant_fact_type"),
        )
        for row in rows
    )


def _character_memory_from_rows(rows: Any) -> dict[str, CharacterMemory]:
    memory: dict[str, CharacterMemory] = {}
    for person_id, recent_count, lifetime_count, dominant_fact_type in rows:
        person_id = str(person_id or "")
        if not person_id:
            continue
        role_label = MEMORY_FACT_ROLE_LABELS.get(str(dominant_fact_type or ""), "长期在场者")
        memory[person_id] = CharacterMemory(
            person_id=person_id,
            recent_fact_count=int(recent_count or 0),
            lifetime_fact_count=int(lifetime_count or 0),
            dominant_role_label=role_label,
            recurrence_label=_memory_recurrence_label(int(recent_count or 0)),
            depth_label=_memory_depth_label(int(lifetime_count or 0)),
            memory_weight_label=_memory_weight_label(int(recent_count or 0), int(lifetime_count or 0)),
            callback_seed=_memory_callback_seed(role_label, int(recent_count or 0)),
        )
    return memory


def _safe_creative_text(value: Any, limit: int = 80) -> str:
    if not isinstance(value, str):
        return ""
    cleaned = _clean_text(value).strip()
    if not cleaned:
        return ""
    lowered = cleaned.lower()
    if any(marker in lowered for marker in ("raw_message", "fact_text", "profile_text", "database_url")):
        return ""
    cleaned = re.sub(r"[`$<>{}\x00-\x08\x0b\x0c\x0e-\x1f\x7f]+", "", cleaned).strip()
    return cleaned[:limit]


def _safe_creative_int(value: Any) -> int:
    if isinstance(value, bool):
        return 0
    try:
        parsed = int(value)
    except (TypeError, ValueError):
        return 0
    if parsed < 0:
        return 0
    return min(parsed, 1000)


def _fetch_creative_profile_memory(
    person_ids: set[str],
    end: datetime,
) -> dict[str, CreativeProfileMemory]:
    clean_ids = sorted(
        {
            person_id
            for person_id in person_ids
            if re.fullmatch(r"[0-9a-fA-F-]{32,36}", person_id or "")
        }
    )
    if not clean_ids:
        return {}
    db_url = _database_url()
    if not db_url:
        return {}

    try:
        import psycopg
    except ImportError:
        return _fetch_creative_profile_memory_with_psql(db_url, clean_ids, end)

    sql = """
        SELECT DISTINCT ON (s.person_id)
            s.person_id::text AS person_id,
            s.communication_style,
            s.safe_reply_hints
        FROM qintopia_identity.member_profile_snapshots s
        WHERE s.person_id::text = ANY(%s::text[])
          AND s.profile_kind = 'creative_profile'
          AND s.profile_version = 'xiaoman-daily-creative-profile-v1'
          AND s.status = 'active'
          AND s.reviewed_at IS NOT NULL
          AND s.generated_at < %s
          AND COALESCE((s.do_not_disclose->>'public_surface_allowed')::boolean, false) = false
          AND COALESCE((s.safe_reply_hints->>'public_surface_allowed')::boolean, false) = false
        ORDER BY s.person_id, s.reviewed_at DESC NULLS LAST, s.generated_at DESC
    """
    with psycopg.connect(db_url) as conn:
        with conn.cursor() as cur:
            cur.execute(sql, (clean_ids, end))
            return _creative_profile_memory_from_rows(cur.fetchall())


def _fetch_creative_profile_memory_with_psql(
    db_url: str,
    person_ids: list[str],
    end: datetime,
) -> dict[str, CreativeProfileMemory]:
    sql = r"""
        WITH selected AS (
            SELECT DISTINCT ON (s.person_id)
                s.person_id::text AS person_id,
                s.communication_style,
                s.safe_reply_hints
            FROM qintopia_identity.member_profile_snapshots s
            WHERE s.person_id::text = ANY(string_to_array(:'person_ids', ','))
              AND s.profile_kind = 'creative_profile'
              AND s.profile_version = 'xiaoman-daily-creative-profile-v1'
              AND s.status = 'active'
              AND s.reviewed_at IS NOT NULL
              AND s.generated_at < :'memory_end'::timestamptz
              AND COALESCE((s.do_not_disclose->>'public_surface_allowed')::boolean, false) = false
              AND COALESCE((s.safe_reply_hints->>'public_surface_allowed')::boolean, false) = false
            ORDER BY s.person_id, s.reviewed_at DESC NULLS LAST, s.generated_at DESC
        )
        SELECT COALESCE(json_agg(row_to_json(selected)), '[]'::json)::text
        FROM selected;
    """
    command = [
        PRODUCTION_PSQL_BIN,
        "--no-psqlrc",
        "--no-align",
        "--tuples-only",
        "--quiet",
        "--set",
        "ON_ERROR_STOP=1",
        "--set",
        f"person_ids={','.join(person_ids)}",
        "--set",
        f"memory_end={end.isoformat()}",
    ]
    try:
        completed = subprocess.run(
            command,
            input=sql,
            env=_psql_env(db_url),
            text=True,
            capture_output=True,
            timeout=30,
            check=False,
        )
    except FileNotFoundError as exc:
        raise RuntimeError(
            f"database reads require psycopg or executable {PRODUCTION_PSQL_BIN}"
        ) from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError("creative profile memory query timed out") from exc
    if completed.returncode != 0:
        raise RuntimeError("creative profile memory query failed")
    try:
        rows = json.loads(completed.stdout.strip() or "[]")
    except json.JSONDecodeError as exc:
        raise RuntimeError("creative profile memory query returned invalid JSON") from exc
    return _creative_profile_memory_from_rows(
        (
            row.get("person_id"),
            row.get("communication_style") or {},
            row.get("safe_reply_hints") or {},
        )
        for row in rows
    )


def _creative_profile_memory_from_rows(rows: Any) -> dict[str, CreativeProfileMemory]:
    memory: dict[str, CreativeProfileMemory] = {}
    for person_id, communication_style, safe_reply_hints in rows:
        person_id = str(person_id or "")
        if not person_id:
            continue
        if not isinstance(communication_style, dict):
            communication_style = {}
        if not isinstance(safe_reply_hints, dict):
            safe_reply_hints = {}
        role_label = _safe_creative_text(
            safe_reply_hints.get("role_label") or communication_style.get("role_label"),
            32,
        )
        if not role_label:
            continue
        memory[person_id] = CreativeProfileMemory(
            person_id=person_id,
            role_label=role_label,
            story_function=_safe_creative_text(
                safe_reply_hints.get("story_function") or communication_style.get("story_function"),
                48,
            ),
            daily_arc=_safe_creative_text(safe_reply_hints.get("daily_arc"), 120),
            memory_weight_label=_safe_creative_text(safe_reply_hints.get("memory_weight_label"), 64),
            meme_seed=_safe_creative_text(safe_reply_hints.get("meme_seed"), 80),
            callback_hint=_safe_creative_text(safe_reply_hints.get("callback_hint"), 120),
            evidence_anchor=_safe_creative_text(safe_reply_hints.get("evidence_anchor"), 80),
            recurrence_evidence_count=_safe_creative_int(
                safe_reply_hints.get("recurrence_evidence_count")
            ),
        )
    return memory


def _memory_recurrence_label(recent_count: int) -> str:
    if recent_count >= 10:
        return "近90天高频复现"
    if recent_count >= 4:
        return "近90天稳定复现"
    if recent_count >= 1:
        return "近90天偶发复现"
    return "今日新鲜出场"


def _memory_depth_label(lifetime_count: int) -> str:
    if lifetime_count >= 24:
        return "长期角色锚点"
    if lifetime_count >= 8:
        return "长期线索可用"
    if lifetime_count >= 1:
        return "历史线索较轻"
    return "暂无长期画像"


def _memory_weight_label(recent_count: int, lifetime_count: int) -> str:
    if lifetime_count <= 0:
        return "只按今日表现呈现"
    return f"{_memory_recurrence_label(recent_count)} · {_memory_depth_label(lifetime_count)}"


def _memory_callback_seed(role_label: str, recent_count: int) -> str:
    if recent_count >= 4:
        return f"可作为「{role_label}」连续出场回调"
    if recent_count >= 1:
        return f"保留为「{role_label}」轻量回看点"
    return f"先记今日「{role_label}」一笔"


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
    existed = output_dir.exists()
    output_dir.mkdir(parents=True, exist_ok=True)
    if existed:
        mode = output_dir.stat().st_mode & 0o777
        if mode != 0o700:
            raise RuntimeError(
                f"output directory already exists with mode {mode:04o}; "
                "use a dedicated private 0700 directory"
            )
    else:
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


def _build_creative_profile_review_payload_draft(
    character_universe: dict[str, Any],
    reviewed_at: str,
) -> dict[str, Any]:
    if not character_universe:
        character_universe = {
            "schema_version": "xiaoman-character-universe-v1",
            "raw_messages_included": False,
            "profile_fact_text_included": False,
            "creative_profile_candidate_policy": {
                "public_surface_allowed": False,
            },
            "creative_profile_candidates": [],
        }
    helper_path = Path(__file__).resolve().with_name("build_creative_profile_review_payload.py")
    spec = importlib.util.spec_from_file_location(
        "xiaoman_daily_case_report_build_creative_profile_review_payload",
        helper_path,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("creative profile review payload builder is unavailable")
    helper = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = helper
    spec.loader.exec_module(helper)
    return helper._build_payload(
        character_universe,
        reviewed_by=REVIEW_DRAFT_REVIEWED_BY,
        reviewed_at=reviewed_at,
        include_rejected=True,
        allow_empty=True,
    )


def _clean_text(text: str) -> str:
    text = text or ""
    text = re.sub(r"https?://\S+", "", text)
    text = re.sub(r"(?<!\S)@(?:[A-Za-z0-9_.-]{1,64}|[\u4e00-\u9fff]{1,6})(?=\s|$)", "", text)
    text = re.sub(r"\s+", " ", text)
    return text.strip()


def _looks_promotional_noise(text: str) -> bool:
    raw = text or ""
    cleaned = _clean_text(raw)
    compact = re.sub(r"\s+", "", cleaned)
    if any(phrase in compact for phrase in PROMOTIONAL_NOISE_PHRASES):
        return True
    if re.search(r"[A-Za-z0-9:/._-]{10,}", raw) and any(
        phrase in compact for phrase in ("付款", "订单", "复制", "打开", "宝贝")
    ):
        return True
    return False


def _discussion_messages(messages: list[ReportMessage]) -> list[ReportMessage]:
    return [m for m in messages if not _looks_promotional_noise(m.text)]


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


def _hot_topics(
    messages: list[ReportMessage],
    cases: list[CaseCard] | None = None,
    limit: int = DEFAULT_HOT_TOPIC_LIMIT,
) -> list[HotTopic]:
    grouped: dict[str, list[ReportMessage]] = {}
    repeated_phrases: dict[str, list[ReportMessage]] = {}
    case_topic_stats: dict[str, tuple[int, int]] = {}
    for message in messages:
        for token in set(_tokenize(message.text)):
            if _is_clean_topic(token) and len(token) >= DEFAULT_HOT_TOPIC_MIN_CHARS:
                grouped.setdefault(token, []).append(message)
        for phrase in _hot_topic_phrases(message.text):
            repeated_phrases.setdefault(phrase, []).append(message)

    # Fallback tokenization has no Chinese word boundaries. A phrase must appear in
    # distinct messages before it can replace those coarse fragments on the hotlist.
    for phrase, group in repeated_phrases.items():
        if len({_clean_text(message.text) for message in group}) >= DEFAULT_HOT_TOPIC_MIN_MESSAGES:
            existing = grouped.setdefault(phrase, [])
            existing_ids = {message.id for message in existing}
            existing.extend(message for message in group if message.id not in existing_ids)

    for case in cases or []:
        topic = _case_storyline_label(case)
        if (
            case.message_count >= DEFAULT_HOT_TOPIC_MIN_MESSAGES
            and _is_clean_topic(topic)
            and not _is_time_bucket_topic(topic)
        ):
            current_message_count, current_participant_count = case_topic_stats.get(topic, (0, 0))
            case_topic_stats[topic] = (
                max(current_message_count, case.message_count),
                max(current_participant_count, case.participant_count),
            )

    ranked = sorted(
        (
            (
                keyword,
                max(len(grouped.get(keyword, [])), case_topic_stats.get(keyword, (0, 0))[0]),
                max(
                    len(
                        {
                            message.sender_id or message.sender_name
                            for message in grouped.get(keyword, [])
                        }
                    ),
                    case_topic_stats.get(keyword, (0, 0))[1],
                ),
            )
            for keyword in set(grouped) | set(case_topic_stats)
            if max(len(grouped.get(keyword, [])), case_topic_stats.get(keyword, (0, 0))[0])
            >= DEFAULT_HOT_TOPIC_MIN_MESSAGES
        ),
        key=lambda item: (
            -(len(item[0]) * item[1]),
            -item[1],
            -item[2],
            -len(item[0]),
            item[0],
        ),
    )
    topics: list[HotTopic] = []
    for keyword, message_count, participant_count in ranked:
        if any(keyword in topic.keyword or topic.keyword in keyword for topic in topics):
            continue
        topics.append(
            HotTopic(
                rank=len(topics) + 1,
                keyword=keyword,
                message_count=message_count,
                participant_count=participant_count,
            )
        )
        if len(topics) == limit:
            break
    return topics


def _hot_topic_phrases(text: str) -> set[str]:
    phrases: set[str] = set()
    for source in re.findall(r"[\u4e00-\u9fa5]+", _clean_text(text)):
        max_length = min(len(source), DEFAULT_HOT_TOPIC_MAX_CHARS)
        for length in range(DEFAULT_HOT_TOPIC_MIN_CHARS, max_length + 1):
            for start in range(len(source) - length + 1):
                phrase = source[start:start + length]
                if _is_clean_topic(phrase):
                    phrases.add(phrase)
    return phrases


def _is_clean_topic(kw: str) -> bool:
    """Reject noise tokens so case titles stay meaningful.

    Excludes stop words, common English noise (none/null/nan), and any token
    without a Chinese character — a case title should read like a real topic.
    """
    if not kw or kw in STOP_WORDS:
        return False
    if kw.lower() in {"none", "null", "nan", "true", "false"}:
        return False
    if any(noise in kw for noise in ("现在规定叫", "规定叫")):
        return False
    if "群里" in kw:
        return False
    if any(noise in kw for noise in ("哈哈", "嘿嘿", "呵呵", "嘻嘻", "呲牙", "啧啧")):
        return False
    if kw.endswith(("不", "吗", "么", "吧", "呢", "啊", "呀", "啦", "哦", "哈", "的", "了")):
        return False
    if len(kw) >= 3 and len(set(kw)) == 1:
        return False
    if not any("\u4e00" <= c <= "\u9fa5" for c in kw):
        return False
    return True


def _is_time_bucket_topic(topic: str) -> bool:
    return bool(re.match(r"^(早场|午后|晚场|夜场)(?:[ ·][^ ]+)?\s*\d{2}:00", topic))


def _time_bucket_title(hour: int, messages: list[ReportMessage]) -> str:
    if 5 <= hour < 12:
        period = "早场"
    elif 12 <= hour < 18:
        period = "午后"
    elif 18 <= hour < 23:
        period = "晚场"
    else:
        period = "夜场"
    for keyword, count in _keyword_scores(messages).most_common(DEFAULT_TOP_KEYWORDS):
        if count >= DEFAULT_MIN_CASE_MESSAGES and _is_clean_topic(keyword):
            return f"{period} · {keyword}"
    return f"{period} {hour:02d}:00 时段"


def _topic_marker_title(cleaned: str) -> str | None:
    pattern = re.compile(r"^([^：:\n]{2,30})[：:]\s*")
    match = pattern.match(cleaned)
    if not match:
        return None
    topic = match.group(1).strip()
    if not (
        4 <= len(topic) <= 24
        and not topic[-1].isdigit()
        and not topic.endswith(("，", ",", "、"))
        and _is_clean_topic(topic)
    ):
        return None
    if not any(hint in topic for hint in TOPIC_MARKER_HINTS):
        return None
    return topic


def _is_digest_snippet_text(text: str) -> bool:
    cleaned = _clean_text(text)
    if len(cleaned) < 12:
        return False
    if _looks_promotional_noise(cleaned):
        return False
    if any(noise in cleaned for noise in ("现在规定叫", "呲牙", "哈哈", "嘿嘿", "呵呵", "嘻嘻", "啧啧")):
        return any(word in cleaned for word in HIGHLIGHT_SIGNAL_WORDS)
    if re.match(r"^[^：:\n]{2,30}[：:]\s*", cleaned) and _topic_marker_title(cleaned) is None:
        return False
    return True


def _time_bucket_bullet(
    time_label: str,
    message_count: int,
    participant_count: int,
) -> str:
    return f"{time_label}：{message_count} 条群消息，{participant_count} 人参与。"


def _detect_topic_markers(messages: list[ReportMessage]) -> dict[str, list[ReportMessage]]:
    """Group messages under explicit topic markers like 'Topic：'.

    Messages that follow a marker (until the next marker) are folded into the
    same case, mirroring how real group threads flow. 接龙 (WeChat sign-up
    chains) are strong thread starters and get their own case title.
    """
    clusters: dict[str, list[ReportMessage]] = {}
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
            has_colon_marker = re.match(r"^[^：:\n]{2,30}[：:]\s*", cleaned) is not None
            topic = _topic_marker_title(cleaned)
            if topic:
                current_topic = topic
            elif has_colon_marker:
                current_topic = None
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
    time_bucket_titles: set[str] = set()
    assigned_ids = {id(m) for cluster in clusters.values() for m in cluster}
    unassigned = [m for m in messages if id(m) not in assigned_ids]

    keyword_scores = _keyword_scores(unassigned)
    top_keywords = [
        kw
        for kw, count in keyword_scores.most_common(DEFAULT_TOP_KEYWORDS)
        if count >= DEFAULT_MIN_CASE_MESSAGES and _is_clean_topic(kw)
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

    qualified_cluster_count = sum(
        1 for cluster in clusters.values() if len(cluster) >= DEFAULT_MIN_CASE_MESSAGES
    )
    if qualified_cluster_count < limit:
        assigned_ids = {id(m) for cluster in clusters.values() for m in cluster}
        buckets: dict[int, list[ReportMessage]] = {}
        for msg in messages:
            if id(msg) in assigned_ids or not msg.sent_at:
                continue
            buckets.setdefault(msg.sent_at.hour, []).append(msg)
        for hour, bucket in sorted(buckets.items(), key=lambda item: (-len(item[1]), item[0])):
            if len(bucket) < DEFAULT_MIN_CASE_MESSAGES:
                continue
            title = _time_bucket_title(hour, bucket)
            while title in clusters:
                title = f"{title} · {hour:02d}:00"
            clusters[title] = bucket
            time_bucket_titles.add(title)
            qualified_cluster_count += 1
            if qualified_cluster_count >= limit:
                break

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
        if keyword in time_bucket_titles:
            bullets = [
                _time_bucket_bullet(
                    time_label,
                    len(cluster),
                    len(participants),
                )
            ]
        else:
            # Pick representative snippets by signal density so noisy chatter does not
            # become the visible takeaway for a topic card.
            representative_messages = [m for m in cluster if _is_digest_snippet_text(m.text)]
            if not representative_messages:
                representative_messages = [
                    m for m in cluster if _clean_text(m.text) and not _looks_promotional_noise(m.text)
                ]
            sorted_by_length = sorted(
                representative_messages,
                key=lambda m: (
                    -len(m.text),
                    m.sent_at.timestamp() if m.sent_at else float("-inf"),
                ),
            )[:3]
            bullets = []
            for m in sorted_by_length:
                snippet = _clean_text(m.text)[:70]
                if snippet and snippet not in bullets:
                    bullets.append(snippet)
            if not bullets:
                continue

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


def _character_role(messages: list[ReportMessage]) -> tuple[str, str, int]:
    text = "\n".join(_clean_text(message.text) for message in messages)
    best_label = "在场感选手"
    best_line = "用持续出现把当天话题接住"
    best_score = 0
    for _role, label, line, hints in CHARACTER_ROLE_RULES:
        score = sum(text.count(hint) for hint in hints)
        if score > best_score:
            best_label = label
            best_line = line
            best_score = score
    return best_label, best_line, best_score


def _character_evidence(messages: list[ReportMessage]) -> str:
    candidates: list[tuple[int, str]] = []
    for message in messages:
        text = _clean_text(message.text)
        if not _is_digest_snippet_text(text):
            continue
        score = min(len(text), 90)
        if any(word in text for word in HIGHLIGHT_SIGNAL_WORDS):
            score += 20
        if any(hint in text for _role, _label, _line, hints in CHARACTER_ROLE_RULES for hint in hints):
            score += 12
        candidates.append((score, text))
    if not candidates:
        for message in messages:
            text = _clean_text(message.text)
            if text:
                candidates.append((len(text), text))
    if not candidates:
        return "今天有持续参与，但没有适合公开摘录的长句。"
    candidates.sort(reverse=True)
    best = candidates[0][1]
    return best[:58] + ("..." if len(best) > 58 else "")


def _character_story_function(role_label: str, message_count: int, topic_count: int) -> str:
    role_functions = {
        "活动推进者": "推进剧情",
        "资料投喂员": "递道具",
        "问题发射台": "抛冲突",
        "现场解法师": "给解法",
        "气氛承包人": "接气口",
        "在场感选手": "稳住场",
    }
    function = role_functions.get(role_label, "补场面")
    if message_count >= 8:
        return f"{function} · 高频出场"
    if topic_count >= 4:
        return f"{function} · 多线串联"
    return function


def _character_callback_hint(role_label: str, evidence: str, memory_label: str) -> str:
    if memory_label:
        return f"今天不是孤例，可以回看「{role_label}」的长期复现"
    if evidence:
        return f"如果后续继续出现，可沉淀为「{role_label}」回调"
    return f"今日暂记为「{role_label}」出场"


def _character_arc_label(role_label: str, memory: CharacterMemory | None, message_count: int) -> str:
    if memory and memory.recent_fact_count >= 4:
        recurrence_label = memory.recurrence_label or _memory_recurrence_label(memory.recent_fact_count)
        return f"{recurrence_label}，今天继续以「{role_label}」推进"
    if memory and memory.lifetime_fact_count > 0:
        depth_label = memory.depth_label or _memory_depth_label(memory.lifetime_fact_count)
        return f"{depth_label}，今日再次露出「{role_label}」信号"
    if message_count >= 5:
        return f"今日高频出场，先形成「{role_label}」日线"
    return f"今日新鲜出场，暂记「{role_label}」"


def _character_meme_seed(
    role_label: str,
    topic_count: int,
    evidence: str,
    memory: CharacterMemory | None,
) -> str:
    if memory:
        return memory.callback_seed or _memory_callback_seed(role_label, memory.recent_fact_count)
    if topic_count >= 3:
        return f"多话题串场的「{role_label}」"
    token = next((token for token in _tokenize(evidence) if _is_clean_topic(token)), "")
    if token:
        return f"围绕「{token}」的「{role_label}」"
    return f"今日「{role_label}」待观察"


def _profile_evidence_count(
    memory: CharacterMemory | None,
    creative_memory: CreativeProfileMemory | None,
    message_count: int,
    topic_count: int,
    relationship_hint: str,
) -> int:
    count = min(memory.recent_fact_count, 20) if memory else 0
    if creative_memory:
        count = max(count, min(creative_memory.recurrence_evidence_count, 20))
    if message_count >= 2:
        count += 1
    if memory and topic_count >= 2:
        count += 1
    if creative_memory and topic_count >= 1:
        count += 1
    if relationship_hint:
        count += 1
    return count


def _profile_upgrade_status(evidence_count: int) -> str:
    return "eligible_for_review" if evidence_count >= 2 else "daily_note_only"


def _profile_upgrade_reason(
    evidence_count: int,
    memory: CharacterMemory | None,
    message_count: int,
    topic_count: int,
    relationship_hint: str,
) -> str:
    if evidence_count < 2:
        return "只有单日轻量信号，不能升级为长期人物画像"
    reasons: list[str] = []
    if memory and memory.recent_fact_count > 0:
        reasons.append(f"近{MEMORY_LOOKBACK_DAYS}天已有 {memory.recent_fact_count} 次角色复现")
    if message_count >= 2:
        reasons.append(f"今日同一身份 {message_count} 条发言支撑")
    if topic_count >= 2:
        reasons.append(f"今日跨 {topic_count} 个公开话题出现")
    if relationship_hint:
        reasons.append("今日存在同场关系候选")
    return "；".join(reasons[:3]) or "达到最小复现证据"


def _relation_group_key(message: ReportMessage) -> str:
    if message.person_id:
        return f"person:{message.person_id}"
    name = (message.sender_name or "").strip()
    return f"name:{name}" if name and name != "匿名" else ""


def _relationship_hints(
    messages: list[ReportMessage],
    character_keys: set[str],
    node_key_by_group: dict[str, str],
    name_by_group: dict[str, str],
) -> dict[str, tuple[str, str, str]]:
    topic_groups: dict[str, dict[str, int]] = {}
    for message in messages:
        group_key = _relation_group_key(message)
        if not group_key or group_key not in character_keys:
            continue
        for token in set(_tokenize(message.text)):
            if _is_clean_topic(token):
                topic_groups.setdefault(token, {}).setdefault(group_key, 0)
                topic_groups[token][group_key] += 1

    candidates: dict[str, list[tuple[int, str, str, str]]] = {}
    for topic, counts in topic_groups.items():
        if len(counts) < 2:
            continue
        ranked = sorted(counts.items(), key=lambda item: (-item[1], name_by_group.get(item[0], "")))
        for group_key, count in ranked:
            for peer_key, peer_count in ranked:
                if peer_key == group_key:
                    continue
                peer_name = name_by_group.get(peer_key, "群友")
                peer_node_key = node_key_by_group.get(peer_key, _node_key(peer_name))
                score = count + peer_count + len(topic)
                candidates.setdefault(group_key, []).append(
                    (
                        score,
                        f"和{peer_name}围绕「{topic}」同场接力",
                        peer_node_key,
                        topic,
                    )
                )
                break

    hints: dict[str, tuple[str, str, str]] = {}
    for group_key, group_candidates in candidates.items():
        group_candidates.sort(key=lambda item: (-item[0], item[1]))
        _score, label, peer_node_key, topic = group_candidates[0]
        hints[group_key] = (label, peer_node_key, topic)
    return hints


def _compute_characters(
    messages: list[ReportMessage],
    memory_by_person: dict[str, CharacterMemory] | None = None,
    creative_memory_by_person: dict[str, CreativeProfileMemory] | None = None,
    limit: int = DEFAULT_CHARACTER_LIMIT,
) -> list[CharacterCard]:
    memory_by_person = memory_by_person or {}
    creative_memory_by_person = creative_memory_by_person or {}
    grouped: dict[str, list[ReportMessage]] = {}
    group_person_ids: dict[str, str] = {}
    for message in messages:
        name = (message.sender_name or "").strip()
        if not name or name == "匿名":
            continue
        if message.person_id:
            group_key = f"person:{message.person_id}"
            group_person_ids[group_key] = message.person_id
        else:
            group_key = f"name:{name}"
        grouped.setdefault(group_key, []).append(message)

    name_by_group: dict[str, str] = {}
    node_key_by_group: dict[str, str] = {}
    for group_key, group in grouped.items():
        names = Counter((message.sender_name or "").strip() for message in group)
        names.pop("", None)
        names.pop("匿名", None)
        name = names.most_common(1)[0][0] if names else "群友"
        name_by_group[group_key] = name
        node_key_by_group[group_key] = _character_node_key(group_key, name)
    relationship_hints = _relationship_hints(
        messages,
        set(grouped),
        node_key_by_group,
        name_by_group,
    )

    ranked: list[tuple[float, CharacterCard]] = []
    for group_key, group in grouped.items():
        name = name_by_group.get(group_key, "群友")
        role_label, one_liner, role_score = _character_role(group)
        topic_count = len(
            {
                token
                for message in group
                for token in _tokenize(message.text)
                if _is_clean_topic(token)
            }
        )
        if len(group) < 2 and role_score == 0:
            continue
        word_count = sum(len(_clean_text(message.text)) for message in group)
        person_id = group_person_ids.get(group_key)
        memory = memory_by_person.get(person_id) if person_id else None
        creative_memory = creative_memory_by_person.get(person_id) if person_id else None
        memory_score = min(memory.recent_fact_count, 10) if memory else 0
        if creative_memory:
            memory_score += min(creative_memory.recurrence_evidence_count, 8)
        memory_label = ""
        if memory:
            memory_label = (
                f"近{MEMORY_LOOKBACK_DAYS}天 {memory.recent_fact_count} 次角色复现"
                f" · 长期偏「{memory.dominant_role_label}」"
            )
        creative_profile_label = ""
        if creative_memory:
            creative_profile_label = f"已审核创意画像「{creative_memory.role_label}」"
            memory_label = (
                f"{memory_label} · {creative_profile_label}"
                if memory_label
                else creative_profile_label
            )
        evidence = _character_evidence(group)
        relationship_hint, relationship_target_key, relationship_topic = relationship_hints.get(
            group_key,
            ("", "", ""),
        )
        node_key = node_key_by_group.get(group_key, _character_node_key(group_key, name))
        profile_evidence_count = _profile_evidence_count(
            memory,
            creative_memory,
            len(group),
            topic_count,
            relationship_hint,
        )
        profile_upgrade_reason = _profile_upgrade_reason(
            profile_evidence_count,
            memory,
            len(group),
            topic_count,
            relationship_hint,
        )
        if creative_memory:
            profile_upgrade_reason = (
                f"已审核 creative_profile 复用；{profile_upgrade_reason}"
                if profile_upgrade_reason
                else "已审核 creative_profile 复用"
            )
        memory_weight_label = "只按今日表现呈现"
        if memory:
            memory_weight_label = memory.memory_weight_label or _memory_weight_label(
                memory.recent_fact_count,
                memory.lifetime_fact_count,
            )
        if creative_memory and creative_memory.memory_weight_label:
            memory_weight_label = creative_memory.memory_weight_label
        score = (
            len(group) * 3
            + role_score * 4
            + min(topic_count, 6)
            + min(word_count / 80, 4)
            + memory_score
        )
        ranked.append(
            (
                score,
                CharacterCard(
                    rank=0,
                    name=name,
                    role_label=role_label,
                    one_liner=one_liner,
                    evidence=evidence,
                    message_count=len(group),
                    topic_count=topic_count,
                    node_key=node_key,
                    memory_label=memory_label,
                    member_fact_memory_used=memory is not None,
                    story_function=creative_memory.story_function
                    if creative_memory and creative_memory.story_function
                    else _character_story_function(role_label, len(group), topic_count),
                    callback_hint=creative_memory.callback_hint
                    if creative_memory and creative_memory.callback_hint
                    else _character_callback_hint(role_label, evidence, memory_label),
                    arc_label=creative_memory.daily_arc
                    if creative_memory and creative_memory.daily_arc
                    else _character_arc_label(role_label, memory, len(group)),
                    relationship_hint=relationship_hint,
                    relationship_target_key=relationship_target_key,
                    relationship_topic=relationship_topic,
                    meme_seed=creative_memory.meme_seed
                    if creative_memory and creative_memory.meme_seed
                    else _character_meme_seed(role_label, topic_count, evidence, memory),
                    memory_weight_label=memory_weight_label,
                    evidence_anchor=f"daily_character_note:{node_key}",
                    profile_evidence_count=profile_evidence_count,
                    profile_upgrade_status=_profile_upgrade_status(profile_evidence_count),
                    profile_upgrade_reason=profile_upgrade_reason,
                    creative_profile_label=creative_profile_label,
                    creative_profile_status="active_reviewed" if creative_memory else "",
                ),
            )
        )

    ranked.sort(key=lambda item: (-item[0], item[1].name))
    characters = [card for _score, card in ranked[:limit]]
    for index, character in enumerate(characters, start=1):
        character.rank = index
    return characters


def _character_node_key(group_key: str, name: str) -> str:
    if group_key.startswith("person:"):
        digest = hashlib.sha256(group_key.encode("utf-8")).hexdigest()[:12]
        return f"person-{digest}"
    return _node_key(name)


def _node_key(label: str) -> str:
    cleaned = re.sub(r"\s+", "-", _clean_text(label)).strip("-")
    cleaned = re.sub(r"[^\w\u4e00-\u9fff-]+", "", cleaned)
    return cleaned[:48] or "node"


def _build_character_universe(
    cases: list[CaseCard],
    hot_topics: list[HotTopic],
    characters: list[CharacterCard],
    report_date: str,
) -> dict[str, Any]:
    def character_key(character: CharacterCard) -> str:
        return character.node_key or _node_key(character.name)

    def character_anchor(character: CharacterCard) -> str:
        return character.evidence_anchor or f"daily_character_note:{character_key(character)}"

    def character_evidence_count(character: CharacterCard) -> int:
        return character.profile_evidence_count or (1 if character.message_count >= 2 else 0)

    def character_upgrade_status(character: CharacterCard) -> str:
        return character.profile_upgrade_status or _profile_upgrade_status(
            character_evidence_count(character)
        )

    def character_upgrade_reason(character: CharacterCard) -> str:
        if character.profile_upgrade_reason:
            return character.profile_upgrade_reason
        return _profile_upgrade_reason(
            character_evidence_count(character),
            None,
            character.message_count,
            character.topic_count,
            character.relationship_hint,
        )

    people = [
        {
            "type": "people",
            "key": character_key(character),
            "label": character.name,
            "role_label": character.role_label,
            "daily_line": character.one_liner,
            "evidence": character.evidence,
            "message_count": character.message_count,
            "topic_count": character.topic_count,
            "memory_label": character.memory_label,
            "story_function": character.story_function,
            "callback_hint": character.callback_hint,
            "arc_label": character.arc_label,
            "relationship_hint": character.relationship_hint,
            "meme_seed": character.meme_seed,
            "memory_weight_label": character.memory_weight_label,
            "evidence_anchor": character_anchor(character),
            "profile_evidence_count": character_evidence_count(character),
            "profile_upgrade_status": character_upgrade_status(character),
            "creative_profile_label": character.creative_profile_label,
            "creative_profile_status": character.creative_profile_status,
            "risk": "internal",
        }
        for character in characters
    ]
    topics = [
        {
            "type": "topics",
            "key": _node_key(topic.keyword),
            "label": topic.keyword,
            "message_count": topic.message_count,
            "participant_count": topic.participant_count,
            "risk": "public_safe_summary",
        }
        for topic in hot_topics
    ]
    events = [
        {
            "type": "events",
            "key": _node_key(case.title),
            "label": case.title,
            "case_no": case.case_no,
            "time_label": case.time_label,
            "summary": case.summary,
            "top_speaker": case.top_speaker,
            "evidence": case.bullets[:3],
            "risk": "internal",
        }
        for case in cases
    ]
    storyline_candidates = [
        {
            "type": "storylines",
            "key": _node_key(case.title),
            "label": case.title.replace("关于「", "").replace("」的讨论", ""),
            "status": "candidate",
            "last_seen": report_date,
            "reason": f"{case.message_count} 条消息，{case.participant_count} 人参与",
            "related_event": case.case_no,
            "risk": "internal_review_required",
        }
        for case in cases
        if case.message_count >= DEFAULT_MIN_CASE_MESSAGES
    ]
    memes: list[dict[str, Any]] = []
    seen_meme_keys: set[str] = set()
    for character in characters:
        label = character.meme_seed.strip()
        if not label:
            continue
        key = _node_key(label)
        if key in seen_meme_keys:
            continue
        seen_meme_keys.add(key)
        memes.append(
            {
                "type": "memes",
                "key": key,
                "label": label,
                "source": "daily_character_note",
                "related_people": [character_key(character)],
                "status": "candidate",
                "risk": "internal_review_required",
            }
        )
    for topic in hot_topics:
        label = f"「{topic.keyword}」今日高频回调"
        key = _node_key(label)
        if key in seen_meme_keys:
            continue
        seen_meme_keys.add(key)
        memes.append(
            {
                "type": "memes",
                "key": key,
                "label": label,
                "source": "daily_hot_topic",
                "message_count": topic.message_count,
                "participant_count": topic.participant_count,
                "status": "candidate",
                "risk": "internal_review_required",
            }
        )
    callbacks = [
        {
            "type": "callbacks",
            "key": _node_key(f"{character.node_key}-{character.role_label}-callback"),
            "label": character.callback_hint,
            "related_people": [character_key(character)],
            "memory_weight_label": character.memory_weight_label,
            "status": "candidate",
            "risk": "internal_review_required",
        }
        for character in characters
        if character.callback_hint
    ]
    creative_profile_candidates = [
        {
            "type": "creative_profile_candidates",
            "key": _node_key(f"{character.node_key}-{character.role_label}-creative-profile"),
            "profile_kind": "creative_profile",
            "profile_version": "daily-character-v1",
            "related_person": character_key(character),
            "candidate_role_label": character.role_label,
            "story_function": character.story_function,
            "daily_arc": character.arc_label,
            "memory_weight_label": character.memory_weight_label,
            "meme_seed": character.meme_seed,
            "callback_hint": character.callback_hint,
            "evidence_anchor": character_anchor(character),
            "recurrence_evidence_count": character_evidence_count(character),
            "minimum_recurrence_met": character_evidence_count(character) >= 2,
            "profile_upgrade_status": character_upgrade_status(character),
            "profile_upgrade_reason": character_upgrade_reason(character),
            "blocked_reason": (
                character_upgrade_reason(character)
                if character_upgrade_status(character) == "daily_note_only"
                else ""
            ),
            "evidence_policy": "daily_character_note_or_quote_map",
            "minimum_recurrence": 2,
            "status": "candidate",
            "public_surface_allowed": False,
            "risk": "internal_review_required",
        }
        for character in characters
        if character.role_label
    ]
    selected_people_keys = {character_key(character) for character in characters}
    relationships: list[dict[str, Any]] = []
    seen_relationships: set[tuple[str, str, str]] = set()
    for character in characters:
        source = character_key(character)
        target = character.relationship_target_key
        topic = character.relationship_topic
        if not target or target not in selected_people_keys or source == target:
            continue
        relation_key = tuple(sorted((source, target)) + [topic])
        if relation_key in seen_relationships:
            continue
        seen_relationships.add(relation_key)
        relationships.append(
            {
                "type": "relationships",
                "key": _node_key("-".join(relation_key)),
                "source": source,
                "target": target,
                "relation": "co_discusses_topic",
                "label": character.relationship_hint,
                "topic": topic,
                "risk": "public_safe_summary",
            }
        )
    edges: list[dict[str, Any]] = []
    for character in characters:
        character_key_value = character_key(character)
        for case in cases:
            if character.name == case.top_speaker or character.name in " ".join(case.bullets):
                edges.append(
                    {
                        "source": character_key_value,
                        "target": _node_key(case.title),
                        "relation": "appears_in",
                        "evidence": case.case_no,
                    }
                )
        for topic in hot_topics:
            if topic.keyword in character.evidence:
                edges.append(
                    {
                        "source": character_key_value,
                        "target": _node_key(topic.keyword),
                        "relation": "mentions_topic",
                        "evidence": "daily_character_note",
                    }
                )
        if character.meme_seed:
            edges.append(
                {
                    "source": character_key_value,
                    "target": _node_key(character.meme_seed),
                    "relation": "seeds_callback",
                    "evidence": "daily_character_note",
                }
            )
    for relationship in relationships:
        edges.append(
            {
                "source": relationship["source"],
                "target": relationship["target"],
                "relation": relationship["relation"],
                "evidence": relationship["topic"],
            }
        )
    return {
        "schema_version": "xiaoman-character-universe-v1",
        "source": "daily_case_report_second_pass",
        "retained_source_policy": "curated_summary_only",
        "raw_messages_included": False,
        "profile_fact_text_included": False,
        "people": people,
        "topics": topics,
        "events": events,
        "memes": memes,
        "callbacks": callbacks,
        "relationships": relationships,
        "creative_profile_candidates": creative_profile_candidates,
        "creative_profile_candidate_policy": {
            "profile_kind": "creative_profile",
            "apply_mode": "candidate_only",
            "writes_member_profile_snapshots": False,
            "public_surface_allowed": False,
            "evidence_policy": "daily_character_note_or_quote_map",
            "review_required": True,
        },
        "storyline_candidates": storyline_candidates,
        "edges": edges,
    }


def _quote_entry(
    index: int,
    source_kind: str,
    excerpt: str,
    *,
    speaker_label: str = "",
    speaker_key: str = "",
    related_people: list[str] | None = None,
    related_topics: list[str] | None = None,
    related_events: list[str] | None = None,
    related_memes: list[str] | None = None,
    source_anchor: str = "",
) -> dict[str, Any] | None:
    cleaned = _clean_text(excerpt)
    if not cleaned:
        return None
    return {
        "key": f"quote-{index:03d}",
        "source_kind": source_kind,
        "speaker_label": speaker_label,
        "speaker_key": speaker_key,
        "excerpt": cleaned[:120] + ("..." if len(cleaned) > 120 else ""),
        "related_people": related_people or ([] if not speaker_key else [speaker_key]),
        "related_topics": related_topics or [],
        "related_events": related_events or [],
        "related_memes": related_memes or [],
        "source_anchor": source_anchor,
        "review_status": "candidate",
        "public_surface_allowed": False,
    }


def _build_quote_map(report: ReportData) -> dict[str, Any]:
    entries: list[dict[str, Any]] = []

    def add(entry: dict[str, Any] | None) -> None:
        if entry is not None:
            entries.append(entry)

    next_index = 1
    if report.highlight:
        add(
            _quote_entry(
                next_index,
                "daily_highlight",
                report.highlight,
                related_topics=[_node_key(topic.keyword) for topic in report.hot_topics[:2]],
            )
        )
        next_index += 1

    for character in report.characters:
        person_key = character.node_key or _node_key(character.name)
        add(
            _quote_entry(
                next_index,
                "daily_character_note",
                character.evidence,
                speaker_label=character.name,
                speaker_key=person_key,
                related_memes=[_node_key(character.meme_seed)] if character.meme_seed else [],
                source_anchor=character.evidence_anchor,
            )
        )
        next_index += 1

    for case in report.cases:
        event_key = _node_key(case.title)
        for bullet in case.bullets[:2]:
            add(
                _quote_entry(
                    next_index,
                    "daily_case_bullet",
                    bullet,
                    speaker_label=case.top_speaker,
                    related_events=[event_key],
                )
            )
            next_index += 1

    return {
        "schema_version": "xiaoman-daily-quote-map-v1",
        "source": "daily_case_report_private_review_bundle",
        "retained_source_policy": "private_curated_excerpts_only",
        "raw_message_rows_included": False,
        "profile_fact_text_included": False,
        "curated_excerpts_included": True,
        "public_surface_allowed": False,
        "review_required": True,
        "entry_count": len(entries),
        "entries": entries,
    }


def _wiki_bundle_counts(bundle: dict[str, Any]) -> dict[str, int]:
    return {
        "people": len(bundle.get("people") or []),
        "topics": len(bundle.get("topics") or []),
        "events": len(bundle.get("events") or []),
        "memes": len(bundle.get("memes") or []),
        "relationships": len(bundle.get("relationships") or []),
        "storylines": len(bundle.get("storylines") or []),
        "timeline": len(bundle.get("timeline") or []),
    }


def _lookback_callback_candidates(report: ReportData) -> list[dict[str, Any]]:
    callbacks: list[dict[str, Any]] = []
    seen: set[str] = set()
    for character in report.characters:
        seed = (character.meme_seed or character.callback_hint or character.role_label).strip()
        if not seed:
            continue
        for days in (7, 14, 30):
            key = _node_key(f"{character.node_key or character.name}-{seed}-{days}d")
            if key in seen:
                continue
            seen.add(key)
            callbacks.append(
                {
                    "key": key,
                    "lookback_days": days,
                    "label": f"{character.name}的「{seed}」{days}天回看候选",
                    "related_person": character.node_key or _node_key(character.name),
                    "trigger": character.callback_hint or character.arc_label,
                    "status": "candidate",
                    "risk": "internal_review_required",
                }
            )
            if len(callbacks) >= 9:
                return callbacks
    for case in report.cases:
        label = _case_storyline_label(case)
        if not label:
            continue
        key = _node_key(f"{label}-7d-lookback")
        if key in seen:
            continue
        seen.add(key)
        callbacks.append(
            {
                "key": key,
                "lookback_days": 7,
                "label": f"「{label}」7天回看候选",
                "related_event": _node_key(case.title),
                "trigger": case.summary,
                "status": "candidate",
                "risk": "internal_review_required",
            }
        )
    return callbacks


def _build_draft_bundle(
    report: ReportData,
    quote_map: dict[str, Any],
    wiki_bundle: dict[str, Any],
) -> dict[str, Any]:
    quote_keys = [str(entry.get("key") or "") for entry in quote_map.get("entries") or []]
    quote_keys = [key for key in quote_keys if key]
    main_storyline = _main_storyline_label(report)
    callback_candidates = _meme_callback_candidates(report)
    relationship_candidates = _relationship_candidates(report)
    character_cards = [
        {
            "person_key": character.node_key or _node_key(character.name),
            "display_label": character.name,
            "role_label": character.role_label,
            "story_function": character.story_function,
            "daily_arc": character.arc_label,
            "callback_hint": character.callback_hint,
            "memory_weight_label": character.memory_weight_label,
            "quote_anchor": character.evidence_anchor,
            "status": "candidate",
            "risk": "internal_review_required",
        }
        for character in report.characters
    ]
    title_candidates = [
        f"小满群聊日报｜{main_storyline}",
        f"{report.character_count} 位剧中人，把今天的群聊推成一条主线",
        f"今日回看：{main_storyline}",
    ]
    opening_candidates = [
        _daily_opening_line(report),
    ]
    if report.highlight:
        opening_candidates.append(f"今天可以先从这句看起：{report.highlight}")
    storyline_timeline = [
        {
            "date": report.report_date,
            "case_no": case.case_no,
            "storyline": _case_storyline_label(case),
            "message_count": case.message_count,
            "participant_count": case.participant_count,
            "status": "candidate",
            "risk": "internal_review_required",
        }
        for case in report.cases
    ]
    lookback_callbacks = _lookback_callback_candidates(report)
    bundle = {
        "schema_version": "xiaoman-daily-draft-bundle-v1",
        "source": "daily_case_report_private_review_bundle",
        "retained_source_policy": "private_curated_drafts_only",
        "raw_message_rows_included": False,
        "profile_fact_text_included": False,
        "public_surface_allowed": False,
        "review_required": True,
        "ordinary_digest": {
            "status": "candidate",
            "title": f"小满群聊日报｜{report.report_date}｜{main_storyline}",
            "main_storyline": main_storyline,
            "section_keys": [
                "今日台词",
                "今日剧中人",
                "梗和回调候选",
                "同场关系",
                "今日主线",
            ],
            "quote_keys": quote_keys[:12],
        },
        "roast_digest": {
            "status": "candidate_requires_owner_review",
            "tone": "轻吐槽人物群像",
            "character_cards": character_cards,
            "callback_angles": callback_candidates,
            "boundary": {
                "criticize_behavior_not_identity": True,
                "single_day_trait_blocked": True,
                "sensitive_attributes_blocked": True,
            },
        },
        "public_draft": {
            "status": "candidate_requires_owner_review",
            "title_candidates": title_candidates,
            "opening_candidates": opening_candidates[:3],
            "storyline_links": [
                item.get("key", "") for item in wiki_bundle.get("storylines") or []
            ][:8],
            "quote_keys": quote_keys[:8],
        },
        "storyline_memory": {
            "active_storyline_candidates": wiki_bundle.get("storylines") or [],
            "timeline": storyline_timeline,
            "lookback_callbacks": lookback_callbacks,
            "relationship_candidates": relationship_candidates,
        },
    }
    bundle["counts"] = {
        "ordinary_digest_section_count": len(bundle["ordinary_digest"]["section_keys"]),
        "roast_profile_candidate_count": len(character_cards),
        "public_draft_title_count": len(title_candidates),
        "storyline_timeline_count": len(storyline_timeline),
        "lookback_callback_count": len(lookback_callbacks),
    }
    return bundle


def _build_wiki_bundle(report: ReportData, quote_map: dict[str, Any]) -> dict[str, Any]:
    universe = report.character_universe or {}
    event_quote_keys: dict[str, list[str]] = {}
    people_quote_keys: dict[str, list[str]] = {}
    meme_quote_keys: dict[str, list[str]] = {}
    for entry in quote_map.get("entries") or []:
        quote_key = str(entry.get("key") or "")
        if not quote_key:
            continue
        for event_key in entry.get("related_events") or []:
            event_quote_keys.setdefault(str(event_key), []).append(quote_key)
        for person_key in entry.get("related_people") or []:
            people_quote_keys.setdefault(str(person_key), []).append(quote_key)
        for meme_key in entry.get("related_memes") or []:
            meme_quote_keys.setdefault(str(meme_key), []).append(quote_key)

    people = []
    for item in universe.get("people") or []:
        key = str(item.get("key") or "")
        people.append(
            {
                "type": "wiki_person",
                "key": key,
                "label": item.get("label", ""),
                "role_label": item.get("role_label", ""),
                "daily_arc": item.get("arc_label", ""),
                "story_function": item.get("story_function", ""),
                "callback_hint": item.get("callback_hint", ""),
                "memory_weight_label": item.get("memory_weight_label", ""),
                "evidence_anchor": item.get("evidence_anchor", ""),
                "profile_upgrade_status": item.get("profile_upgrade_status", ""),
                "creative_profile_status": item.get("creative_profile_status", ""),
                "quote_keys": people_quote_keys.get(key, []),
                "status": "candidate",
                "risk": "internal_review_required",
            }
        )

    topics = [
        {
            "type": "wiki_topic",
            "key": item.get("key", ""),
            "label": item.get("label", ""),
            "message_count": item.get("message_count", 0),
            "participant_count": item.get("participant_count", 0),
            "status": "candidate",
            "risk": "public_safe_summary",
        }
        for item in universe.get("topics") or []
    ]

    events = []
    for item in universe.get("events") or []:
        key = str(item.get("key") or "")
        events.append(
            {
                "type": "wiki_event",
                "key": key,
                "label": item.get("label", ""),
                "case_no": item.get("case_no", ""),
                "time_label": item.get("time_label", ""),
                "summary": item.get("summary", ""),
                "quote_keys": event_quote_keys.get(key, []),
                "status": "candidate",
                "risk": "internal_review_required",
            }
        )

    memes = []
    for item in universe.get("memes") or []:
        key = str(item.get("key") or "")
        memes.append(
            {
                "type": "wiki_meme",
                "key": key,
                "label": item.get("label", ""),
                "source": item.get("source", ""),
                "related_people": item.get("related_people", []),
                "quote_keys": meme_quote_keys.get(key, []),
                "status": "candidate",
                "risk": "internal_review_required",
            }
        )

    relationships = [
        {
            "type": "wiki_relationship",
            "key": item.get("key", ""),
            "source": item.get("source", ""),
            "target": item.get("target", ""),
            "relation": item.get("relation", ""),
            "label": item.get("label", ""),
            "topic": item.get("topic", ""),
            "status": "candidate",
            "risk": "public_safe_summary",
        }
        for item in universe.get("relationships") or []
    ]

    storylines = [
        {
            "type": "wiki_storyline",
            "key": item.get("key", ""),
            "label": item.get("label", ""),
            "last_seen": item.get("last_seen", report.report_date),
            "reason": item.get("reason", ""),
            "related_event": item.get("related_event", ""),
            "status": "candidate",
            "risk": "internal_review_required",
        }
        for item in universe.get("storyline_candidates") or []
    ]

    timeline = [
        {
            "type": "daily_timeline_entry",
            "key": _node_key(f"{report.report_date}-{case.case_no}"),
            "date": report.report_date,
            "case_no": case.case_no,
            "label": _case_storyline_label(case),
            "time_label": case.time_label,
            "message_count": case.message_count,
            "participant_count": case.participant_count,
            "status": "candidate",
            "risk": "internal_review_required",
        }
        for case in report.cases
    ]

    bundle = {
        "schema_version": "xiaoman-daily-wiki-bundle-v1",
        "source": "daily_case_report_private_review_bundle",
        "retained_source_policy": "candidate_nodes_and_quote_keys_only",
        "raw_message_rows_included": False,
        "profile_fact_text_included": False,
        "public_surface_allowed": False,
        "review_required": True,
        "people": people,
        "topics": topics,
        "events": events,
        "memes": memes,
        "relationships": relationships,
        "storylines": storylines,
        "timeline": timeline,
    }
    bundle["counts"] = _wiki_bundle_counts(bundle)
    return bundle


def _build_run_manifest(
    report: ReportData,
    quote_map: dict[str, Any],
    wiki_bundle: dict[str, Any],
    draft_bundle: dict[str, Any] | None = None,
    *,
    source_chat_id: str | None = None,
) -> dict[str, Any]:
    universe = report.character_universe or {}
    draft_counts = (draft_bundle or {}).get("counts") or {}
    return {
        "schema_version": "xiaoman-daily-run-manifest-v1",
        "source": "daily_case_report",
        "template_version": TEMPLATE_VERSION,
        "report_date": report.report_date,
        "time_range": report.time_range,
        "window_start": report.window_start,
        "window_end": report.window_end,
        "timezone": report.timezone,
        "source_chat_ref": _source_chat_ref(source_chat_id),
        "inputs": {
            "message_count": report.message_count,
            "participant_count": report.participant_count,
            "latest_chat_records_preserved": True,
            "long_term_member_facts_used": any(
                character.member_fact_memory_used for character in report.characters
            ),
            "reviewed_creative_profiles_used": any(
                character.creative_profile_status == "active_reviewed"
                for character in report.characters
            ),
            "long_term_member_fact_text_included": False,
        },
        "outputs": {
            "poster": "generated_at_runtime",
            "daily_markdown": "private_review_file",
            "character_universe": "private_review_json",
            "quote_map": "private_review_json",
            "wiki_bundle": "private_review_json",
            "draft_bundle": "private_review_json",
            "review_report": "private_review_markdown",
        },
        "counts": {
            "case_count": report.case_count,
            "character_count": report.character_count,
            "hot_topic_count": len(report.hot_topics),
            "quote_map_entry_count": quote_map.get("entry_count", 0),
            "wiki_people_count": (wiki_bundle.get("counts") or {}).get("people", 0),
            "wiki_event_count": (wiki_bundle.get("counts") or {}).get("events", 0),
            "wiki_storyline_count": (wiki_bundle.get("counts") or {}).get("storylines", 0),
            "draft_roast_profile_candidate_count": draft_counts.get(
                "roast_profile_candidate_count",
                0,
            ),
            "draft_storyline_timeline_count": draft_counts.get(
                "storyline_timeline_count",
                0,
            ),
            "draft_lookback_callback_count": draft_counts.get(
                "lookback_callback_count",
                0,
            ),
            "creative_profile_candidate_count": len(
                universe.get("creative_profile_candidates") or []
            ),
        },
        "privacy": {
            "public_surface_allowed": False,
            "raw_message_rows_included": False,
            "profile_fact_text_included": False,
            "creative_profile_public_surface_allowed": (
                (universe.get("creative_profile_candidate_policy") or {}).get(
                    "public_surface_allowed"
                )
                is True
            ),
            "writes_member_profile_snapshots": False,
        },
        "review_required": True,
    }


def _render_review_report(
    report: ReportData,
    quote_map: dict[str, Any],
    wiki_bundle: dict[str, Any],
    draft_bundle: dict[str, Any],
    run_manifest: dict[str, Any],
) -> str:
    universe = report.character_universe or {}
    counts = wiki_bundle.get("counts") or {}
    draft_counts = draft_bundle.get("counts") or {}
    profile_candidates = universe.get("creative_profile_candidates") or []
    lines = [
        f"# 小满日报私有审核包｜{report.report_date}",
        "",
        "## 生成范围",
        "",
        f"- 时间范围：{report.time_range}",
        f"- 最新聊天记录：保留，{report.message_count} 条消息 / {report.participant_count} 位活跃成员",
        f"- 已审核创意画像复用：{sum(1 for character in report.characters if character.creative_profile_status == 'active_reviewed')} 位",
        f"- 今日主线：{report.case_count} 条",
        f"- 今日剧中人：{report.character_count} 位",
        f"- 引用映射：{quote_map.get('entry_count', 0)} 条候选证据",
        f"- Wiki 候选：people={counts.get('people', 0)} / events={counts.get('events', 0)} / memes={counts.get('memes', 0)} / relationships={counts.get('relationships', 0)} / storylines={counts.get('storylines', 0)}",
        f"- 草稿候选：roast_profiles={draft_counts.get('roast_profile_candidate_count', 0)} / storyline_timeline={draft_counts.get('storyline_timeline_count', 0)} / lookback_callbacks={draft_counts.get('lookback_callback_count', 0)}",
        "",
        "## 审核清单",
        "",
        "- [ ] 公开日报是否只使用群聊窗口内的当日内容和安全衍生标签",
        "- [ ] 已审核 creative_profile 只作为风格/回调提示，不能覆盖当日消息证据",
        "- [ ] 今日剧中人的角色是否有 quote-map 或 case bullet 支撑",
        "- [ ] eligible_for_review 是否满足最小复现证据；daily_note_only 不得写入长期画像",
        "- [ ] creative_profile_candidates 是否仍为 candidate_only，没有写入长期画像表",
        "- [ ] 同名成员是否按 person_id 优先分组，缺失 person_id 才使用展示名兜底",
        "- [ ] meme / relationship / storyline 是否只是候选，没有被当作事实发布",
        "- [ ] roast/public draft 是否仍为 owner-review 候选，没有进入自动公开发送面",
        "",
        "## 隐私边界",
        "",
        f"- raw_message_rows_included={str(run_manifest['privacy']['raw_message_rows_included']).lower()}",
        f"- profile_fact_text_included={str(run_manifest['privacy']['profile_fact_text_included']).lower()}",
        f"- creative_profile_public_surface_allowed={str(run_manifest['privacy']['creative_profile_public_surface_allowed']).lower()}",
        f"- writes_member_profile_snapshots={str(run_manifest['privacy']['writes_member_profile_snapshots']).lower()}",
        "",
        "## 可审核人物画像候选",
        "",
    ]
    if profile_candidates:
        for item in profile_candidates[:8]:
            lines.append(
                f"- {item.get('related_person', '')}：{item.get('candidate_role_label', '')} / "
                f"{item.get('story_function', '')} / {item.get('daily_arc', '')} "
                f"（status={item.get('profile_upgrade_status', '')}; "
                f"evidence_count={item.get('recurrence_evidence_count', 0)}; "
                f"anchor={item.get('evidence_anchor', '')}）"
            )
    else:
        lines.append("- 今日没有形成可审核人物画像候选。")
    lines.extend(
        [
            "",
            "## 产物策略",
            "",
            "- 画报和日报 Markdown 用于人工查看。",
            "- 已审核 `creative_profile` 只读取 safe_reply_hints / communication_style 的安全字段，不读取 summary。",
            "- quote-map / wiki-bundle / run-manifest 只用于内部审核和后续人工确认。",
            "- draft-bundle 承载普通日报、轻吐槽素材和公众号候选素材，但只作为 owner review 输入。",
            "- worker-run evidence 只能保留 presence/count/privacy flags，不能保留 quote、wiki 节点正文或人物画像文本。",
        ]
    )
    return "\n".join(lines)


def _case_storyline_label(case: CaseCard) -> str:
    label = case.title.replace("关于「", "").replace("」的讨论", "").strip()
    return label or case.title


def _main_storyline_label(report: ReportData) -> str:
    lead = _case_storyline_label(report.cases[0]) if report.cases else ""
    top_character = report.characters[0] if report.characters else None
    if lead and top_character and top_character.relationship_hint:
        return f"{lead}，{top_character.name}{top_character.relationship_hint}"
    if lead and top_character:
        return f"{lead}，{top_character.name}以「{top_character.role_label}」出场"
    if lead:
        return lead
    if report.hot_topics and top_character:
        return f"{report.hot_topics[0].keyword}，{top_character.name}接住今日话题"
    if report.hot_topics:
        return report.hot_topics[0].keyword
    if top_character:
        return f"{top_character.name}的今日出场"
    return "今天群里先把日常续上"


def _daily_opening_line(report: ReportData) -> str:
    storyline = _main_storyline_label(report)
    if report.message_count <= 0:
        return "今天暂时没有形成可沉淀的群聊主线，日报保留空窗记录。"
    cast_line = ""
    if report.characters:
        cast = "、".join(
            f"{character.name}（{character.role_label}）" for character in report.characters[:3]
        )
        cast_line = f" 核心出场是 {cast}。"
    return (
        f"今天的主线是「{storyline}」：{report.message_count} 条消息、"
        f"{report.participant_count} 位活跃成员，把信息、提问和现场反应压成一页可回看的群聊切片。"
        f"{cast_line}"
    )


def _meme_callback_candidates(report: ReportData, limit: int = 5) -> list[str]:
    candidates: list[str] = []
    seen: set[str] = set()
    for topic in report.hot_topics:
        label = topic.keyword.strip()
        if label and label not in seen:
            candidates.append(f"「{label}」：{topic.message_count} 条消息，{topic.participant_count} 人参与")
            seen.add(label)
    for character in report.characters:
        label = character.meme_seed.strip() or character.role_label.strip()
        if label and label not in seen:
            detail = character.callback_hint
            if character.relationship_hint:
                detail = f"{detail}；{character.relationship_hint}"
            candidates.append(f"「{label}」：{detail}")
            seen.add(label)
    for case in report.cases:
        label = _case_storyline_label(case)
        if label and label not in seen:
            candidates.append(f"「{label}」：{case.summary}")
            seen.add(label)
    return candidates[:limit]


def _relationship_candidates(report: ReportData, limit: int = 4) -> list[str]:
    relationships = (report.character_universe or {}).get("relationships") or []
    candidates: list[str] = []
    seen: set[str] = set()
    for relationship in relationships:
        label = str(relationship.get("label") or "").strip()
        topic = str(relationship.get("topic") or "").strip()
        if not label or label in seen:
            continue
        seen.add(label)
        candidates.append(f"{label}（公开话题：{topic or '未标注'}）")
        if len(candidates) >= limit:
            break
    if candidates:
        return candidates
    for character in report.characters:
        label = character.relationship_hint.strip()
        if label and label not in seen:
            seen.add(label)
            candidates.append(f"{label}（公开话题：{character.relationship_topic or '未标注'}）")
            if len(candidates) >= limit:
                break
    return candidates


def _extract_highlight(messages: list[ReportMessage]) -> str | None:
    """Pick one real, quotable group message for the '今日高亮' block."""
    candidates = []
    for m in messages:
        text = _clean_text(m.text)
        if len(text) < 20:
            continue
        if "接龙" in text or text.startswith("打卡") or _looks_promotional_noise(text):
            continue
        score = min(len(text), 120)
        if any(word in text for word in HIGHLIGHT_SIGNAL_WORDS):
            score += 35
        if len(text) > 180:
            score -= 25
        candidates.append((score, len(text), text))
    if not candidates:
        return None
    candidates.sort(reverse=True)
    best = candidates[0][2]
    return best[:92] + ("…" if len(best) > 92 else "")


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

    discussion_messages = _discussion_messages(messages)
    unique_senders = {m.sender_id for m in discussion_messages}
    cases = _cluster_cases(discussion_messages)
    hot_topics = _hot_topics(discussion_messages, cases)
    suspects = _compute_suspects(discussion_messages)
    character_memory = {}
    creative_profile_memory = {}
    if not args.dry_run and not args.fixture:
        person_ids = {message.person_id for message in discussion_messages if message.person_id}
        try:
            character_memory = _fetch_character_memory(person_ids, end)
        except Exception:
            character_memory = {}
        try:
            creative_profile_memory = _fetch_creative_profile_memory(person_ids, end)
        except Exception:
            creative_profile_memory = {}
    characters = _compute_characters(discussion_messages, character_memory, creative_profile_memory)
    character_universe = _build_character_universe(cases, hot_topics, characters, display_date)
    hourly = _hourly_timeline(messages, start)
    max_hourly = max(hourly) if hourly else 1

    time_range = _time_range_label(start, end)
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
        highlight=_extract_highlight(discussion_messages),
        hot_topics=hot_topics,
        character_count=len(characters),
        characters=characters,
        character_universe=character_universe,
        window_start=start.isoformat(),
        window_end=end.isoformat(),
        timezone=args.timezone,
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
    chart_width = width - 96
    peak_count = max(report.hourly_counts or [0])
    max_hourly = peak_count or 1
    peak_idx = report.hourly_counts.index(peak_count) if peak_count else 0
    timeline_svg = _bar_svg(report.hourly_counts, max_hourly, chart_width, 68)
    timeline_labels = "".join(
        f'<text x="{int((idx / 24) * chart_width)}" y="94" font-size="9" fill="#4a4a4a" text-anchor="middle">{idx:02d}</text>'
        for idx in range(0, 25, 4)
    )
    peak_x = peak_idx * (chart_width // 24) + (chart_width // 48)
    peak_svg = f'<text x="{peak_x}" y="12" font-size="10" fill="#f25a18" font-weight="700" text-anchor="middle">{peak_count}</text>'
    main_storyline = _main_storyline_label(report)
    opening_line = _daily_opening_line(report)
    callback_candidates = _meme_callback_candidates(report)
    relationship_candidates = _relationship_candidates(report)

    stats_html = "\n".join(
        f"""
      <div class="stat">
        <div class="stat-label">{label}</div>
        <div class="stat-value">{value}</div>
        <div class="stat-caption">{caption}</div>
      </div>"""
        for label, value, caption in (
            ("消息", report.message_count, "当日素材"),
            ("出场", report.participant_count, "活跃成员"),
            ("主线", report.case_count, "可归档"),
            ("人物", report.character_count, "群像卡"),
        )
    )

    case_cards = "".join(
        f"""
      <article class="case-card">
        <div class="case-head">
          <span class="case-number">{html.escape(case.case_no.replace("CASE ", ""))}</span>
          <span class="case-time">{html.escape(case.time_label)}</span>
        </div>
        <h3>{html.escape(_case_storyline_label(case))}</h3>
        <p class="case-summary">{html.escape(case.summary)}</p>
        <ul class="case-notes">{"".join(f"<li>{html.escape(bullet)}</li>" for bullet in case.bullets[:3])}</ul>
      </article>"""
        for case in report.cases
    )
    cases_html = f"""
  <section class="section cases-section">
    <div class="section-kicker">STORYLINE FILES</div>
    <h2>故事线候选</h2>
    <div class="cases">{case_cards}</div>
  </section>""" if case_cards else ""

    suspects_html = "".join(
        f"""
      <div class="mvp-card">
        <div class="mvp-rank">{suspect.rank}</div>
        <div class="mvp-copy">
          <div class="mvp-name">{html.escape(suspect.name)}</div>
          <div class="mvp-meta">{suspect.message_count} 条 / {suspect.word_count} 字</div>
        </div>
        <div class="mvp-score">{suspect.message_count}</div>
      </div>"""
        for suspect in report.suspects
    )
    mvp_html = f"""
  <section class="section mvp-section">
    <div class="section-kicker">VOICE INDEX</div>
    <h2>发言出场榜</h2>
    <div class="mvp-grid">{suspects_html}</div>
  </section>""" if suspects_html else ""

    highlight_html = ""
    if report.highlight:
        highlight_html = f"""
  <section class="highlight">
    <div class="highlight-kicker">QUOTE ANCHOR</div>
    <div class="highlight-title">今日台词</div>
    <p>“{html.escape(report.highlight)}”</p>
  </section>"""

    callbacks_html = ""
    if callback_candidates:
        callbacks_html = f"""
  <section class="hotlist">
    <div class="hotlist-heading"><span>MEME MAP</span><h2>梗和回调候选</h2></div>
    <div class="hotlist-grid">{"".join(
        f'''<div class="hot-topic"><span class="hot-rank">{index}</span><strong>{html.escape(candidate.split("：", 1)[0])}</strong><small>{html.escape(candidate.split("：", 1)[1] if "：" in candidate else candidate)}</small></div>'''
        for index, candidate in enumerate(callback_candidates, start=1)
    )}</div>
  </section>"""

    relationships_html = ""
    if relationship_candidates:
        relationships_html = f"""
  <section class="relationships">
    <div class="relationships-heading"><span>ENSEMBLE LINKS</span><h2>同场关系</h2></div>
    <div class="relationship-list">{"".join(
        f'''<div class="relationship-row"><span>{index}</span><p>{html.escape(candidate)}</p></div>'''
        for index, candidate in enumerate(relationship_candidates, start=1)
    )}</div>
  </section>"""

    characters_html = ""
    if report.characters:
        characters_html = f"""
  <section class="characters">
    <div class="characters-heading"><span>CAST NOTES</span><h2>人物出场表</h2></div>
    <div class="character-grid">{"".join(
        f'''<article class="character-card"><div class="character-rank">{character.rank}</div><div class="character-copy"><h3>{html.escape(character.name)}</h3><strong>{html.escape(character.role_label)} · {html.escape(character.story_function)}</strong><p>{html.escape(character.arc_label or character.one_liner)}</p><blockquote>{html.escape(character.evidence)}</blockquote><small>{html.escape(character.callback_hint)}{(" · " + html.escape(character.relationship_hint)) if character.relationship_hint else ""}{(" · " + html.escape(character.memory_weight_label)) if character.memory_weight_label else ""}</small></div></article>'''
        for character in report.characters
    )}</div>
  </section>"""

    return f"""<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #ddd8ce; color: #111111; font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; }}
  .daily-paper {{ width: {width}px; margin: 18px auto; background: #fff8df; border: 9px solid #111111; }}
  .topline {{ min-height: 42px; display: flex; align-items: center; justify-content: space-between; padding: 0 24px; background: #111111; color: #ffd92e; font-size: 11px; font-weight: 800; }}
  .hero {{ position: relative; min-height: 196px; padding: 22px 154px 20px 24px; background: #ffd92e; border-bottom: 4px solid #111111; }}
  .hero-group {{ font-size: 25px; font-weight: 800; line-height: 1.25; }}
  .hero-title {{ margin-top: 7px; font-size: 42px; font-weight: 900; line-height: 1; }}
  .hero-mainline {{ margin-top: 14px; font-size: 18px; font-weight: 900; line-height: 1.45; }}
  .hero-opening {{ margin-top: 8px; color: #2b2b2b; font-size: 13px; font-weight: 700; line-height: 1.55; }}
  .hero-time {{ margin-top: 12px; padding-top: 6px; border-top: 4px solid #111111; font-size: 11px; }}
  .hero-badge {{ position: absolute; right: 24px; top: 24px; display: grid; width: 106px; height: 106px; place-items: center; border: 4px solid #111111; border-radius: 12px; background: #88d7ff; font-size: 21px; font-weight: 900; text-align: center; line-height: 1.1; }}
  .stats {{ display: grid; grid-template-columns: repeat(4, 1fr); background: #111111; color: #fff8df; }}
  .stat {{ min-height: 96px; padding: 17px 20px; border-right: 2px solid #3d3d3d; }}
  .stat:last-child {{ border-right: 0; }}
  .stat-label, .section-kicker, .highlight-kicker, .hotlist-heading span {{ color: #ffd92e; font-size: 11px; font-weight: 800; }}
  .stat-value {{ margin-top: 5px; font-size: 34px; font-weight: 900; line-height: 1; }}
  .stat-caption {{ margin-top: 5px; color: #c9c9c9; font-size: 11px; }}
  .timeline {{ margin: 0; padding: 26px 24px 18px; background: #ffd92e; border-bottom: 4px solid #111111; }}
  .timeline-head {{ display: flex; align-items: baseline; justify-content: space-between; }}
  .timeline h2, .section h2 {{ font-size: 26px; font-weight: 900; line-height: 1.1; }}
  .peak {{ font-size: 12px; font-weight: 700; }}
  .timeline svg {{ display: block; width: 100%; height: 106px; margin-top: 8px; }}
  .highlight {{ display: grid; grid-template-columns: 154px 1fr; gap: 18px; margin: 34px 24px 0; padding: 18px 20px; border: 4px solid #111111; background: #f25a18; color: #fff8df; }}
  .highlight-kicker {{ grid-column: 1; color: #ffd92e; }}
  .highlight-title {{ grid-column: 1; align-self: center; font-size: 25px; font-weight: 900; }}
  .highlight p {{ grid-column: 2; grid-row: 1 / span 2; align-self: center; font-size: 15px; font-weight: 700; line-height: 1.65; }}
  .hotlist {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #fff8df; }}
  .hotlist-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .hotlist-heading span {{ color: #f25a18; }}
  .hotlist-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .hotlist-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }}
  .hot-topic {{ display: grid; grid-template-columns: 25px 94px 1fr; align-items: center; gap: 7px; min-height: 48px; padding: 6px 8px; border: 2px solid #111111; background: #fff0a6; }}
  .hot-rank {{ display: grid; width: 23px; height: 23px; place-items: center; border: 2px solid #111111; border-radius: 50%; background: #ffd92e; font-size: 11px; font-weight: 900; }}
  .hot-topic strong {{ min-width: 0; font-size: 14px; }}
  .hot-topic small {{ color: #555555; font-size: 10px; line-height: 1.35; }}
  .relationships {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #88d7ff; }}
  .relationships-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .relationships-heading span {{ color: #111111; font-size: 11px; font-weight: 800; }}
  .relationships-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .relationship-list {{ display: grid; gap: 8px; }}
  .relationship-row {{ display: grid; grid-template-columns: 28px 1fr; align-items: center; min-height: 42px; border: 2px solid #111111; background: #ffffff; }}
  .relationship-row span {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; background: #ffd92e; font-size: 11px; font-weight: 900; }}
  .relationship-row p {{ padding: 8px 10px; font-size: 12px; font-weight: 700; line-height: 1.45; }}
  .characters {{ margin: 22px 24px 0; padding: 18px 16px 16px; border: 4px solid #111111; background: #ffffff; }}
  .characters-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 14px; }}
  .characters-heading span {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .characters-heading h2 {{ font-size: 21px; font-weight: 900; }}
  .character-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; }}
  .character-card {{ display: grid; grid-template-columns: 34px 1fr; min-height: 142px; border: 3px solid #111111; background: #fff8df; }}
  .character-rank {{ display: grid; place-items: center; border-right: 2px solid #111111; background: #88d7ff; font-size: 16px; font-weight: 900; }}
  .character-copy {{ min-width: 0; padding: 10px 12px; }}
  .character-copy h3 {{ font-size: 16px; font-weight: 900; line-height: 1.25; }}
  .character-copy strong {{ display: block; margin-top: 4px; color: #f25a18; font-size: 12px; }}
  .character-copy p {{ margin-top: 5px; color: #333333; font-size: 11px; line-height: 1.45; }}
  .character-copy blockquote {{ margin-top: 7px; padding-left: 8px; border-left: 3px solid #111111; font-size: 10px; line-height: 1.45; }}
  .character-copy small {{ display: block; margin-top: 6px; color: #555555; font-size: 10px; }}
  .section {{ margin: 34px 24px 0; }}
  .section-kicker {{ color: #f25a18; }}
  .section h2 {{ margin-top: 6px; }}
  .cases {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 18px; margin-top: 18px; }}
  .case-card {{ min-height: 214px; padding: 16px; border: 4px solid #111111; background: #ffffff; }}
  .case-head {{ display: flex; align-items: center; gap: 12px; }}
  .case-number {{ display: grid; width: 39px; height: 39px; place-items: center; border: 3px solid #111111; border-radius: 50%; background: #ffd92e; font-size: 12px; font-weight: 900; }}
  .case-time {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .case-card h3 {{ margin-top: 13px; font-size: 18px; line-height: 1.35; }}
  .case-summary {{ margin-top: 8px; color: #555555; font-size: 12px; line-height: 1.5; }}
  .case-notes {{ margin-top: 14px; padding: 10px 12px 8px 26px; background: #fff0a6; font-size: 11px; line-height: 1.55; }}
  .case-notes li + li {{ margin-top: 4px; }}
  .mvp-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; margin-top: 18px; }}
  .mvp-card {{ display: grid; grid-template-columns: 34px 1fr 48px; align-items: center; min-height: 70px; border: 3px solid #111111; background: #ffffff; }}
  .mvp-rank {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; font-size: 16px; font-weight: 900; }}
  .mvp-copy {{ padding: 8px 10px; }}
  .mvp-name {{ font-size: 15px; font-weight: 800; }}
  .mvp-meta {{ margin-top: 3px; color: #555555; font-size: 10px; }}
  .mvp-score {{ padding-right: 10px; text-align: right; font-size: 28px; font-weight: 900; }}
  .footer {{ margin-top: 34px; padding: 14px 24px; background: #111111; color: #ffd92e; font-size: 10px; }}
</style>
</head>
<body>
<main class="daily-paper">
  <div class="topline"><span>XIAOMAN CHARACTER DAILY</span><span>{html.escape(report.report_date)}</span></div>
  <header class="hero">
    <div class="hero-group">{html.escape(report.group_name)}</div>
    <div class="hero-title">小满群聊日报</div>
    <div class="hero-mainline">今日主线：{html.escape(main_storyline)}</div>
    <div class="hero-opening">{html.escape(opening_line)}</div>
    <div class="hero-time">{html.escape(report.time_range)} · {report.member_count} 名成员</div>
    <div class="hero-badge">人物<br>主线</div>
  </header>
  <section class="stats">{stats_html}</section>
  <section class="timeline">
    <div class="timeline-head"><h2>24H 活跃节奏</h2><div class="peak">峰值 {peak_count} 条 / {peak_idx:02d}:00</div></div>
    <svg viewBox="0 0 {chart_width} 106" aria-label="24小时活跃节奏">{timeline_svg}{peak_svg}{timeline_labels}</svg>
  </section>
  {characters_html}
  {highlight_html}
  {callbacks_html}
  {relationships_html}
  {cases_html}
  {mvp_html}
  <footer class="footer">本报告由小满根据最新群聊窗口自动整理 · 长期画像只以公开安全的角色复现计数参与</footer>
</main>
</body>
</html>"""


def _render_daily_markdown(report: ReportData) -> str:
    main_storyline = _main_storyline_label(report)
    callback_candidates = _meme_callback_candidates(report)
    relationship_candidates = _relationship_candidates(report)
    lines = [
        f"# 小满群聊日报｜{report.report_date}｜{main_storyline}",
        "",
        _daily_opening_line(report),
        "",
        f"- 日期：{report.report_date}",
        f"- 时间范围：{report.time_range}",
        f"- 消息：{report.message_count} 条",
        f"- 活跃：{report.participant_count} 人",
        f"- 可归档主线：{report.case_count} 条",
        f"- 今日剧中人：{report.character_count} 位",
        "",
    ]
    if report.highlight:
        lines.extend(["## 今日台词", "", f"> {report.highlight}", ""])
    if report.characters:
        lines.extend(["## 今日剧中人", ""])
        for character in report.characters:
            memory = f"（{character.memory_label}）" if character.memory_label else ""
            lines.extend(
                [
                    f"- **{character.name}（{character.role_label}）**："
                    f"{character.story_function}。{character.arc_label or character.one_liner}。"
                    f"{character.callback_hint}{memory}",
                    f"> {character.evidence}",
                    "",
                ]
            )
            if character.relationship_hint:
                lines.extend([f"  同场接力：{character.relationship_hint}", ""])
    if callback_candidates:
        lines.extend(["## 梗和回调候选", ""])
        lines.extend(f"- {candidate}" for candidate in callback_candidates)
        lines.append("")
    if relationship_candidates:
        lines.extend(["## 同场关系", ""])
        lines.extend(f"- {candidate}" for candidate in relationship_candidates)
        lines.append("")
    if report.cases:
        lines.extend(["## 今日主线", ""])
        for case in report.cases:
            lines.extend(
                [
                    f"### {case.case_no}｜{_case_storyline_label(case)}",
                    "",
                    f"- 时间：{case.time_label}",
                    f"- 规模：{case.summary}",
                    f"- 主讲：{case.top_speaker}",
                    "",
                ]
            )
            lines.extend(f"- {bullet}" for bullet in case.bullets[:3])
            lines.append("")
    if report.suspects:
        lines.extend(["## 发言出场榜", ""])
        for suspect in report.suspects:
            lines.append(
                f"- {suspect.rank}. {suspect.name}：{suspect.message_count} 条 / {suspect.word_count} 字"
            )
        lines.append("")
    universe = report.character_universe or {}
    if universe.get("storyline_candidates"):
        lines.extend(["## 可沉淀故事线", ""])
        for item in universe["storyline_candidates"][:5]:
            lines.append(f"- [[{item['label']}]]：{item['reason']}")
        lines.append("")
    if universe.get("creative_profile_candidates"):
        lines.extend(["## 可审核人物画像候选", ""])
        for item in universe["creative_profile_candidates"][:5]:
            lines.append(
                f"- {item['candidate_role_label']} / {item['story_function']}："
                f"{item['daily_arc']}（{item['profile_upgrade_status']}；"
                f"evidence_count={item['recurrence_evidence_count']}；{item['evidence_policy']}）"
            )
        lines.append("")
    lines.extend(
        [
            "## 公开边界",
            "",
            "- 本日报由小满根据最新群聊窗口自动整理。",
            "- 长期画像只以角色复现计数参与，不展示内部画像原文。",
            "- creative_profile_candidates 仅供内部审核，不写入长期画像表，不允许直接公开展示。",
            "- raw_messages_included=false；profile_fact_text_included=false。",
        ]
    )
    return "\n".join(lines)


def _file_url(path: Path) -> str:
    return path.resolve().as_uri()


def _font_candidates() -> list[str]:
    configured = os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_FONT")
    candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
    ]
    return ([configured] if configured else []) + candidates


def _pil_font(size: int, *, bold: bool = False) -> Any:
    from PIL import ImageFont

    candidates = _font_candidates()
    if bold:
        candidates = [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
            "/System/Library/Fonts/PingFang.ttc",
        ] + candidates
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return ImageFont.truetype(candidate, size=size)
    return ImageFont.load_default()


def _wrap_for_draw(draw: Any, text: str, font: Any, max_width: int) -> list[str]:
    words = list(text)
    lines: list[str] = []
    line = ""
    for word in words:
        candidate = line + word
        if line and draw.textlength(candidate, font=font) > max_width:
            lines.append(line)
            line = word
        else:
            line = candidate
    if line:
        lines.append(line)
    return lines or [""]


def _draw_wrapped_text(
    draw: Any,
    xy: tuple[int, int],
    text: str,
    font: Any,
    fill: str,
    max_width: int,
    *,
    line_gap: int = 8,
    max_lines: int | None = None,
) -> int:
    x, y = xy
    lines = _wrap_for_draw(draw, text, font, max_width)
    if max_lines is not None and len(lines) > max_lines:
        lines = lines[:max_lines]
        lines[-1] = lines[-1].rstrip("，。；、 ") + "..."
    line_height = int(font.size * 1.35) if hasattr(font, "size") else 24
    for line in lines:
        draw.text((x, y), line, font=font, fill=fill)
        y += line_height + line_gap
    return y


def _draw_card(
    draw: Any,
    box: tuple[int, int, int, int],
    fill: str,
    outline: str = "#d8c7a2",
) -> None:
    draw.rounded_rectangle(box, radius=18, fill=fill, outline=outline, width=2)


def _render_image_with_pillow(
    report: ReportData,
    output_path: Path,
    width: int,
    image_format: str,
) -> None:
    try:
        from PIL import Image, ImageDraw
    except ImportError as exc:
        raise RuntimeError(
            "image rendering requires playwright or Pillow; use --render html only for non-production debugging"
        ) from exc

    scale = 2
    canvas_width = width * scale
    outer = 22 * scale
    padding = 44 * scale
    gutter = 18 * scale
    content_width = canvas_width - padding * 2
    image = Image.new("RGB", (canvas_width, 7600), "#fff8df")
    draw = ImageDraw.Draw(image)

    title_font = _pil_font(44 * scale, bold=True)
    hero_font = _pil_font(26 * scale, bold=True)
    section_font = _pil_font(24 * scale, bold=True)
    card_title_font = _pil_font(18 * scale, bold=True)
    stat_font = _pil_font(31 * scale, bold=True)
    body_font = _pil_font(16 * scale)
    small_font = _pil_font(12 * scale)
    tiny_font = _pil_font(10 * scale)
    mono_font = _pil_font(15 * scale, bold=True)

    ink = "#111111"
    yellow = "#ffd92e"
    orange = "#f25a18"
    cream = "#fff8df"
    pale_yellow = "#fff0a6"
    blue = "#88d7ff"
    main_storyline = _main_storyline_label(report)
    opening_line = _daily_opening_line(report)
    callback_candidates = _meme_callback_candidates(report)
    relationship_candidates = _relationship_candidates(report)

    def text_right(x: int, y: int, text: str, font: Any, fill: str) -> None:
        box = draw.textbbox((0, 0), text, font=font)
        draw.text((x - (box[2] - box[0]), y), text, font=font, fill=fill)

    def section_label(y_pos: int, kicker: str, title: str) -> int:
        draw.text((padding, y_pos), kicker, font=tiny_font, fill=orange)
        y_pos += 20 * scale
        draw.text((padding, y_pos), title, font=section_font, fill=ink)
        return y_pos + 42 * scale

    y = outer
    draw.rectangle((outer, y, canvas_width - outer, y + 42 * scale), fill=ink)
    draw.text((padding, y + 12 * scale), "XIAOMAN CHARACTER DAILY", font=tiny_font, fill=yellow)
    text_right(canvas_width - padding, y + 12 * scale, report.report_date, tiny_font, yellow)
    y += 42 * scale

    hero_top = y
    hero_height = 206 * scale
    draw.rectangle((outer, hero_top, canvas_width - outer, hero_top + hero_height), fill=yellow, outline=ink, width=3 * scale)
    draw.text((padding, hero_top + 20 * scale), report.group_name, font=hero_font, fill=ink)
    draw.text((padding, hero_top + 60 * scale), "小满群聊日报", font=title_font, fill=ink)
    _draw_wrapped_text(
        draw,
        (padding, hero_top + 112 * scale),
        f"今日主线：{main_storyline}",
        body_font,
        ink,
        content_width - 132 * scale,
        max_lines=1,
    )
    _draw_wrapped_text(
        draw,
        (padding, hero_top + 142 * scale),
        opening_line,
        small_font,
        "#2b2b2b",
        content_width - 132 * scale,
        max_lines=2,
        line_gap=2 * scale,
    )
    draw.rectangle((padding, hero_top + 184 * scale, canvas_width - padding - 126 * scale, hero_top + 188 * scale), fill=ink)
    draw.text((padding, hero_top + 192 * scale), report.time_range, font=tiny_font, fill=ink)
    badge_box = (canvas_width - padding - 106 * scale, hero_top + 24 * scale, canvas_width - padding, hero_top + 130 * scale)
    draw.rounded_rectangle(badge_box, radius=12 * scale, fill=blue, outline=ink, width=3 * scale)
    draw.text((badge_box[0] + 22 * scale, badge_box[1] + 30 * scale), "人物", font=hero_font, fill=ink)
    draw.text((badge_box[0] + 22 * scale, badge_box[1] + 66 * scale), "主线", font=hero_font, fill=ink)
    y = hero_top + hero_height

    stats = [
        ("消息", report.message_count, "当日素材"),
        ("出场", report.participant_count, "活跃成员"),
        ("主线", report.case_count, "可归档"),
        ("人物", report.character_count, "群像卡"),
    ]
    stat_height = 96 * scale
    stat_width = (canvas_width - outer * 2) // 4
    draw.rectangle((outer, y, canvas_width - outer, y + stat_height), fill=ink)
    for index, (label, value, caption) in enumerate(stats):
        x = outer + index * stat_width
        if index:
            draw.line((x, y + 12 * scale, x, y + stat_height - 12 * scale), fill="#3a3a3a", width=2 * scale)
        draw.text((x + 22 * scale, y + 16 * scale), label, font=tiny_font, fill=yellow)
        draw.text((x + 22 * scale, y + 38 * scale), str(value), font=stat_font, fill=cream)
        draw.text((x + 88 * scale, y + 56 * scale), caption, font=tiny_font, fill="#d8d8d8")
    y += stat_height

    timeline_top = y
    timeline_height = 145 * scale
    draw.rectangle((outer, timeline_top, canvas_width - outer, timeline_top + timeline_height), fill=yellow, outline=ink, width=3 * scale)
    draw.text((padding, timeline_top + 28 * scale), "24H 活跃节奏", font=section_font, fill=ink)
    max_count = max(report.hourly_counts or [0]) or 1
    peak_idx = report.hourly_counts.index(max_count) if report.hourly_counts else 0
    text_right(canvas_width - padding, timeline_top + 34 * scale, f"峰值 {max_count} 条 / {peak_idx:02d}:00", small_font, ink)
    bar_left = padding + 200 * scale
    bar_width_area = canvas_width - padding - bar_left
    bar_gap = 5 * scale
    bar_width = max(4 * scale, (bar_width_area - bar_gap * 23) // 24)
    base_y = timeline_top + 112 * scale
    for index, count in enumerate(report.hourly_counts[:24]):
        height = int((count / max_count) * 72 * scale)
        x = bar_left + index * (bar_width + bar_gap)
        fill = orange if index == peak_idx and count else ink if count else "#e6c737"
        draw.rectangle((x, base_y - height, x + bar_width, base_y), fill=fill)
    y = timeline_top + timeline_height + 34 * scale

    if report.highlight:
        highlight_top = y
        highlight_height = 138 * scale
        draw.rectangle((outer, highlight_top, canvas_width - outer, highlight_top + highlight_height), fill=orange, outline=ink, width=3 * scale)
        draw.text((padding, highlight_top + 22 * scale), "QUOTE ANCHOR", font=tiny_font, fill=yellow)
        draw.text((padding, highlight_top + 46 * scale), "今日台词", font=section_font, fill=cream)
        _draw_wrapped_text(
            draw,
            (padding + 180 * scale, highlight_top + 30 * scale),
            f"“{report.highlight}”",
            body_font,
            cream,
            content_width - 180 * scale,
            max_lines=3,
        )
        y += highlight_height + 38 * scale

    if callback_candidates:
        hotlist_top = y
        hotlist_rows = (len(callback_candidates) + 1) // 2
        hotlist_height = (54 + 50 * hotlist_rows) * scale
        draw.rectangle((outer, hotlist_top, canvas_width - outer, hotlist_top + hotlist_height), fill=cream, outline=ink, width=3 * scale)
        draw.text((padding, hotlist_top + 14 * scale), "MEME MAP", font=tiny_font, fill=orange)
        draw.text((padding + 102 * scale, hotlist_top + 10 * scale), "梗和回调候选", font=body_font, fill=ink)
        topic_width = (content_width - gutter) // 2
        for index, candidate in enumerate(callback_candidates):
            label, _, detail = candidate.partition("：")
            column = index % 2
            row = index // 2
            x = padding + column * (topic_width + gutter)
            row_top = hotlist_top + (44 + row * 46) * scale
            draw.rectangle((x, row_top, x + topic_width, row_top + 34 * scale), fill=pale_yellow, outline=ink, width=2 * scale)
            draw.ellipse((x + 8 * scale, row_top + 7 * scale, x + 30 * scale, row_top + 29 * scale), fill=yellow, outline=ink, width=2 * scale)
            draw.text((x + 14 * scale, row_top + 10 * scale), str(index + 1), font=tiny_font, fill=ink)
            draw.text((x + 40 * scale, row_top + 8 * scale), label[:8], font=small_font, fill=ink)
            _draw_wrapped_text(
                draw,
                (x + 120 * scale, row_top + 8 * scale),
                detail,
                tiny_font,
                "#555555",
                topic_width - 130 * scale,
                max_lines=1,
                line_gap=0,
            )
        y = hotlist_top + hotlist_height + 38 * scale

    if relationship_candidates:
        relationship_top = y
        relationship_height = (54 + 42 * len(relationship_candidates)) * scale
        draw.rectangle((outer, relationship_top, canvas_width - outer, relationship_top + relationship_height), fill=blue, outline=ink, width=3 * scale)
        draw.text((padding, relationship_top + 14 * scale), "ENSEMBLE LINKS", font=tiny_font, fill=ink)
        draw.text((padding + 132 * scale, relationship_top + 10 * scale), "同场关系", font=body_font, fill=ink)
        row_width = content_width
        for index, candidate in enumerate(relationship_candidates):
            row_top = relationship_top + (44 + index * 38) * scale
            draw.rectangle((padding, row_top, padding + row_width, row_top + 30 * scale), fill="#ffffff", outline=ink, width=2 * scale)
            draw.rectangle((padding, row_top, padding + 28 * scale, row_top + 30 * scale), fill=yellow, outline=ink, width=2 * scale)
            draw.text((padding + 10 * scale, row_top + 8 * scale), str(index + 1), font=tiny_font, fill=ink)
            _draw_wrapped_text(
                draw,
                (padding + 40 * scale, row_top + 7 * scale),
                candidate,
                tiny_font,
                ink,
                row_width - 52 * scale,
                max_lines=1,
                line_gap=0,
            )
        y = relationship_top + relationship_height + 38 * scale

    if report.characters:
        character_top = y
        character_rows = (len(report.characters) + 1) // 2
        character_height = (58 + 138 * character_rows) * scale
        draw.rectangle((outer, character_top, canvas_width - outer, character_top + character_height), fill="#ffffff", outline=ink, width=3 * scale)
        draw.text((padding, character_top + 16 * scale), "CAST NOTES", font=tiny_font, fill=orange)
        draw.text((padding + 112 * scale, character_top + 12 * scale), "人物出场表", font=body_font, fill=ink)
        card_width = (content_width - gutter) // 2
        card_height = 116 * scale
        for index, character in enumerate(report.characters):
            column = index % 2
            row = index // 2
            x = padding + column * (card_width + gutter)
            card_y = character_top + (48 + row * 128) * scale
            draw.rectangle((x, card_y, x + card_width, card_y + card_height), fill=cream, outline=ink, width=2 * scale)
            draw.rectangle((x, card_y, x + 34 * scale, card_y + card_height), fill=blue, outline=ink, width=2 * scale)
            draw.text((x + 12 * scale, card_y + 42 * scale), str(character.rank), font=tiny_font, fill=ink)
            copy_x = x + 46 * scale
            draw.text((copy_x, card_y + 10 * scale), character.name, font=body_font, fill=ink)
            draw.text((copy_x, card_y + 34 * scale), f"{character.role_label} · {character.story_function}", font=tiny_font, fill=orange)
            _draw_wrapped_text(
                draw,
                (copy_x, card_y + 54 * scale),
                f"{character.arc_label or character.one_liner}｜{character.evidence}",
                tiny_font,
                ink,
                card_width - 58 * scale,
                max_lines=3,
                line_gap=2 * scale,
            )
            text_right(
                x + card_width - 10 * scale,
                card_y + 92 * scale,
                f"{character.message_count} 条 · {character.topic_count} 触点",
                tiny_font,
                "#555555",
            )
            if character.memory_label:
                _draw_wrapped_text(
                    draw,
                    (copy_x, card_y + 92 * scale),
                    character.memory_label,
                    tiny_font,
                    "#555555",
                    card_width - 104 * scale,
                    max_lines=1,
                    line_gap=0,
                )
        y = character_top + character_height + 38 * scale

    y = section_label(y, "STORYLINE FILES", "故事线候选")
    cases = report.cases[:DEFAULT_CASE_LIMIT]
    if not cases:
        draw.rectangle((padding, y, padding + content_width, y + 92 * scale), fill="#ffffff", outline=ink, width=3 * scale)
        draw.text((padding + 22 * scale, y + 32 * scale), "过去 24 小时暂无可归档话题。", font=body_font, fill="#555555")
        y += 116 * scale
    else:
        card_width = (content_width - gutter) // 2
        card_height = 226 * scale
        for index, case in enumerate(cases):
            col = index % 2
            row = index // 2
            x = padding + col * (card_width + gutter)
            card_y = y + row * (card_height + gutter)
            draw.rectangle((x, card_y, x + card_width, card_y + card_height), fill="#ffffff", outline=ink, width=3 * scale)
            draw.ellipse((x + 18 * scale, card_y + 18 * scale, x + 58 * scale, card_y + 58 * scale), fill=yellow, outline=ink, width=3 * scale)
            draw.text((x + 28 * scale, card_y + 26 * scale), f"{index + 1:02d}", font=tiny_font, fill=ink)
            draw.text((x + 72 * scale, card_y + 22 * scale), case.time_label, font=tiny_font, fill=orange)
            title_y = _draw_wrapped_text(
                draw,
                (x + 18 * scale, card_y + 72 * scale),
                _case_storyline_label(case),
                card_title_font,
                ink,
                card_width - 36 * scale,
                max_lines=2,
                line_gap=4 * scale,
            )
            _draw_wrapped_text(
                draw,
                (x + 18 * scale, title_y + 4 * scale),
                case.summary,
                small_font,
                "#444444",
                card_width - 36 * scale,
                max_lines=1,
            )
            note_top = card_y + 150 * scale
            draw.rectangle((x + 18 * scale, note_top, x + card_width - 18 * scale, card_y + card_height - 18 * scale), fill=pale_yellow)
            bullet_text = " / ".join(case.bullets[:2])
            _draw_wrapped_text(
                draw,
                (x + 30 * scale, note_top + 12 * scale),
                bullet_text,
                tiny_font,
                ink,
                card_width - 60 * scale,
                max_lines=3,
                line_gap=3 * scale,
            )
        y += ((len(cases) + 1) // 2) * (card_height + gutter) + 18 * scale

    y = section_label(y, "VOICE INDEX", "发言出场榜")
    if not report.suspects:
        draw.text((padding, y), "暂无发言榜。", font=body_font, fill="#555555")
        y += 54 * scale
    else:
        row_height = 72 * scale
        for index, suspect in enumerate(report.suspects[:DEFAULT_SUSPECT_LIMIT]):
            x = padding + (index % 2) * ((content_width - gutter) // 2 + gutter)
            row_y = y + (index // 2) * (row_height + 10 * scale)
            row_width = (content_width - gutter) // 2
            draw.rectangle((x, row_y, x + row_width, row_y + row_height), fill="#ffffff", outline=ink, width=2 * scale)
            draw.text((x + 16 * scale, row_y + 20 * scale), str(suspect.rank), font=mono_font, fill=ink)
            draw.text((x + 58 * scale, row_y + 14 * scale), suspect.name, font=body_font, fill=ink)
            draw.text((x + 58 * scale, row_y + 42 * scale), f"{suspect.message_count} 条 / {suspect.word_count} 字", font=tiny_font, fill="#555555")
            text_right(x + row_width - 16 * scale, row_y + 21 * scale, f"{suspect.message_count}", stat_font, ink)
        y += ((min(len(report.suspects), DEFAULT_SUSPECT_LIMIT) + 1) // 2) * (row_height + 10 * scale) + 28 * scale

    draw.rectangle((outer, y, canvas_width - outer, y + 42 * scale), fill=ink)
    draw.text((padding, y + 14 * scale), "本报告由小满根据最新群聊窗口自动整理 · 长期画像仅以角色复现计数参与", font=tiny_font, fill=yellow)
    y += 42 * scale + outer

    cropped = image.crop((0, 0, canvas_width, min(y, image.height)))
    save_kwargs: dict[str, Any] = {}
    if image_format == "jpeg":
        save_kwargs["quality"] = DEFAULT_JPEG_QUALITY
        save_kwargs["optimize"] = True
        save_format = "JPEG"
    else:
        save_format = "PNG"
    cropped.save(output_path, format=save_format, **save_kwargs)


def _render_image(
    html_path: Path,
    output_path: Path,
    width: int,
    image_format: str,
    report: ReportData | None = None,
) -> None:
    try:
        _render_image_with_playwright(html_path, output_path, width, image_format)
    except Exception as exc:
        if report is not None:
            _render_image_with_pillow(report, output_path, width, image_format)
            return
        raise RuntimeError(
            "image rendering requires playwright or a report object for Pillow fallback"
        ) from exc


def _render_image_with_playwright(
    html_path: Path,
    output_path: Path,
    width: int,
    image_format: str,
) -> None:
    from playwright.sync_api import sync_playwright

    screenshot_options: dict[str, Any] = {
        "path": str(output_path),
        "full_page": True,
        "type": image_format,
    }
    if image_format == "jpeg":
        screenshot_options["quality"] = DEFAULT_JPEG_QUALITY

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
        page.screenshot(**screenshot_options)
        browser.close()


def _render_png(html_path: Path, output_path: Path, width: int) -> None:
    _render_image(html_path, output_path, width, "png")


def _image_mime_type(image_format: str) -> str:
    return "image/jpeg" if image_format == "jpeg" else "image/png"


def _image_extension(image_format: str) -> str:
    return "jpg" if image_format == "jpeg" else "png"


def _source_chat_ref(chat_id: str | None) -> dict[str, str] | None:
    if not chat_id:
        return None
    digest = hashlib.sha256(chat_id.encode("utf-8")).hexdigest()
    return {"kind": "sha256", "value": f"sha256:{digest}"}


def _artifact_candidate(
    path: Path,
    image_format: str,
    report: ReportData,
    output_width: int | None = None,
    source_chat_id: str | None = None,
) -> dict[str, Any]:
    data = path.read_bytes()
    return {
        "artifact_type": "generated_image",
        "workflow_type": "daily_case_report",
        "template_version": TEMPLATE_VERSION,
        "mime_type": _image_mime_type(image_format),
        "filename": path.name,
        "content_hash": f"sha256:{hashlib.sha256(data).hexdigest()}",
        "file_md5": hashlib.md5(data).hexdigest(),  # nosec: QiWe protocol requires MD5.
        "byte_size": len(data),
        "render": {
            "image_format": image_format,
            "width": output_width,
            "jpeg_quality": DEFAULT_JPEG_QUALITY if image_format == "jpeg" else None,
        },
        "report_window": {
            "start": report.window_start,
            "end": report.window_end,
            "display": report.report_date,
            "time_range": report.time_range,
            "timezone": report.timezone,
        },
        "content_metrics": {
            "message_count": report.message_count,
            "participant_count": report.participant_count,
            "case_count": report.case_count,
            "character_count": report.character_count,
            "hot_topic_count": len(report.hot_topics),
        },
        "source_chat_ref": _source_chat_ref(source_chat_id),
        "retained_source_policy": "sanitized_metadata_only",
    }


def _operator_review_message(
    report: ReportData,
    html_path: Path,
    image_path: Path | None,
    include_html: bool = False,
) -> str:
    lines = [
        f"【{report.group_name}｜小满群聊日报】",
        f"日报日期：{report.report_date}（{report.time_range}）",
        f"消息 {report.message_count} 条 / 活跃 {report.participant_count} 人 / 主线 {report.case_count} 条 / 剧中人 {report.character_count} 位 / 发言榜 {report.suspect_count} 名",
        f"今日主线：{_main_storyline_label(report)}",
        "",
    ]
    for case in report.cases:
        lines.append(f"• 主线 {case.case_no.replace('CASE ', '')}：{_case_storyline_label(case)}（{case.summary}）")
    for character in report.characters:
        lines.append(f"• 剧中人 {character.rank}：{character.name}｜{character.role_label}｜{character.story_function}")
    lines.append("")
    if image_path:
        lines.append(f"图片文件：{image_path}")
    if include_html and html_path.exists():
        label = "HTML 预览（仅调试用）" if image_path else "HTML 预览"
        lines.append(f"{label}：{html_path}")
    lines.append("")
    lines.append("本报告仅生成本地文件，尚未自动发布；生产自动发布需接入 AgentOS artifact 与 QiWe image-send。")
    return "\n".join(lines)


def _result_json(
    report: ReportData,
    deliverable_path: Path,
    image_path: Path | None,
    image_format: str | None = None,
    html_path: Path | None = None,
    markdown_path: Path | None = None,
    universe_path: Path | None = None,
    quote_map_path: Path | None = None,
    wiki_bundle_path: Path | None = None,
    draft_bundle_path: Path | None = None,
    run_manifest_path: Path | None = None,
    review_report_path: Path | None = None,
    creative_profile_review_payload_path: Path | None = None,
    quote_map: dict[str, Any] | None = None,
    wiki_bundle: dict[str, Any] | None = None,
    draft_bundle: dict[str, Any] | None = None,
    run_manifest: dict[str, Any] | None = None,
    creative_profile_review_payload: dict[str, Any] | None = None,
    output_width: int | None = None,
    source_chat_id: str | None = None,
) -> dict[str, Any]:
    html_exists = html_path is not None and html_path.exists()
    markdown_exists = markdown_path is not None and markdown_path.exists()
    universe_exists = universe_path is not None and universe_path.exists()
    quote_map_exists = quote_map_path is not None and quote_map_path.exists()
    wiki_bundle_exists = wiki_bundle_path is not None and wiki_bundle_path.exists()
    draft_bundle_exists = draft_bundle_path is not None and draft_bundle_path.exists()
    run_manifest_exists = run_manifest_path is not None and run_manifest_path.exists()
    review_report_exists = review_report_path is not None and review_report_path.exists()
    creative_profile_review_payload_exists = (
        creative_profile_review_payload_path is not None
        and creative_profile_review_payload_path.exists()
    )
    quote_map = quote_map or _build_quote_map(report)
    wiki_bundle = wiki_bundle or _build_wiki_bundle(report, quote_map)
    draft_bundle = draft_bundle or _build_draft_bundle(report, quote_map, wiki_bundle)
    run_manifest = run_manifest or _build_run_manifest(
        report,
        quote_map,
        wiki_bundle,
        draft_bundle,
        source_chat_id=source_chat_id,
    )
    creative_profile_review_payload = creative_profile_review_payload or {}
    artifact_candidate = (
        _artifact_candidate(image_path, image_format, report, output_width, source_chat_id)
        if image_path is not None and image_format is not None and image_path.exists()
        else None
    )
    return {
        "success": True,
        "skill": "xiaoman_daily_case_report",
        "external_send_executed": False,
        "requires_human_confirmation": False,
        "auto_publish_ready": False,
        "group_name": report.group_name,
        "report_date": report.report_date,
        "time_range": report.time_range,
        "message_count": report.message_count,
        "participant_count": report.participant_count,
        "case_count": report.case_count,
        "character_count": report.character_count,
        "suspect_count": report.suspect_count,
        "deliverable_path": str(deliverable_path),
        "image_path": str(image_path) if image_path else None,
        "image_format": image_format,
        "image_mime_type": _image_mime_type(image_format) if image_format else None,
        "png_path": str(image_path) if image_format == "png" and image_path else None,
        "html_path": str(html_path) if html_exists else None,
        "daily_report_markdown_path": str(markdown_path) if markdown_exists else None,
        "daily_report_markdown": _render_daily_markdown(report),
        "character_universe_path": str(universe_path) if universe_exists else None,
        "character_universe": report.character_universe,
        "quote_map_path": str(quote_map_path) if quote_map_exists else None,
        "quote_map": quote_map,
        "wiki_bundle_path": str(wiki_bundle_path) if wiki_bundle_exists else None,
        "wiki_bundle": wiki_bundle,
        "draft_bundle_path": str(draft_bundle_path) if draft_bundle_exists else None,
        "draft_bundle": draft_bundle,
        "run_manifest_path": str(run_manifest_path) if run_manifest_exists else None,
        "run_manifest": run_manifest,
        "review_report_path": str(review_report_path) if review_report_exists else None,
        "creative_profile_review_payload_path": (
            str(creative_profile_review_payload_path)
            if creative_profile_review_payload_exists
            else None
        ),
        "private_review_bundle": {
            "schema_version": "xiaoman-daily-private-review-bundle-v1",
            "source": "wx_cli_style_daily_migration",
            "public_surface_allowed": False,
            "review_required": True,
            "raw_message_rows_included": False,
            "profile_fact_text_included": False,
            "quote_map_entry_count": quote_map.get("entry_count", 0),
            "wiki_counts": wiki_bundle.get("counts", {}),
            "draft_counts": draft_bundle.get("counts", {}),
            "run_manifest_schema_version": run_manifest.get("schema_version", ""),
            "creative_profile_review_payload": {
                "schema_version": creative_profile_review_payload.get("schema_version"),
                "source": creative_profile_review_payload.get("source", ""),
                "candidate_count": len(creative_profile_review_payload.get("candidates") or []),
                "pending_review_count": sum(
                    1
                    for candidate in creative_profile_review_payload.get("candidates") or []
                    if candidate.get("review_decision") == "pending_review"
                ),
                "approved_candidate_count": sum(
                    1
                    for candidate in creative_profile_review_payload.get("candidates") or []
                    if candidate.get("review_decision") == "approved"
                ),
                "person_id_required": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("person_id_required")
                is True,
                "display_name_binding_allowed": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("display_name_binding_allowed")
                is True,
                "public_surface_allowed": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("public_surface_allowed")
                is True,
                "raw_messages_included": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("raw_messages_included")
                is True,
                "profile_fact_text_included": (
                    creative_profile_review_payload.get("review_notes") or {}
                ).get("profile_fact_text_included")
                is True,
            },
        },
        "artifact_candidate": artifact_candidate,
        "operator_review_message": _operator_review_message(
            report, html_path or deliverable_path, image_path, html_exists
        ),
    }


def main() -> int:
    args = _parse_args()
    _normalize_render_args(args)

    real_messages = _uses_real_messages(args)
    if real_messages and (args.keep_html or args.render == "html"):
        print(
            "ERROR: production read-through cannot retain HTML because it contains real group content; "
            "use --render image without --keep-html",
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
    markdown_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.md"
    universe_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.character-universe.json"
    quote_map_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.quote-map.json"
    wiki_bundle_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.wiki-bundle.json"
    draft_bundle_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.draft-bundle.json"
    run_manifest_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.run-manifest.json"
    review_report_path = output_dir / f"xiaoman-daily-case-report-{timestamp}.review.md"
    creative_profile_review_payload_path = (
        output_dir
        / f"xiaoman-daily-case-report-{timestamp}.creative-profile-review-payload.draft.json"
    )
    image_path = output_dir / (
        f"xiaoman-daily-case-report-{timestamp}.{_image_extension(args.image_format)}"
    )

    html_content = _render_html(report, args.output_width)
    quote_map = _build_quote_map(report)
    wiki_bundle = _build_wiki_bundle(report, quote_map)
    draft_bundle = _build_draft_bundle(report, quote_map, wiki_bundle)
    run_manifest = _build_run_manifest(
        report,
        quote_map,
        wiki_bundle,
        draft_bundle,
        source_chat_id=args.chat_id,
    )
    creative_profile_review_payload = _build_creative_profile_review_payload_draft(
        report.character_universe,
        datetime.now(_report_timezone(getattr(args, "timezone", DEFAULT_TIMEZONE)))
        .replace(microsecond=0)
        .isoformat(),
    )
    _write_private_text(html_path, html_content)
    _write_private_text(markdown_path, _render_daily_markdown(report))
    _write_private_text(
        universe_path,
        json.dumps(report.character_universe, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        quote_map_path,
        json.dumps(quote_map, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        wiki_bundle_path,
        json.dumps(wiki_bundle, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        draft_bundle_path,
        json.dumps(draft_bundle, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        run_manifest_path,
        json.dumps(run_manifest, ensure_ascii=False, indent=2),
    )
    _write_private_text(
        review_report_path,
        _render_review_report(report, quote_map, wiki_bundle, draft_bundle, run_manifest),
    )
    _write_private_text(
        creative_profile_review_payload_path,
        json.dumps(creative_profile_review_payload, ensure_ascii=False, indent=2, sort_keys=True),
    )

    image_generated = False
    try:
        if args.render in ("auto", "image", "png"):
            try:
                _render_image(
                    html_path,
                    image_path,
                    args.output_width,
                    args.image_format,
                    report,
                )
                image_generated = True
            except RuntimeError as exc:
                print(f"WARN: image rendering skipped: {exc}", file=sys.stderr)
                if args.render in ("image", "png") or real_messages:
                    return 2

        html_is_deliverable = not image_generated

        deliverable = image_path if image_generated else html_path
        result = _result_json(
            report,
            deliverable,
            image_path if image_generated else None,
            args.image_format if image_generated else None,
            None if real_messages else html_path if html_path.exists() else None,
            markdown_path if markdown_path.exists() else None,
            universe_path if universe_path.exists() else None,
            quote_map_path if quote_map_path.exists() else None,
            wiki_bundle_path if wiki_bundle_path.exists() else None,
            draft_bundle_path if draft_bundle_path.exists() else None,
            run_manifest_path if run_manifest_path.exists() else None,
            review_report_path if review_report_path.exists() else None,
            creative_profile_review_payload_path
            if creative_profile_review_payload_path.exists()
            else None,
            quote_map,
            wiki_bundle,
            draft_bundle,
            run_manifest,
            creative_profile_review_payload,
            args.output_width if image_generated else None,
            args.chat_id if image_generated else None,
        )

        if args.json:
            print(json.dumps(result, ensure_ascii=False, indent=2))
        else:
            print(result["operator_review_message"])
        return 0
    finally:
        html_is_deliverable = not image_generated
        should_remove_html = real_messages or (not args.keep_html and not html_is_deliverable)
        if should_remove_html and html_path.exists():
            try:
                html_path.unlink()
            except OSError:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
