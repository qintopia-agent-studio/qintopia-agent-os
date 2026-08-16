"""Message and memory collection layer for the daily case-report pipeline.

Handles:
- Postgres message fetching (psycopg or psql fallback)
- Character memory queries
- Creative profile memory queries
- Fixture loading for dry-run mode
"""
from __future__ import annotations

import json
import os
import re
import subprocess
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, unquote, urlparse

from models import (
    CharacterMemory,
    CreativeProfileMemory,
    MEMORY_FACT_ROLE_LABELS,
    MEMORY_FACT_TYPES,
    MEMORY_LOOKBACK_DAYS,
    PRODUCTION_PSQL_BIN,
    PRODUCTION_PSQL_PATH,
    ReportMessage,
    clean_text,
    memory_callback_seed,
    memory_depth_label,
    memory_recurrence_label,
    memory_weight_label,
)


# ---------------------------------------------------------------------------
# Database URL / read-through gates
# ---------------------------------------------------------------------------

def database_url() -> str | None:
    return (
        os.environ.get("QINTOPIA_MESSAGE_STORE_DATABASE_URL")
        or os.environ.get("QINTOPIA_SIDECAR_DATABASE_URL")
    )


def require_read_through() -> bool:
    return os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE") == "1"


# ---------------------------------------------------------------------------
# Fixture loading
# ---------------------------------------------------------------------------

def load_fixture(path: str) -> list[ReportMessage]:
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


# ---------------------------------------------------------------------------
# Message fetching
# ---------------------------------------------------------------------------

def fetch_messages(
    chat_id: str | None,
    start: datetime,
    end: datetime,
) -> list[ReportMessage]:
    db_url = database_url()
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


# ---------------------------------------------------------------------------
# Character memory
# ---------------------------------------------------------------------------

def fetch_character_memory(
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
    db_url = database_url()
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
            recurrence_label=memory_recurrence_label(int(recent_count or 0)),
            depth_label=memory_depth_label(int(lifetime_count or 0)),
            memory_weight_label=memory_weight_label(int(recent_count or 0), int(lifetime_count or 0)),
            callback_seed=memory_callback_seed(role_label, int(recent_count or 0)),
        )
    return memory


# ---------------------------------------------------------------------------
# Creative profile memory
# ---------------------------------------------------------------------------

def _safe_creative_text(value: Any, limit: int = 80) -> str:
    if not isinstance(value, str):
        return ""
    cleaned = clean_text(value).strip()
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


def _reviewed_public_expressive_label(safe_reply_hints: dict[str, Any]) -> str:
    labels = safe_reply_hints.get("public_expressive_labels")
    if not isinstance(labels, dict):
        return ""
    if labels.get("public_surface_allowed") is not True:
        return ""
    if str(labels.get("review_status") or "") not in {"reviewed", "approved"}:
        return ""
    return _safe_creative_text(
        labels.get("relationship_tension")
        or labels.get("callback_label")
        or labels.get("roast_label"),
        48,
    )


def fetch_creative_profile_memory(
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
    db_url = database_url()
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
            expressive_label=_reviewed_public_expressive_label(safe_reply_hints),
            evidence_anchor=_safe_creative_text(safe_reply_hints.get("evidence_anchor"), 80),
            recurrence_evidence_count=_safe_creative_int(
                safe_reply_hints.get("recurrence_evidence_count")
            ),
        )
    return memory
