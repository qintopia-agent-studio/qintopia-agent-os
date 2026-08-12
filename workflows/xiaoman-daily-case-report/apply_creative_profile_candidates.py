#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, unquote, urlparse


APPLY_APPROVAL = "approved-production-xiaoman-creative-profile-candidates"
PROFILE_KIND = "creative_profile"
PROFILE_VERSION = "xiaoman-daily-creative-profile-v1"
PRODUCTION_PSQL_BIN = "/usr/bin/psql"
PRODUCTION_PSQL_PATH = "/usr/bin:/bin"
MAX_INPUT_BYTES = 128 * 1024
UUID_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)
SAFE_TEXT_RE = re.compile(r"^[^`$<>{}\x00-\x08\x0b\x0c\x0e-\x1f\x7f]{1,240}$")
SAFE_ANCHOR_RE = re.compile(r"^daily_character_note:[A-Za-z0-9_\-\u4e00-\u9fff]{1,64}$")


class ApplyError(ValueError):
    pass


@dataclass(frozen=True)
class ReviewedCandidate:
    candidate_key: str
    person_id: str
    role_label: str
    story_function: str
    daily_arc: str
    memory_weight_label: str
    meme_seed: str
    callback_hint: str
    evidence_anchor: str
    recurrence_evidence_count: int
    profile_upgrade_reason: str


def _database_url() -> str | None:
    return (
        os.environ.get("QINTOPIA_MESSAGE_STORE_DATABASE_URL")
        or os.environ.get("QINTOPIA_SIDECAR_DATABASE_URL")
    )


def _reject_duplicate_json_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ApplyError("payload contains a duplicate key")
        result[key] = value
    return result


def _load_payload(path: str | None) -> dict[str, Any]:
    if path:
        data = Path(path).read_bytes()
    else:
        data = sys.stdin.buffer.read(MAX_INPUT_BYTES + 1)
    if not data or len(data) > MAX_INPUT_BYTES:
        raise ApplyError("payload length is invalid")
    try:
        value = json.loads(data, object_pairs_hook=_reject_duplicate_json_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ApplyError("payload is not valid JSON") from exc
    if not isinstance(value, dict):
        raise ApplyError("payload must be one JSON object")
    return value


def _safe_text(value: Any, key: str, *, required: bool = True) -> str:
    if value is None and not required:
        return ""
    if not isinstance(value, str):
        raise ApplyError(f"{key} must be a string")
    value = value.strip()
    if not value and required:
        raise ApplyError(f"{key} is required")
    if value and not SAFE_TEXT_RE.fullmatch(value):
        raise ApplyError(f"{key} contains unsupported content")
    lowered = value.lower()
    if any(marker in lowered for marker in ("raw_message", "fact_text", "profile_text", "database_url")):
        raise ApplyError(f"{key} contains forbidden raw/private marker")
    return value


def _safe_int(value: Any, key: str) -> int:
    if not isinstance(value, int):
        raise ApplyError(f"{key} must be an integer")
    if value < 0 or value > 1000:
        raise ApplyError(f"{key} is out of range")
    return value


def _validate_candidate(item: Any, index: int) -> ReviewedCandidate | None:
    if not isinstance(item, dict):
        raise ApplyError(f"candidate {index} must be an object")
    allowed = {
        "candidate_key",
        "review_decision",
        "person_id",
        "candidate_role_label",
        "story_function",
        "daily_arc",
        "memory_weight_label",
        "meme_seed",
        "callback_hint",
        "evidence_anchor",
        "recurrence_evidence_count",
        "minimum_recurrence_met",
        "profile_upgrade_status",
        "profile_upgrade_reason",
        "evidence_policy",
        "public_surface_allowed",
    }
    if set(item) - allowed:
        raise ApplyError(f"candidate {index} contains unsupported fields")
    decision = _safe_text(item.get("review_decision"), f"candidate {index} review_decision")
    if decision not in {"approved", "rejected"}:
        raise ApplyError(f"candidate {index} review_decision is invalid")
    if decision == "rejected":
        return None
    person_id = _safe_text(item.get("person_id"), f"candidate {index} person_id").lower()
    if not UUID_RE.fullmatch(person_id):
        raise ApplyError(f"candidate {index} person_id must be a reviewed UUID")
    if item.get("profile_upgrade_status") != "eligible_for_review":
        raise ApplyError(f"candidate {index} is not eligible_for_review")
    if item.get("minimum_recurrence_met") is not True:
        raise ApplyError(f"candidate {index} minimum recurrence is not met")
    if item.get("public_surface_allowed") is not False:
        raise ApplyError(f"candidate {index} public surface must remain false")
    evidence_count = _safe_int(
        item.get("recurrence_evidence_count"),
        f"candidate {index} recurrence_evidence_count",
    )
    if evidence_count < 2:
        raise ApplyError(f"candidate {index} recurrence evidence is too weak")
    anchor = _safe_text(item.get("evidence_anchor"), f"candidate {index} evidence_anchor")
    if not SAFE_ANCHOR_RE.fullmatch(anchor):
        raise ApplyError(f"candidate {index} evidence_anchor is invalid")
    if item.get("evidence_policy") != "daily_character_note_or_quote_map":
        raise ApplyError(f"candidate {index} evidence policy is invalid")
    return ReviewedCandidate(
        candidate_key=_safe_text(item.get("candidate_key"), f"candidate {index} candidate_key"),
        person_id=person_id,
        role_label=_safe_text(item.get("candidate_role_label"), f"candidate {index} role"),
        story_function=_safe_text(item.get("story_function"), f"candidate {index} story_function"),
        daily_arc=_safe_text(item.get("daily_arc"), f"candidate {index} daily_arc"),
        memory_weight_label=_safe_text(
            item.get("memory_weight_label"),
            f"candidate {index} memory_weight_label",
            required=False,
        ),
        meme_seed=_safe_text(item.get("meme_seed"), f"candidate {index} meme_seed", required=False),
        callback_hint=_safe_text(
            item.get("callback_hint"),
            f"candidate {index} callback_hint",
            required=False,
        ),
        evidence_anchor=anchor,
        recurrence_evidence_count=evidence_count,
        profile_upgrade_reason=_safe_text(
            item.get("profile_upgrade_reason"),
            f"candidate {index} profile_upgrade_reason",
        ),
    )


def _validate_payload(value: dict[str, Any]) -> list[ReviewedCandidate]:
    allowed = {
        "schema_version",
        "source",
        "character_universe_schema_version",
        "reviewed_by",
        "reviewed_at",
        "candidates",
    }
    if set(value) - allowed:
        raise ApplyError("payload contains unsupported fields")
    if value.get("schema_version") != 1:
        raise ApplyError("payload schema_version must be 1")
    if value.get("source") != "xiaoman-daily-creative-profile-review-v1":
        raise ApplyError("payload source is invalid")
    if value.get("character_universe_schema_version") != "xiaoman-character-universe-v1":
        raise ApplyError("character universe schema is invalid")
    _safe_text(value.get("reviewed_by"), "reviewed_by")
    _safe_text(value.get("reviewed_at"), "reviewed_at")
    raw_candidates = value.get("candidates")
    if not isinstance(raw_candidates, list) or not raw_candidates:
        raise ApplyError("candidates must be a non-empty array")
    candidates: list[ReviewedCandidate] = []
    seen: set[tuple[str, str]] = set()
    for index, raw in enumerate(raw_candidates, start=1):
        candidate = _validate_candidate(raw, index)
        if candidate is None:
            continue
        key = (candidate.person_id, candidate.candidate_key)
        if key in seen:
            raise ApplyError("payload contains duplicate approved candidate")
        seen.add(key)
        candidates.append(candidate)
    if not candidates:
        raise ApplyError("payload has no approved candidates to apply")
    return candidates


def _summary(candidate: ReviewedCandidate) -> str:
    parts = [
        f"小满日报审核通过的 creative_profile：{candidate.role_label}",
        f"故事功能：{candidate.story_function}",
        f"日线：{candidate.daily_arc}",
        f"复现证据：{candidate.recurrence_evidence_count}",
    ]
    if candidate.meme_seed:
        parts.append(f"回调种子：{candidate.meme_seed}")
    return "；".join(parts)[:600]


def _profile_payload(candidate: ReviewedCandidate) -> dict[str, Any]:
    return {
        "role_label": candidate.role_label,
        "story_function": candidate.story_function,
        "daily_arc": candidate.daily_arc,
        "memory_weight_label": candidate.memory_weight_label,
        "meme_seed": candidate.meme_seed,
        "callback_hint": candidate.callback_hint,
        "evidence_anchor": candidate.evidence_anchor,
        "recurrence_evidence_count": candidate.recurrence_evidence_count,
        "profile_upgrade_reason": candidate.profile_upgrade_reason,
        "public_surface_allowed": False,
    }


def _input_hash(candidate: ReviewedCandidate) -> str:
    encoded = json.dumps(_profile_payload(candidate), ensure_ascii=False, sort_keys=True).encode(
        "utf-8"
    )
    return hashlib.sha256(encoded).hexdigest()


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
        raise ApplyError("database URL shape is not supported")
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


def _apply_with_psql(candidates: list[ReviewedCandidate], db_url: str) -> None:
    payload_json = json.dumps([
        {
            "person_id": candidate.person_id,
            "summary": _summary(candidate),
            "communication_style": {
                "profile_track": "public_safe_character_universe",
                "role_label": candidate.role_label,
                "story_function": candidate.story_function,
            },
            "safe_reply_hints": _profile_payload(candidate),
            "do_not_disclose": {
                "raw_messages": True,
                "hidden_profile_details": True,
                "member_facts_fact_text": True,
                "public_surface_allowed": False,
            },
            "input_hash": _input_hash(candidate),
        }
        for candidate in candidates
    ], ensure_ascii=False, separators=(",", ":"))
    payload_literal = "'" + payload_json.replace("\\", "\\\\").replace("'", "''") + "'"
    sql = r"""
        WITH payload AS (
            SELECT *
            FROM jsonb_to_recordset((:payload_json)::jsonb) AS item(
                person_id uuid,
                summary text,
                communication_style jsonb,
                safe_reply_hints jsonb,
                do_not_disclose jsonb,
                input_hash text
            )
        ),
        superseded AS (
            UPDATE qintopia_identity.member_profile_snapshots snapshots
            SET status = 'superseded'
            FROM payload
            WHERE snapshots.person_id = payload.person_id
              AND snapshots.profile_kind = 'creative_profile'
              AND snapshots.status = 'active'
            RETURNING snapshots.id
        )
        INSERT INTO qintopia_identity.member_profile_snapshots
            (
                person_id,
                profile_kind,
                profile_version,
                status,
                summary,
                communication_style,
                safe_reply_hints,
                do_not_disclose,
                information_class,
                confidence,
                generated_by,
                input_hash,
                reviewed_at
            )
        SELECT
            person_id,
            'creative_profile',
            :'profile_version',
            'active',
            summary,
            communication_style,
            safe_reply_hints,
            do_not_disclose,
            'Internal',
            0.74,
            'xiaoman-daily-creative-profile-review-v1',
            input_hash,
            now()
        FROM payload;
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
        f"profile_version={PROFILE_VERSION}",
    ]
    completed = subprocess.run(
        command,
        input=f"\\set payload_json {payload_literal}\n{sql}",
        env=_psql_env(db_url),
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )
    if completed.returncode != 0:
        raise ApplyError("creative profile candidate apply failed")


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Apply reviewed Xiaoman creative profiles")
    parser.add_argument("--payload-json", help="Path to reviewed payload JSON. Omit to read stdin.")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval", default="")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    try:
        candidates = _validate_payload(_load_payload(args.payload_json))
        if args.apply:
            if args.approval != APPLY_APPROVAL:
                raise ApplyError("exact owner approval is required for apply")
            db_url = _database_url()
            if not db_url:
                raise ApplyError("database URL is not configured")
            _apply_with_psql(candidates, db_url)
        report = {
            "success": True,
            "schema_version": "xiaoman-creative-profile-apply-report-v1",
            "apply_executed": args.apply,
            "profile_kind": PROFILE_KIND,
            "profile_version": PROFILE_VERSION,
            "approved_candidate_count": len(candidates),
            "public_surface_allowed": False,
            "raw_messages_included": False,
            "profile_fact_text_included": False,
            "person_ids_included": False,
        }
        print(json.dumps(report, ensure_ascii=False, sort_keys=True))
        return 0
    except ApplyError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
