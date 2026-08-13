#!/usr/bin/env python3
"""Xiaoman wx-cli style daily report generator."""
from __future__ import annotations

import argparse
import html
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from datetime import datetime, time, timedelta
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, unquote, urlparse
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError


DEFAULT_GROUP_NAME = "秦托邦的小伙伴（新）"
DEFAULT_REPORT_TITLE = "秦托邦日报"
CHAT_ID_ENV = "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
DEFAULT_TIMEZONE = "Asia/Shanghai"
DEFAULT_OUTPUT_WIDTH = 750
DEFAULT_STORYLINE_LIMIT = 7
DEFAULT_CHARACTER_LIMIT = 6
DEFAULT_QUOTE_LIMIT = 6
DEFAULT_HOURLY_BUCKETS = 24
DEFAULT_WINDOW_HOURS = 24
DEFAULT_MIN_STORYLINE_MESSAGES = 2
DEFAULT_TOP_KEYWORDS = 24
UUID_RE = re.compile(
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)

STOP_WORDS: set[str] = {
    "这个", "那个", "然后", "就是", "什么", "怎么", "还是", "可以", "今天",
    "明天", "现在", "已经", "没有", "但是", "因为", "所以", "一下", "大家",
    "我们", "你们", "他们", "自己", "这里", "那里", "这样", "那样", "一个",
    "不是", "不用", "不要", "应该", "可能", "需要", "觉得", "看看", "一直",
    "时候", "过来", "过去", "为了", "作为", "关于", "或者", "以及", "并且",
    "虽然", "不过", "只是", "而且", "哈哈", "嘿嘿", "嗯嗯", "好的", "收到",
    "谢谢", "请问", "知道", "真的", "里面", "出来", "起来", "起来了",
}

TOPIC_MARKERS = {
    "活动": ("活动接龙", "从一句报名开始，群聊自动切进执行模式。"),
    "接龙": ("接龙现场", "秦托邦的组织能力，经常藏在一串加一里。"),
    "报名": ("报名窗口", "有人发起，有人犹豫，有人已经在路上。"),
    "打卡": ("共学打卡", "今天的自律额度，被几条消息悄悄续上。"),
    "学习": ("知识分享", "群里又出现了把日常变教材的人。"),
    "分享": ("资料投喂", "一条链接背后，通常是一整条知识补给线。"),
    "求助": ("现场求助", "问题一抛出来，社区协作就开始转动。"),
    "报错": ("技术急诊", "报错不是事故，是群友登场的提示音。"),
    "吃": ("饭局雷达", "只要出现食物，秦托邦行动力会明显提升。"),
    "喝": ("酒水支线", "气氛到位以后，理性通常坐到副驾驶。"),
    "猫": ("社区观察", "本地生活线索又一次从群聊缝隙里冒头。"),
    "雨": ("天气系统", "天气从来不只是天气，它负责给故事加旁白。"),
    "跑": ("运动支线", "嘴上说随便动动，身体已经开始写连续剧。"),
}


@dataclass
class ReportMessage:
    id: str
    sender_id: str
    sender_name: str
    text: str
    sent_at: datetime | None
    message_kind: str
    sender_person_id: str | None = None


@dataclass
class CreativeMemorySignal:
    person_id: str
    label: str
    fact_type: str
    count: int
    last_seen: str
    public_safe: bool


@dataclass
class CaseCard:
    case_no: str
    title: str
    time_label: str
    summary: str
    bullets: list[str]
    message_count: int
    participant_count: int
    color_bg: str = "#f7efe2"
    color_text: str = "#1f1b18"
    top_speaker: str = "群友"
    chapter_title: str = ""
    narrative: str = ""
    callback_hint: str = ""


@dataclass
class CharacterSketch:
    rank: int
    identity_key: str
    name: str
    message_count: int
    word_count: int
    role_line: str
    today_evidence: list[str]
    memory_line: str
    private_memory_count: int


@dataclass
class Suspect:
    rank: int
    name: str
    message_count: int
    word_count: int
    avatar_emoji: str = ""


@dataclass
class QuoteLine:
    speaker: str
    text: str
    time_label: str


@dataclass
class DraftBundle:
    digest_markdown: str
    roast_markdown: str
    public_draft_markdown: str
    quote_map: list[dict[str, str]]
    profile_candidates: list[dict[str, Any]]
    privacy_flags: dict[str, bool]
    draft_counts: dict[str, int]


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
    headline: str = ""
    subtitle: str = ""
    opening: str = ""
    characters: list[CharacterSketch] = field(default_factory=list)
    quotes: list[QuoteLine] = field(default_factory=list)
    tomorrow_clues: list[str] = field(default_factory=list)
    draft_bundle: DraftBundle | None = None


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Xiaoman wx-cli style daily report")
    parser.add_argument("--date", help="Backfill one calendar day (YYYY-MM-DD).")
    parser.add_argument("--chat-id", default=os.environ.get(CHAT_ID_ENV))
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
        help="png is the group-facing image; html is a debug preview only.",
    )
    parser.add_argument("--keep-html", action="store_true")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def _report_timezone(timezone_name: str) -> ZoneInfo:
    try:
        return ZoneInfo(timezone_name)
    except ZoneInfoNotFoundError as exc:
        raise RuntimeError(f"unsupported daily report timezone: {timezone_name}") from exc


def _report_date_at(args: argparse.Namespace, now: datetime) -> tuple[datetime, datetime, str]:
    report_zone = _report_timezone(args.timezone)
    if args.date:
        base_date = datetime.strptime(args.date, "%Y-%m-%d").date()
        start = datetime.combine(base_date, time.min, tzinfo=report_zone)
        end = start + timedelta(days=1)
        return start, end, start.strftime("%Y年%m月%d日")

    local_now = now.astimezone(report_zone) if now.tzinfo else now.replace(tzinfo=report_zone)
    end = local_now.replace(microsecond=0)
    start = end - timedelta(hours=DEFAULT_WINDOW_HOURS)
    display = f"过去{DEFAULT_WINDOW_HOURS}小时（截至 {end.strftime('%Y年%m月%d日 %H:%M')}）"
    return start, end, display


def _report_date(args: argparse.Namespace) -> tuple[datetime, datetime, str]:
    report_zone = _report_timezone(args.timezone)
    return _report_date_at(args, datetime.now(report_zone))


def _time_range_label(start: datetime, end: datetime) -> str:
    end_display = end - timedelta(seconds=1)
    if start.date() == end_display.date():
        return f"{start.strftime('%H:%M')}–{end_display.strftime('%H:%M')}"
    return f"{start.strftime('%m/%d %H:%M')}–{end_display.strftime('%m/%d %H:%M')}"


def _database_url() -> str | None:
    return (
        os.environ.get("QINTOPIA_MESSAGE_STORE_DATABASE_URL")
        or os.environ.get("QINTOPIA_SIDECAR_DATABASE_URL")
    )


def _require_read_through() -> bool:
    return os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE") == "1"


def _load_fixture(path: str) -> tuple[list[ReportMessage], dict[str, list[CreativeMemorySignal]]]:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    messages: list[ReportMessage] = []
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
                text=item.get("text") or "",
                sent_at=sent_at,
                message_kind=str(item.get("message_kind", "text")),
                sender_person_id=item.get("sender_person_id") or item.get("person_id"),
            )
        )

    memory: dict[str, list[CreativeMemorySignal]] = defaultdict(list)
    for item in data.get("creative_memory", []):
        person_id = str(item.get("person_id") or "")
        if not person_id:
            continue
        memory[person_id].append(
            CreativeMemorySignal(
                person_id=person_id,
                label=str(item.get("label") or item.get("fact_key") or item.get("fact_type") or "角色信号"),
                fact_type=str(item.get("fact_type") or "creative_profile"),
                count=int(item.get("count") or 1),
                last_seen=str(item.get("last_seen") or ""),
                public_safe=bool(item.get("public_safe", False)),
            )
        )
    return messages, dict(memory)


def _normalize_message_times(messages: list[ReportMessage], report_zone: ZoneInfo) -> list[ReportMessage]:
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
                sender_person_id=msg.sender_person_id,
            )
        )
    return normalized


def _psql_env_from_url(db_url: str) -> dict[str, str]:
    parsed = urlparse(db_url)
    if parsed.scheme not in {"postgres", "postgresql"}:
        raise RuntimeError("message store database URL must be postgres/postgresql")
    database = parsed.path.lstrip("/")
    if not database:
        raise RuntimeError("message store database URL is missing a database name")
    env = os.environ.copy()
    env["PATH"] = "/usr/bin:/bin"
    if parsed.hostname:
        env["PGHOST"] = parsed.hostname
    if parsed.port:
        env["PGPORT"] = str(parsed.port)
    if parsed.username:
        env["PGUSER"] = unquote(parsed.username)
    if parsed.password:
        env["PGPASSWORD"] = unquote(parsed.password)
    env["PGDATABASE"] = unquote(database)
    for key, value in parse_qs(parsed.query).items():
        if key == "sslmode" and value:
            env["PGSSLMODE"] = value[0]
    return env


def _run_psql_json(db_url: str, sql: str, variables: dict[str, str] | None = None) -> Any:
    cmd = ["/usr/bin/psql", "--no-psqlrc", "-X", "-q", "-t", "-A", "--set=ON_ERROR_STOP=1"]
    for key, value in (variables or {}).items():
        cmd.append(f"--set={key}={value}")
    try:
        completed = subprocess.run(
            cmd,
            input=sql,
            env=_psql_env_from_url(db_url),
            text=True,
            capture_output=True,
            check=False,
        )
    except FileNotFoundError as exc:
        raise RuntimeError("psycopg is unavailable and /usr/bin/psql was not found") from exc
    if completed.returncode != 0:
        raise RuntimeError("psql message-store read failed")
    output = completed.stdout.strip()
    return json.loads(output or "[]")


def _message_from_row(row: Any) -> ReportMessage:
    sent_at = None
    raw_sent = row.get("report_time")
    if raw_sent:
        sent_at = datetime.fromisoformat(str(raw_sent).replace("Z", "+00:00"))
    return ReportMessage(
        id=str(row.get("id") or ""),
        sender_id=str(row.get("sender_id") or ""),
        sender_name=str(row.get("sender_name") or "匿名"),
        text=row.get("text") or "",
        sent_at=sent_at,
        message_kind=str(row.get("message_kind") or "text"),
        sender_person_id=row.get("sender_person_id"),
    )


def _fetch_messages_with_psycopg(chat_id: str | None, start: datetime, end: datetime) -> list[ReportMessage]:
    db_url = _database_url()
    if not db_url:
        raise RuntimeError(
            "message store database URL not configured; "
            "set QINTOPIA_MESSAGE_STORE_DATABASE_URL or run with --fixture/--dry-run"
        )
    import psycopg

    sql = """
        SELECT
            m.id::text AS id,
            m.sender_id,
            m.sender_name,
            m.text,
            m.message_kind,
            m.sender_person_id::text AS sender_person_id,
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
                sender_person_id = row[5] if len(row) > 6 else None
                report_time = row[6] if len(row) > 6 else row[5]
                messages.append(
                    ReportMessage(
                        id=row[0],
                        sender_id=row[1] or "",
                        sender_name=row[2] or "匿名",
                        text=row[3] or "",
                        message_kind=row[4] or "text",
                        sender_person_id=sender_person_id,
                        sent_at=report_time,
                    )
                )
    return messages


def _fetch_messages_with_psql(chat_id: str | None, start: datetime, end: datetime) -> list[ReportMessage]:
    db_url = _database_url()
    if not db_url:
        raise RuntimeError(
            "message store database URL not configured; "
            "set QINTOPIA_MESSAGE_STORE_DATABASE_URL or run with --fixture/--dry-run"
        )
    chat_filter = "AND m.chat_id = :'chat_id'" if chat_id else ""
    sql = f"""
        WITH rows AS (
            SELECT jsonb_build_object(
                'id', m.id::text,
                'sender_id', COALESCE(m.sender_id, ''),
                'sender_name', COALESCE(m.sender_name, '匿名'),
                'text', COALESCE(m.text, ''),
                'message_kind', COALESCE(m.message_kind, 'text'),
                'sender_person_id', m.sender_person_id::text,
                'report_time', COALESCE(m.sent_at, m.received_at)
            ) AS item
            FROM qintopia_messages.messages m
            WHERE m.platform = 'qiwe'
              AND m.chat_type = 'group'
              AND m.message_kind = 'text'
              AND NULLIF(BTRIM(m.text), '') IS NOT NULL
              AND COALESCE(m.sent_at, m.received_at) >= :'start'::timestamptz
              AND COALESCE(m.sent_at, m.received_at) < :'end'::timestamptz
              {chat_filter}
            ORDER BY COALESCE(m.sent_at, m.received_at) ASC
        )
        SELECT COALESCE(jsonb_agg(item), '[]'::jsonb)::text FROM rows;
    """
    variables = {"start": start.isoformat(), "end": end.isoformat()}
    if chat_id:
        variables["chat_id"] = chat_id
    return [_message_from_row(row) for row in _run_psql_json(db_url, sql, variables)]


def _fetch_messages(chat_id: str | None, start: datetime, end: datetime) -> list[ReportMessage]:
    try:
        return _fetch_messages_with_psycopg(chat_id, start, end)
    except ImportError:
        return _fetch_messages_with_psql(chat_id, start, end)


def _fetch_creative_memory_with_psycopg(person_ids: list[str]) -> dict[str, list[CreativeMemorySignal]]:
    db_url = _database_url()
    if not db_url or not person_ids:
        return {}
    import psycopg

    sql = """
        SELECT
            person_id::text,
            COALESCE(NULLIF(fact_key, ''), fact_type) AS label,
            fact_type,
            count(*)::int AS signal_count,
            max(observed_at)::text AS last_seen,
            bool_or(lower(visibility) = 'public' OR lower(information_class) = 'public') AS public_safe
        FROM qintopia_identity.member_facts
        WHERE person_id = ANY(%s::uuid[])
          AND revoked_at IS NULL
          AND lower(visibility) IN ('internal', 'public')
        GROUP BY person_id, COALESCE(NULLIF(fact_key, ''), fact_type), fact_type
        ORDER BY signal_count DESC, last_seen DESC
    """
    memory: dict[str, list[CreativeMemorySignal]] = defaultdict(list)
    with psycopg.connect(db_url) as conn:
        with conn.cursor() as cur:
            cur.execute(sql, (person_ids,))
            for person_id, label, fact_type, count, last_seen, public_safe in cur.fetchall():
                memory[person_id].append(
                    CreativeMemorySignal(
                        person_id=person_id,
                        label=label,
                        fact_type=fact_type,
                        count=count,
                        last_seen=last_seen or "",
                        public_safe=bool(public_safe),
                    )
                )
    return dict(memory)


def _fetch_creative_memory_with_psql(person_ids: list[str]) -> dict[str, list[CreativeMemorySignal]]:
    db_url = _database_url()
    safe_ids = [pid for pid in person_ids if UUID_RE.match(pid)]
    if not db_url or not safe_ids:
        return {}
    values = ",\n".join(f"('{pid}'::uuid)" for pid in safe_ids)
    sql = f"""
        WITH requested(person_id) AS (VALUES {values}),
        rows AS (
            SELECT jsonb_build_object(
                'person_id', f.person_id::text,
                'label', COALESCE(NULLIF(f.fact_key, ''), f.fact_type),
                'fact_type', f.fact_type,
                'count', count(*)::int,
                'last_seen', max(f.observed_at)::text,
                'public_safe', bool_or(lower(f.visibility) = 'public' OR lower(f.information_class) = 'public')
            ) AS item
            FROM qintopia_identity.member_facts f
            JOIN requested r ON r.person_id = f.person_id
            WHERE f.revoked_at IS NULL
              AND lower(f.visibility) IN ('internal', 'public')
            GROUP BY f.person_id, COALESCE(NULLIF(f.fact_key, ''), f.fact_type), f.fact_type
            ORDER BY count(*) DESC, max(f.observed_at) DESC
        )
        SELECT COALESCE(jsonb_agg(item), '[]'::jsonb)::text FROM rows;
    """
    memory: dict[str, list[CreativeMemorySignal]] = defaultdict(list)
    for item in _run_psql_json(db_url, sql):
        person_id = str(item.get("person_id") or "")
        if not person_id:
            continue
        memory[person_id].append(
            CreativeMemorySignal(
                person_id=person_id,
                label=str(item.get("label") or "角色信号"),
                fact_type=str(item.get("fact_type") or "creative_profile"),
                count=int(item.get("count") or 1),
                last_seen=str(item.get("last_seen") or ""),
                public_safe=bool(item.get("public_safe")),
            )
        )
    return dict(memory)


def _fetch_creative_memory(person_ids: list[str]) -> dict[str, list[CreativeMemorySignal]]:
    safe_ids = sorted({pid for pid in person_ids if UUID_RE.match(pid)})
    if not safe_ids:
        return {}
    try:
        return _fetch_creative_memory_with_psycopg(safe_ids)
    except ImportError:
        return _fetch_creative_memory_with_psql(safe_ids)


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


def _clean_text(text: str) -> str:
    text = text or ""
    text = re.sub(r"https?://\S+", "", text)
    text = re.sub(r"(?<!\S)@(?:[A-Za-z0-9_.-]{1,64}|[\u4e00-\u9fff]{1,6})(?=\s|$)", "", text)
    text = re.sub(r"\[[^\]]{1,12}\]", "", text)
    text = re.sub(r"\s+", " ", text)
    return text.strip()


def _tokenize(text: str) -> list[str]:
    text = _clean_text(text).lower()
    try:
        import jieba

        tokens = list(jieba.lcut(text))
    except ImportError:
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
    if not kw or kw in STOP_WORDS:
        return False
    if kw.lower() in {"none", "null", "nan", "true", "false"}:
        return False
    return any("\u4e00" <= c <= "\u9fa5" for c in kw)


def _identity_key(msg: ReportMessage) -> str:
    if msg.sender_person_id:
        return f"person:{msg.sender_person_id}"
    if msg.sender_id:
        return f"sender:{msg.sender_id}"
    return f"name:{msg.sender_name or '匿名'}"


def _display_name_for_messages(messages: list[ReportMessage]) -> str:
    names = [m.sender_name for m in messages if m.sender_name and m.sender_name != "匿名"]
    if not names:
        return "匿名"
    return Counter(names).most_common(1)[0][0]


def _detect_topic_markers(messages: list[ReportMessage]) -> dict[str, list[ReportMessage]]:
    clusters: dict[str, list[ReportMessage]] = {}
    pattern = re.compile(r"^([^：:\n]{2,30})[：:]\s*")
    current_topic: str | None = None
    for msg in messages:
        cleaned = _clean_text(msg.text)
        if cleaned.startswith("#接龙"):
            body = cleaned[3:].strip()
            match = re.match(r"^([^\s，,0-9]{2,20})", body)
            current_topic = f"接龙 · {match.group(1) if match else body[:12]}"
        else:
            match = pattern.match(cleaned)
            if match:
                topic = match.group(1).strip()
                if 3 <= len(topic) <= 24 and not topic[-1].isdigit() and not topic.endswith(("，", ",", "、")):
                    current_topic = topic
        if current_topic:
            clusters.setdefault(current_topic, []).append(msg)
    return clusters


def _topic_frame(title: str) -> tuple[str, str]:
    for marker, frame in TOPIC_MARKERS.items():
        if marker in title:
            return frame
    return "今日支线", "这条线索不一定最大，但足够说明今天的群聊有自己的剧情。"


def _time_label_for_cluster(cluster: list[ReportMessage]) -> str:
    times = [m.sent_at for m in cluster if m.sent_at]
    if not times:
        return "时间未知"
    start_t, end_t = min(times), max(times)
    if start_t.date() == end_t.date():
        return f"{start_t.strftime('%H:%M')}–{end_t.strftime('%H:%M')}"
    return f"{start_t.strftime('%m/%d %H:%M')}–{end_t.strftime('%m/%d %H:%M')}"


def _representative_bullets(cluster: list[ReportMessage], limit: int = 3) -> list[str]:
    sorted_by_signal = sorted(
        cluster,
        key=lambda m: (-len(_clean_text(m.text)), m.sent_at.timestamp() if m.sent_at else 0),
    )
    bullets: list[str] = []
    for msg in sorted_by_signal:
        snippet = _clean_text(msg.text)
        if len(snippet) < 6:
            continue
        snippet = snippet[:72] + ("…" if len(snippet) > 72 else "")
        if snippet not in bullets:
            bullets.append(snippet)
        if len(bullets) >= limit:
            break
    return bullets or ["这条支线留下了足够的讨论痕迹。"]


def _build_narrative(topic: str, cluster: list[ReportMessage]) -> str:
    frame_title, frame_line = _topic_frame(topic)
    speaker_counts = Counter(m.sender_name or "匿名" for m in cluster)
    top_speakers = [name for name, _ in speaker_counts.most_common(3) if name != "匿名"]
    speaker_line = "、".join(top_speakers) if top_speakers else "群友们"
    first = _clean_text(cluster[0].text) if cluster else ""
    first = first[:42] + ("…" if len(first) > 42 else "")
    return f"{frame_line} {speaker_line}把它从一句“{first}”接成了{len(cluster)}条消息的小现场。"


def _cluster_cases(messages: list[ReportMessage], limit: int = DEFAULT_STORYLINE_LIMIT) -> list[CaseCard]:
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
        if best_keyword:
            clusters.setdefault(best_keyword, []).append(msg)

    sorted_clusters = sorted(clusters.items(), key=lambda item: (-len(item[1]), item[0]))
    cases: list[CaseCard] = []
    for index, (topic, cluster) in enumerate(sorted_clusters[:limit], start=1):
        if len(cluster) < DEFAULT_MIN_STORYLINE_MESSAGES:
            continue
        participants = {_identity_key(m) for m in cluster}
        speaker_counts = Counter(m.sender_name or "匿名" for m in cluster)
        top_speaker = speaker_counts.most_common(1)[0][0] if speaker_counts else "群友"
        frame_title, _ = _topic_frame(topic)
        chapter_title = f"第{index}章：{topic}，{frame_title}"
        cases.append(
            CaseCard(
                case_no=f"CHAPTER {index:02d}",
                title=topic,
                time_label=_time_label_for_cluster(cluster),
                summary=f"{len(cluster)} 条消息，{len(participants)} 个稳定身份参与",
                bullets=_representative_bullets(cluster),
                message_count=len(cluster),
                participant_count=len(participants),
                top_speaker=top_speaker,
                chapter_title=chapter_title,
                narrative=_build_narrative(topic, cluster),
                callback_hint=_callback_hint(topic),
            )
        )
    return cases


def _callback_hint(topic: str) -> str:
    if any(k in topic for k in ("接龙", "活动", "报名")):
        return "明天可以继续观察：这条接龙最后是变成现场，还是变成群聊考古。"
    if any(k in topic for k in ("吃", "喝", "饭", "酒")):
        return "这类线索适合沉淀进饭局宇宙，后续出现同款可以直接接梗。"
    if any(k in topic for k in ("学习", "分享", "资料")):
        return "这是知识分享系列的候选素材，适合回收成公开稿。"
    return "如果它在 7/14/30 天后复现，就可以升级成跨日故事线。"


def _compute_characters(
    messages: list[ReportMessage],
    memory: dict[str, list[CreativeMemorySignal]],
    limit: int = DEFAULT_CHARACTER_LIMIT,
) -> list[CharacterSketch]:
    grouped: dict[str, list[ReportMessage]] = defaultdict(list)
    for msg in messages:
        grouped[_identity_key(msg)].append(msg)

    duplicate_names = Counter(_display_name_for_messages(group) for group in grouped.values())
    ranked = sorted(grouped.items(), key=lambda item: (-len(item[1]), item[0]))[:limit]
    sketches: list[CharacterSketch] = []
    for rank, (key, group) in enumerate(ranked, start=1):
        name = _display_name_for_messages(group)
        if duplicate_names[name] > 1:
            name = f"{name}（当日身份{rank}）"
        word_count = sum(len(_clean_text(m.text)) for m in group)
        top_tokens = [kw for kw, _ in _keyword_scores(group).most_common(3)]
        if len(group) >= 8:
            role_line = "今日高频出场，把群聊节奏往前推了一截。"
        elif any(k in "".join(top_tokens) for k in ("活动", "接龙", "报名")):
            role_line = "今日更像活动线索的发起者或接球人。"
        elif any(k in "".join(top_tokens) for k in ("学习", "分享", "资料")):
            role_line = "今日承担了知识补给和信息投喂的角色。"
        else:
            role_line = "今日留下了能被写进日报的人物侧影。"

        person_id = key.removeprefix("person:") if key.startswith("person:") else ""
        signals = memory.get(person_id, [])
        public_signals = [s for s in signals if s.public_safe]
        private_count = sum(s.count for s in signals if not s.public_safe)
        if public_signals:
            fragments = [f"{_safe_label(s.label)}×{s.count}" for s in public_signals[:2]]
            memory_line = f"长期线索：{'、'.join(fragments)}"
        elif private_count:
            memory_line = f"私有人物画像候选 {private_count} 条，已放入审阅包，不直接上图。"
        else:
            memory_line = "暂无可上图长期标签，仅使用今日发言刻画。"

        sketches.append(
            CharacterSketch(
                rank=rank,
                identity_key=key,
                name=name,
                message_count=len(group),
                word_count=word_count,
                role_line=role_line,
                today_evidence=_representative_bullets(group, 2),
                memory_line=memory_line,
                private_memory_count=private_count,
            )
        )
    return sketches


def _safe_label(label: str) -> str:
    label = re.sub(r"[^\w\u4e00-\u9fa5\-·]", "", label)
    return label[:16] or "角色信号"


def _compute_suspects(messages: list[ReportMessage], limit: int = DEFAULT_CHARACTER_LIMIT) -> list[Suspect]:
    sketches = _compute_characters(messages, {}, limit)
    return [
        Suspect(
            rank=s.rank,
            name=s.name,
            message_count=s.message_count,
            word_count=s.word_count,
        )
        for s in sketches
    ]


def _extract_quotes(messages: list[ReportMessage], limit: int = DEFAULT_QUOTE_LIMIT) -> list[QuoteLine]:
    scored: list[tuple[int, ReportMessage, str]] = []
    for msg in messages:
        text = _clean_text(msg.text)
        if len(text) < 8 or len(text) > 96:
            continue
        score = len(text)
        if any(mark in text for mark in ("？", "?", "！", "!", "哈哈", "绝了", "离谱", "可以")):
            score += 30
        if text.startswith(("#接龙", "打卡")):
            score -= 20
        scored.append((score, msg, text))
    scored.sort(key=lambda item: (-item[0], item[1].sent_at or datetime.min.replace(tzinfo=ZoneInfo(DEFAULT_TIMEZONE))))
    quotes: list[QuoteLine] = []
    seen: set[str] = set()
    for _, msg, text in scored:
        if text in seen:
            continue
        seen.add(text)
        quotes.append(
            QuoteLine(
                speaker=msg.sender_name or "群友",
                text=text,
                time_label=msg.sent_at.strftime("%H:%M") if msg.sent_at else "时间未知",
            )
        )
        if len(quotes) >= limit:
            break
    return quotes


def _extract_highlight(messages: list[ReportMessage]) -> str:
    quotes = _extract_quotes(messages, 1)
    if quotes:
        return quotes[0].text
    return "今日群聊很安静，但安静本身也是社区节奏的一部分。"


def _hourly_timeline(messages: list[ReportMessage], start: datetime, buckets: int = DEFAULT_HOURLY_BUCKETS) -> list[int]:
    counts = [0] * buckets
    for msg in messages:
        if not msg.sent_at:
            continue
        delta = msg.sent_at - start
        hour = int(delta.total_seconds() // 3600)
        if 0 <= hour < buckets:
            counts[hour] += 1
    return counts


def _headline(cases: list[CaseCard], messages: list[ReportMessage]) -> tuple[str, str, str]:
    if cases:
        top = cases[0]
        headline = f"{top.title}成了今天的主线"
        subtitle = f"{top.summary}，{top.top_speaker}站在风口附近。"
        opening = top.narrative
        return headline, subtitle, opening
    if messages:
        return (
            "今天没有大案，只有社区的日常心跳",
            f"{len(messages)} 条消息像散点一样落在一天里。",
            "参考项目最重要的不是热闹本身，而是把日常里的角色、梗和线索留下来。",
        )
    return (
        "今天群聊留白",
        "没有足够消息生成主线，但日报系统仍完成了值守。",
        "留白不强行编故事，等下一次真实对话把页面点亮。",
    )


def _tomorrow_clues(cases: list[CaseCard], messages: list[ReportMessage]) -> list[str]:
    clues: list[str] = []
    for case in cases[:3]:
        if case.callback_hint:
            clues.append(case.callback_hint)
    if any("明天" in _clean_text(m.text) for m in messages):
        clues.append("群聊已经显式提到明天，适合在下一份日报里做回收检查。")
    if not clues:
        clues.append("观察今天的高频人物是否连续出现，连续三天即可形成角色弧线。")
    return clues[:4]


def _build_report(args: argparse.Namespace) -> ReportData:
    start, end, display_date = _report_date(args)
    report_zone = _report_timezone(args.timezone)

    fixture_memory: dict[str, list[CreativeMemorySignal]] = {}
    if args.fixture:
        messages, fixture_memory = _load_fixture(args.fixture)
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
    person_ids = [m.sender_person_id for m in messages if m.sender_person_id]
    memory = fixture_memory or (_fetch_creative_memory(person_ids) if person_ids else {})
    cases = _cluster_cases(messages)
    characters = _compute_characters(messages, memory)
    suspects = _compute_suspects(messages)
    quotes = _extract_quotes(messages)
    hourly = _hourly_timeline(messages, start)
    headline, subtitle, opening = _headline(cases, messages)
    time_range = _time_range_label(start, end)
    unique_identities = {_identity_key(m) for m in messages}

    report = ReportData(
        group_name=args.group_name,
        report_title=args.report_title,
        report_date=display_date,
        time_range=time_range,
        member_count=int(os.environ.get("QINTOPIA_DAILY_CASE_REPORT_MEMBER_COUNT", len(unique_identities) or 1)),
        message_count=len(messages),
        participant_count=len(unique_identities),
        case_count=len(cases),
        suspect_count=min(len(suspects), DEFAULT_CHARACTER_LIMIT),
        hourly_counts=hourly,
        cases=cases,
        suspects=suspects,
        quote=os.environ.get("QINTOPIA_DAILY_CASE_REPORT_QUOTE", "所有引用可回溯至当天群聊消息。"),
        highlight=_extract_highlight(messages),
        headline=headline,
        subtitle=subtitle,
        opening=opening,
        characters=characters,
        quotes=quotes,
        tomorrow_clues=_tomorrow_clues(cases, messages),
    )
    report.draft_bundle = _build_draft_bundle(report, memory)
    return report


def _sample_messages(start: datetime) -> list[ReportMessage]:
    demos = [
        ("08:05", "阿杰", "每日共学打卡：今天第 8 天，把 Solidity 函数修饰符啃完了，合约编译通过"),
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
        ("16:45", "小林", "新人报到：刚进群，跟着大家从零学 web3，请多关照。"),
        ("16:50", "阿杰", "欢迎欢迎，置顶文档先看一遍。"),
        ("16:55", "Mia", "有问题随时问，群里氛围很友好。"),
        ("20:10", "小雨", "今日收尾：今天收获满满，明天继续打卡。"),
        ("20:15", "阿杰", "+1，一起加油。"),
    ]
    messages: list[ReportMessage] = []
    sample_person_ids = {
        name: f"00000000-0000-0000-0000-{idx:012d}"
        for idx, name in enumerate(sorted({name for _, name, _ in demos}), start=1)
    }
    for idx, (time_str, name, text) in enumerate(demos, start=1):
        hour, minute = map(int, time_str.split(":"))
        sent_at = start + timedelta(hours=hour, minutes=minute)
        messages.append(
            ReportMessage(
                id=f"demo-{idx}",
                sender_id=f"user-{name}",
                sender_person_id=sample_person_ids[name],
                sender_name=name,
                text=text,
                sent_at=sent_at,
                message_kind="text",
            )
        )
    return messages


def _build_draft_bundle(report: ReportData, memory: dict[str, list[CreativeMemorySignal]]) -> DraftBundle:
    digest = _build_digest_markdown(report)
    roast = _build_roast_markdown(report)
    public_draft = _build_public_draft_markdown(report)
    quote_map = [
        {"speaker": quote.speaker, "time": quote.time_label, "text": quote.text}
        for quote in report.quotes
    ]
    profile_candidates: list[dict[str, Any]] = []
    for character in report.characters:
        signals = memory.get(character.identity_key.removeprefix("person:"), [])
        profile_candidates.append(
            {
                "display_name": character.name,
                "identity_scope": "person_id" if character.identity_key.startswith("person:") else "sender_fallback",
                "today_role": character.role_line,
                "today_evidence_count": len(character.today_evidence),
                "memory_signal_count": sum(signal.count for signal in signals),
                "public_safe_signal_count": sum(signal.count for signal in signals if signal.public_safe),
                "private_signal_count": sum(signal.count for signal in signals if not signal.public_safe),
                "candidate_labels": [
                    {"label": _safe_label(signal.label), "count": signal.count, "public_safe": signal.public_safe}
                    for signal in signals[:6]
                ],
            }
        )
    return DraftBundle(
        digest_markdown=digest,
        roast_markdown=roast,
        public_draft_markdown=public_draft,
        quote_map=quote_map,
        profile_candidates=profile_candidates,
        privacy_flags={
            "stable_identity_grouping": True,
            "raw_member_fact_text_retained": False,
            "private_profile_text_excluded_from_public_image": True,
            "external_send_executed": False,
            "requires_human_confirmation": True,
        },
        draft_counts={
            "digest": 1,
            "roast": 1,
            "public_draft": 1,
            "quote_map": len(quote_map),
            "profile_candidates": len(profile_candidates),
            "storylines": len(report.cases),
        },
    )


def _build_digest_markdown(report: ReportData) -> str:
    lines = [
        f"# {report.group_name}事实日报 | {report.report_date}",
        "",
        f"- 时间范围：{report.time_range}",
        f"- 消息总量：{report.message_count}",
        f"- 活跃身份：{report.participant_count}",
        f"- 今日主线：{report.headline}",
        "",
        "## 主要话题",
    ]
    for case in report.cases:
        lines.append(f"- {case.title}：{case.summary}")
    lines.extend(["", "## 人物动态"])
    for character in report.characters:
        lines.append(f"- {character.name}：{character.role_line}（{character.message_count} 条）")
    lines.extend(["", "## 待回收线索"])
    lines.extend(f"- {clue}" for clue in report.tomorrow_clues)
    return "\n".join(lines)


def _build_roast_markdown(report: ReportData) -> str:
    lines = [
        f"# {report.group_name}日报 | {report.report_date} | {report.headline}",
        "",
        f"**战报**：{report.message_count} 条消息，{report.participant_count} 个稳定身份出场。",
        "",
        report.opening,
        "",
    ]
    for case in report.cases:
        lines.extend([f"## {case.chapter_title}", "", case.narrative, ""])
        for bullet in case.bullets:
            lines.append(f"- {bullet}")
        lines.extend(["", f"> {case.callback_hint}", ""])
    lines.append("## 今日人物速写")
    for character in report.characters:
        lines.extend(["", f"> **{character.name}**：{character.role_line} {character.memory_line}"])
    lines.extend(["", "## 今日金句"])
    for quote in report.quotes:
        lines.append(f"- {quote.speaker} {quote.time_label}：{quote.text}")
    lines.extend(["", "## 明日线索"])
    lines.extend(f"- {clue}" for clue in report.tomorrow_clues)
    lines.extend(["", "*图片草稿优先，PDF 仅作为内部归档候选。*"])
    return "\n".join(lines)


def _build_public_draft_markdown(report: ReportData) -> str:
    lines = [
        f"# {report.headline}",
        "",
        report.subtitle,
        "",
        "今天的群聊不是流水账，而是一张社区运行图：谁在发起，谁在接球，哪些梗值得以后回收。",
        "",
    ]
    for case in report.cases[:4]:
        lines.extend([f"## {case.title}", "", case.narrative, ""])
    lines.append("这份公开候选稿必须人工审核后才能外发。")
    return "\n".join(lines)


def _bar_svg(counts: list[int], max_count: int, width: int, height: int) -> str:
    if not counts or max_count == 0:
        return ""
    bar_width = width / len(counts)
    bars = []
    for idx, count in enumerate(counts):
        h = int((count / max_count) * height) if max_count else 0
        x = int(idx * bar_width)
        y = height - h
        bars.append(f'<rect x="{x}" y="{y}" width="{max(3, int(bar_width) - 3)}" height="{h}" fill="#1f1b18"/>')
    return "\n".join(bars)


def _render_html(report: ReportData, width: int) -> str:
    max_hourly = max(report.hourly_counts) if report.hourly_counts else 1
    timeline_svg = _bar_svg(report.hourly_counts, max_hourly, width - 96, 88)

    chapters = "\n".join(
        f"""
        <section class="chapter">
          <div class="chapter-meta">{html.escape(case.case_no)} · {html.escape(case.time_label)} · {case.message_count} 条</div>
          <h2>{html.escape(case.chapter_title or case.title)}</h2>
          <p>{html.escape(case.narrative)}</p>
          <ul>{"".join(f"<li>{html.escape(item)}</li>" for item in case.bullets)}</ul>
          <div class="callback">{html.escape(case.callback_hint)}</div>
        </section>
        """
        for case in report.cases
    )
    if not chapters:
        chapters = '<section class="chapter"><h2>今日留白</h2><p>今天没有足够主线，日报只保留真实节奏，不强行编故事。</p></section>'

    characters = "\n".join(
        f"""
        <div class="person">
          <div class="person-rank">{character.rank:02d}</div>
          <div>
            <h3>{html.escape(character.name)}</h3>
            <p>{html.escape(character.role_line)}</p>
            <small>{html.escape(character.memory_line)}</small>
          </div>
        </div>
        """
        for character in report.characters
    )

    quotes = "\n".join(
        f"""
        <blockquote>
          <p>{html.escape(quote.text)}</p>
          <cite>{html.escape(quote.speaker)} · {html.escape(quote.time_label)}</cite>
        </blockquote>
        """
        for quote in report.quotes[:4]
    )
    if not quotes:
        quotes = "<p class=\"muted\">今天没有适合上图的金句。</p>"

    tomorrow = "".join(f"<li>{html.escape(clue)}</li>" for clue in report.tomorrow_clues)

    return f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{
    background: #171310;
    font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
    color: #1f1b18;
  }}
  .poster {{
    width: {width}px;
    min-height: 1200px;
    padding: 34px 30px 38px;
    background:
      linear-gradient(rgba(31, 27, 24, 0.055) 1px, transparent 1px),
      linear-gradient(90deg, rgba(31, 27, 24, 0.045) 1px, transparent 1px),
      linear-gradient(135deg, #fbf3e7 0%, #f3e0c2 52%, #e8cfaa 100%);
    background-size: 28px 28px, 28px 28px, auto;
  }}
  .masthead {{
    border: 5px solid #1f1b18;
    padding: 24px;
    background: #fff8eb;
    box-shadow: 8px 8px 0 rgba(31, 27, 24, 0.16);
    margin-bottom: 24px;
  }}
  .kicker {{
    display: inline-block;
    background: #1f1b18;
    color: #f2c94c;
    padding: 8px 12px;
    font-size: 13px;
    font-weight: 900;
    margin-bottom: 14px;
  }}
  h1 {{
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 45px;
    line-height: 1.08;
    font-weight: 900;
    letter-spacing: 0;
    margin-bottom: 12px;
  }}
  .subtitle {{ font-size: 15px; line-height: 1.7; color: #554a42; }}
  .stats {{
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    margin-bottom: 22px;
  }}
  .stat {{
    border: 4px solid #1f1b18;
    background: #f2c94c;
    padding: 12px 8px;
    text-align: center;
    box-shadow: 5px 5px 0 rgba(31, 27, 24, 0.14);
  }}
  .stat:nth-child(2) {{ background: #b4312b; color: #fff8eb; }}
  .stat:nth-child(3) {{ background: #1f8f5f; color: #fff8eb; }}
  .stat:nth-child(4) {{ background: #2f6f91; color: #fff8eb; }}
  .stat strong {{ display: block; font-size: 30px; line-height: 1; }}
  .stat span {{ display: block; margin-top: 6px; font-size: 11px; font-weight: 800; }}
  .timeline, .chapter, .people, .quotes, .tomorrow {{
    border: 4px solid #1f1b18;
    background: #fff8eb;
    padding: 20px;
    margin-bottom: 18px;
    box-shadow: 7px 7px 0 rgba(31, 27, 24, 0.13);
  }}
  .section-title {{
    display: inline-block;
    background: #1f1b18;
    color: #fff8eb;
    padding: 7px 11px;
    font-size: 13px;
    font-weight: 900;
    margin-bottom: 14px;
  }}
  .chapter-meta {{
    color: #b4312b;
    font-size: 12px;
    font-weight: 900;
    margin-bottom: 8px;
  }}
  h2 {{
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 25px;
    line-height: 1.22;
    letter-spacing: 0;
    margin-bottom: 10px;
  }}
  .chapter p, .subtitle, .person p, blockquote p {{
    font-size: 15px;
    line-height: 1.72;
  }}
  .chapter ul, .tomorrow ul {{ margin: 12px 0 0 18px; }}
  .chapter li, .tomorrow li {{ font-size: 14px; line-height: 1.65; margin-bottom: 7px; }}
  .callback {{
    margin-top: 12px;
    padding: 10px 12px;
    background: #f7efe2;
    border-left: 5px solid #b4312b;
    font-size: 13px;
    line-height: 1.6;
    color: #554a42;
  }}
  .person {{
    display: grid;
    grid-template-columns: 46px 1fr;
    gap: 12px;
    padding: 13px 0;
    border-top: 1px solid rgba(31, 27, 24, 0.18);
  }}
  .person:first-of-type {{ border-top: 0; }}
  .person-rank {{
    width: 42px;
    height: 42px;
    border: 3px solid #1f1b18;
    background: #f2c94c;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 900;
  }}
  h3 {{ font-size: 17px; margin-bottom: 5px; }}
  small {{ display: block; margin-top: 6px; color: #73675f; line-height: 1.55; }}
  blockquote {{
    border-left: 5px solid #1f8f5f;
    padding: 10px 0 10px 14px;
    margin-bottom: 10px;
    background: #f7efe2;
  }}
  cite {{ display: block; margin-top: 6px; font-size: 12px; color: #73675f; font-style: normal; }}
  .footer {{
    text-align: center;
    color: #73675f;
    font-size: 11px;
    line-height: 1.7;
    padding-top: 8px;
  }}
  .muted {{ color: #73675f; font-size: 14px; }}
</style>
</head>
<body>
<main class="poster">
  <header class="masthead">
    <div class="kicker">WX-CLI STYLE DAILY · IMAGE FIRST</div>
    <h1>{html.escape(report.headline)}</h1>
    <p class="subtitle">{html.escape(report.subtitle)}</p>
    <p class="subtitle">{html.escape(report.group_name)} · {html.escape(report.report_date)} · {html.escape(report.time_range)}</p>
  </header>

  <section class="stats">
    <div class="stat"><strong>{report.message_count}</strong><span>消息</span></div>
    <div class="stat"><strong>{report.participant_count}</strong><span>稳定身份</span></div>
    <div class="stat"><strong>{report.case_count}</strong><span>故事线</span></div>
    <div class="stat"><strong>{report.suspect_count}</strong><span>人物速写</span></div>
  </section>

  <section class="timeline">
    <div class="section-title">24H 群聊心电图</div>
    <svg width="{width - 60}" height="104" viewBox="0 0 {width - 60} 104">
      <g transform="translate(18, 8)">{timeline_svg}</g>
    </svg>
  </section>

  {chapters}

  <section class="people">
    <div class="section-title">今日人物速写</div>
    {characters}
  </section>

  <section class="quotes">
    <div class="section-title">今日金句</div>
    {quotes}
  </section>

  <section class="tomorrow">
    <div class="section-title">明日线索</div>
    <ul>{tomorrow}</ul>
  </section>

  <div class="footer">图片草稿，未发送。长期人物画像默认进入私有审阅包，公开前必须人工确认。</div>
</main>
</body>
</html>"""


def _file_url(path: Path) -> str:
    return path.resolve().as_uri()


def _render_png(
    html_path: Path,
    output_path: Path,
    width: int,
    report: ReportData | None = None,
) -> None:
    try:
        from playwright.sync_api import sync_playwright
    except ImportError as exc:
        if report is not None:
            _render_png_with_pillow(report, output_path, width)
            return
        raise RuntimeError("playwright or Pillow is required for PNG rendering; use --render html for debug") from exc

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


def _load_pillow_font(size: int, bold: bool = False):
    from PIL import ImageFont

    preferred = [
        os.environ.get("QINTOPIA_DAILY_CASE_REPORT_FONT"),
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc" if bold else "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    for path in preferred:
        if path and Path(path).exists():
            try:
                return ImageFont.truetype(path, size=size)
            except OSError:
                continue
    return ImageFont.load_default()


def _render_png_with_pillow(report: ReportData, output_path: Path, width: int) -> None:
    try:
        from PIL import Image, ImageDraw
    except ImportError as exc:
        raise RuntimeError("playwright is unavailable and Pillow renderer is not installed") from exc

    ink = "#1f1b18"
    muted = "#73675f"
    paper = "#fbf3e7"
    card = "#fff8eb"
    red = "#b4312b"
    green = "#1f8f5f"
    blue = "#2f6f91"
    yellow = "#f2c94c"
    margin = 30
    inner = width - margin * 2
    image = Image.new("RGB", (width, 16000), paper)
    draw = ImageDraw.Draw(image)

    title_font = _load_pillow_font(34, bold=True)
    h2_font = _load_pillow_font(22, bold=True)
    h3_font = _load_pillow_font(17, bold=True)
    body_font = _load_pillow_font(15)
    small_font = _load_pillow_font(12)
    stat_font = _load_pillow_font(28, bold=True)
    label_font = _load_pillow_font(12, bold=True)

    def ensure_space(y: int, extra: int) -> tuple[Any, Any]:
        nonlocal image, draw
        if y + extra <= image.height - margin:
            return image, draw
        expanded = Image.new("RGB", (width, image.height + 8000), paper)
        expanded.paste(image, (0, 0))
        image = expanded
        draw = ImageDraw.Draw(image)
        return image, draw

    def text_width(text: str, font: Any) -> int:
        bbox = draw.textbbox((0, 0), text, font=font)
        return bbox[2] - bbox[0]

    def wrap_text(text: str, font: Any, max_width: int) -> list[str]:
        lines: list[str] = []
        for paragraph in str(text).splitlines() or [""]:
            current = ""
            for char in paragraph:
                candidate = current + char
                if current and text_width(candidate, font) > max_width:
                    lines.append(current)
                    current = char
                else:
                    current = candidate
            if current:
                lines.append(current)
            if not paragraph:
                lines.append("")
        return lines or [""]

    def draw_wrapped(text: str, x: int, y: int, font: Any, fill: str, max_width: int, gap: int = 6) -> int:
        for line in wrap_text(text, font, max_width):
            bbox = draw.textbbox((x, y), line or " ", font=font)
            height = bbox[3] - bbox[1]
            draw.text((x, y), line, font=font, fill=fill)
            y += height + gap
        return y

    def draw_section_box(y: int, height: int, fill: str = card, outline: str = ink) -> int:
        ensure_space(y, height + 16)
        draw.rectangle([margin, y, width - margin, y + height], fill=fill, outline=outline, width=4)
        return y + 18

    def draw_label(label: str, x: int, y: int, fill: str = ink) -> int:
        label_width = text_width(label, label_font) + 18
        draw.rectangle([x, y, x + label_width, y + 26], fill=fill)
        draw.text((x + 9, y + 6), label, font=label_font, fill=card)
        return y + 38

    y = margin
    masthead_height = 230
    draw_section_box(y, masthead_height)
    y += 22
    draw.rectangle([margin + 20, y, margin + 220, y + 28], fill=ink)
    draw.text((margin + 30, y + 7), "WX-CLI DAILY · IMAGE FIRST", font=label_font, fill=yellow)
    y += 44
    y = draw_wrapped(report.headline, margin + 20, y, title_font, ink, inner - 40, 8)
    y = draw_wrapped(report.subtitle, margin + 20, y + 6, body_font, muted, inner - 40, 6)
    y = draw_wrapped(
        f"{report.group_name} · {report.report_date} · {report.time_range}",
        margin + 20,
        y + 2,
        small_font,
        muted,
        inner - 40,
        4,
    )
    y = margin + masthead_height + 22

    stat_gap = 10
    stat_w = (inner - stat_gap * 3) // 4
    stats = [
        (str(report.message_count), "消息", yellow, ink),
        (str(report.participant_count), "稳定身份", red, card),
        (str(report.case_count), "故事线", green, card),
        (str(report.suspect_count), "人物速写", blue, card),
    ]
    for idx, (value, label, bg, fg) in enumerate(stats):
        x = margin + idx * (stat_w + stat_gap)
        draw.rectangle([x, y, x + stat_w, y + 82], fill=bg, outline=ink, width=4)
        value_w = text_width(value, stat_font)
        draw.text((x + (stat_w - value_w) // 2, y + 12), value, font=stat_font, fill=fg)
        label_w = text_width(label, small_font)
        draw.text((x + (stat_w - label_w) // 2, y + 52), label, font=small_font, fill=fg)
    y += 106

    timeline_height = 142
    box_top = y
    draw_section_box(box_top, timeline_height)
    draw_label("24H 群聊心电图", margin + 18, box_top + 18)
    max_hourly = max(report.hourly_counts) if report.hourly_counts else 1
    chart_x = margin + 24
    chart_y = box_top + 74
    chart_w = inner - 48
    chart_h = 42
    bar_w = max(4, chart_w // max(1, len(report.hourly_counts)))
    for idx, count in enumerate(report.hourly_counts):
        h = int((count / max_hourly) * chart_h) if max_hourly else 0
        x = chart_x + idx * bar_w
        draw.rectangle([x, chart_y + chart_h - h, x + max(2, bar_w - 2), chart_y + chart_h], fill=ink)
    y = box_top + timeline_height + 18

    for case in report.cases:
        estimated = 190 + 28 * len(case.bullets)
        box_top = y
        draw_section_box(box_top, estimated)
        local_y = box_top + 18
        local_y = draw_wrapped(
            f"{case.case_no} · {case.time_label} · {case.message_count} 条",
            margin + 18,
            local_y,
            small_font,
            red,
            inner - 36,
            4,
        )
        local_y = draw_wrapped(case.chapter_title or case.title, margin + 18, local_y + 4, h2_font, ink, inner - 36, 7)
        local_y = draw_wrapped(case.narrative, margin + 18, local_y + 2, body_font, ink, inner - 36, 6)
        for bullet in case.bullets:
            local_y = draw_wrapped(f"- {bullet}", margin + 28, local_y + 2, small_font, ink, inner - 46, 5)
        draw.rectangle([margin + 18, local_y + 8, width - margin - 18, local_y + 62], fill="#f7efe2")
        draw.rectangle([margin + 18, local_y + 8, margin + 23, local_y + 62], fill=red)
        local_y = draw_wrapped(case.callback_hint, margin + 34, local_y + 16, small_font, muted, inner - 58, 4)
        y = max(box_top + estimated + 18, local_y + 24)

    box_top = y
    character_height = 70 + 96 * max(1, len(report.characters))
    draw_section_box(box_top, character_height)
    local_y = draw_label("今日人物速写", margin + 18, box_top + 18)
    for character in report.characters:
        draw.rectangle([margin + 18, local_y, margin + 58, local_y + 40], fill=yellow, outline=ink, width=2)
        draw.text((margin + 26, local_y + 11), f"{character.rank:02d}", font=small_font, fill=ink)
        text_x = margin + 70
        draw.text((text_x, local_y), character.name, font=h3_font, fill=ink)
        local_y = draw_wrapped(character.role_line, text_x, local_y + 24, small_font, ink, inner - 88, 5)
        local_y = draw_wrapped(character.memory_line, text_x, local_y, small_font, muted, inner - 88, 5) + 10
    y = box_top + character_height + 18

    box_top = y
    quote_height = 68 + 86 * max(1, min(len(report.quotes), 4))
    draw_section_box(box_top, quote_height)
    local_y = draw_label("今日金句", margin + 18, box_top + 18)
    for quote in report.quotes[:4]:
        draw.rectangle([margin + 18, local_y, margin + 23, local_y + 58], fill=green)
        local_y = draw_wrapped(quote.text, margin + 34, local_y + 2, body_font, ink, inner - 58, 5)
        local_y = draw_wrapped(f"{quote.speaker} · {quote.time_label}", margin + 34, local_y, small_font, muted, inner - 58, 5) + 8
    y = box_top + quote_height + 18

    box_top = y
    tomorrow_height = 74 + 42 * max(1, len(report.tomorrow_clues))
    draw_section_box(box_top, tomorrow_height)
    local_y = draw_label("明日线索", margin + 18, box_top + 18)
    for clue in report.tomorrow_clues:
        local_y = draw_wrapped(f"- {clue}", margin + 28, local_y, small_font, ink, inner - 46, 5)
    y = box_top + tomorrow_height + 24

    y = draw_wrapped(
        "图片草稿，未发送。长期人物画像默认进入私有审阅包，公开前必须人工确认。",
        margin,
        y,
        small_font,
        muted,
        inner,
        5,
    )
    cropped = image.crop((0, 0, width, min(image.height, y + margin)))
    cropped.save(output_path, format="PNG")
    os.chmod(output_path, 0o600)


def _operator_review_message(
    report: ReportData,
    html_path: Path,
    png_path: Path | None,
    draft_bundle_path: Path | None,
    include_html: bool = False,
) -> str:
    lines = [
        f"【{report.group_name}｜{report.report_title}】",
        f"日报日期：{report.report_date}（{report.time_range}）",
        f"主标题：{report.headline}",
        f"消息 {report.message_count} 条 / 活跃身份 {report.participant_count} 个 / 故事线 {report.case_count} 条 / 人物速写 {report.suspect_count} 个",
        "",
    ]
    for case in report.cases[:5]:
        lines.append(f"• {case.chapter_title or case.title}（{case.summary}）")
    lines.append("")
    if png_path:
        lines.append(f"图片（群内主交付）：{png_path}")
    if draft_bundle_path:
        lines.append(f"私有审阅包：{draft_bundle_path}")
    if include_html and html_path.exists():
        label = "HTML 预览（仅调试用）" if png_path else "HTML 预览"
        lines.append(f"{label}：{html_path}")
    lines.append("")
    lines.append("本报告仅生成草稿，未发送到任何群聊。确认无误后再进入图片上传和群发链路。")
    return "\n".join(lines)


def _public_output_style(report: ReportData) -> dict[str, bool | str | int]:
    return {
        "schema_version": "xiaoman-character-daily-image-v1",
        "image_first_delivery": True,
        "pdf_default_delivery": False,
        "storyline_first_output": True,
        "character_cards_present": bool(report.characters),
        "quote_section_present": bool(report.quotes),
        "tomorrow_clues_present": bool(report.tomorrow_clues),
        "stable_identity_grouping": True,
        "private_draft_boundary": True,
    }


def _write_draft_bundle(report: ReportData, path: Path) -> None:
    bundle = report.draft_bundle
    if bundle is None:
        return
    payload = {
        "schema_version": "xiaoman-daily-wx-cli-draft-bundle-v1",
        "digest_markdown": bundle.digest_markdown,
        "roast_markdown": bundle.roast_markdown,
        "public_draft_markdown": bundle.public_draft_markdown,
        "quote_map": bundle.quote_map,
        "profile_candidates": bundle.profile_candidates,
        "privacy_flags": bundle.privacy_flags,
        "draft_counts": bundle.draft_counts,
    }
    _write_private_text(path, json.dumps(payload, ensure_ascii=False, indent=2))


def _result_json(
    report: ReportData,
    deliverable_path: Path,
    png_path: Path | None,
    draft_bundle_path: Path | None,
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
        "storyline_count": report.case_count,
        "character_count": len(report.characters),
        "quote_count": len(report.quotes),
        "deliverable_path": str(deliverable_path),
        "png_path": str(png_path) if png_path else None,
        "draft_bundle_path": str(draft_bundle_path) if draft_bundle_path else None,
        "html_path": str(html_path) if html_exists else None,
        "draft_counts": report.draft_bundle.draft_counts if report.draft_bundle else {},
        "privacy_flags": report.draft_bundle.privacy_flags if report.draft_bundle else {},
        "public_output_style": _public_output_style(report),
        "operator_review_message": _operator_review_message(
            report,
            html_path or deliverable_path,
            png_path,
            draft_bundle_path,
            html_exists,
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
    html_path = output_dir / f"xiaoman-daily-report-{timestamp}.html"
    png_path = output_dir / f"xiaoman-daily-report-{timestamp}.png"
    draft_bundle_path = output_dir / f"xiaoman-daily-report-{timestamp}.draft-bundle.json"

    _write_private_text(html_path, _render_html(report, args.output_width))
    _write_draft_bundle(report, draft_bundle_path)

    png_generated = False
    try:
        if args.render in ("auto", "png"):
            try:
                _render_png(html_path, png_path, args.output_width, report)
                png_generated = True
            except RuntimeError as exc:
                print(f"WARN: PNG rendering skipped: {exc}", file=sys.stderr)
                if args.render == "png" or real_messages:
                    return 2

        deliverable = png_path if png_generated else html_path
        result = _result_json(
            report,
            deliverable,
            png_path if png_generated else None,
            draft_bundle_path if draft_bundle_path.exists() else None,
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
