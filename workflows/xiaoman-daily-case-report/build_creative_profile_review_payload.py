#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


PAYLOAD_SOURCE = "xiaoman-daily-creative-profile-review-v1"
CHARACTER_UNIVERSE_SCHEMA = "xiaoman-character-universe-v1"
MAX_INPUT_BYTES = 256 * 1024
SAFE_TEXT_RE = re.compile(r"^[^`$<>{}\x00-\x08\x0b\x0c\x0e-\x1f\x7f]{0,240}$")


class PayloadBuildError(ValueError):
    pass


@dataclass(frozen=True)
class ReviewDraft:
    candidate_key: str
    review_decision: str
    person_id: str
    candidate_role_label: str
    story_function: str
    daily_arc: str
    memory_weight_label: str
    meme_seed: str
    callback_hint: str
    evidence_anchor: str
    recurrence_evidence_count: int
    minimum_recurrence_met: bool
    profile_upgrade_status: str
    profile_upgrade_reason: str
    evidence_policy: str
    public_surface_allowed: bool


def _load_json_file(path: str) -> dict[str, Any]:
    try:
        data = Path(path).read_bytes()
    except OSError as exc:
        raise PayloadBuildError("character universe file cannot be read") from exc
    if not data or len(data) > MAX_INPUT_BYTES:
        raise PayloadBuildError("character universe file length is invalid")
    try:
        value = json.loads(data)
    except json.JSONDecodeError as exc:
        raise PayloadBuildError("character universe file is not valid JSON") from exc
    if not isinstance(value, dict):
        raise PayloadBuildError("character universe must be one JSON object")
    return value


def _safe_text(value: Any, key: str, *, required: bool = True) -> str:
    if value is None:
        value = ""
    if not isinstance(value, str):
        raise PayloadBuildError(f"{key} must be a string")
    value = value.strip()
    if required and not value:
        raise PayloadBuildError(f"{key} is required")
    if not SAFE_TEXT_RE.fullmatch(value):
        raise PayloadBuildError(f"{key} contains unsupported content")
    lowered = value.lower()
    if any(marker in lowered for marker in ("raw_message", "fact_text", "profile_text", "database_url")):
        raise PayloadBuildError(f"{key} contains forbidden raw/private marker")
    return value


def _safe_int(value: Any, key: str) -> int:
    if isinstance(value, bool):
        raise PayloadBuildError(f"{key} must be an integer")
    try:
        parsed = int(value)
    except (TypeError, ValueError) as exc:
        raise PayloadBuildError(f"{key} must be an integer") from exc
    if parsed < 0 or parsed > 1000:
        raise PayloadBuildError(f"{key} is out of range")
    return parsed


def _candidate_key(item: dict[str, Any], index: int) -> str:
    raw_key = _safe_text(item.get("key") or "", f"candidate {index} key", required=False)
    if raw_key:
        return raw_key[:96]
    encoded = json.dumps(item, ensure_ascii=False, sort_keys=True).encode("utf-8")
    return f"candidate-{hashlib.sha256(encoded).hexdigest()[:16]}"


def _draft_from_candidate(item: Any, index: int, *, include_rejected: bool) -> ReviewDraft | None:
    if not isinstance(item, dict):
        raise PayloadBuildError(f"candidate {index} must be an object")
    if item.get("public_surface_allowed") is not False:
        raise PayloadBuildError(f"candidate {index} public surface must remain false")
    evidence_count = _safe_int(
        item.get("recurrence_evidence_count"),
        f"candidate {index} recurrence_evidence_count",
    )
    minimum_recurrence_met = item.get("minimum_recurrence_met") is True
    status = _safe_text(item.get("profile_upgrade_status"), f"candidate {index} status")
    review_decision = (
        "pending_review" if status == "eligible_for_review" and minimum_recurrence_met else "rejected"
    )
    if review_decision == "rejected" and not include_rejected:
        return None

    return ReviewDraft(
        candidate_key=_candidate_key(item, index),
        review_decision=review_decision,
        # Deliberately blank: the owner must bind a stable UUID after checking identity.
        person_id="",
        candidate_role_label=_safe_text(
            item.get("candidate_role_label"),
            f"candidate {index} role",
        ),
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
        evidence_anchor=_safe_text(item.get("evidence_anchor"), f"candidate {index} evidence_anchor"),
        recurrence_evidence_count=evidence_count,
        minimum_recurrence_met=minimum_recurrence_met,
        profile_upgrade_status=status,
        profile_upgrade_reason=_safe_text(
            item.get("profile_upgrade_reason"),
            f"candidate {index} profile_upgrade_reason",
            required=False,
        ),
        evidence_policy=_safe_text(item.get("evidence_policy"), f"candidate {index} evidence_policy"),
        public_surface_allowed=False,
    )


def _reviewed_at(value: str) -> str:
    if value:
        return _safe_text(value, "reviewed_at")
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def _build_payload(
    universe: dict[str, Any],
    *,
    reviewed_by: str,
    reviewed_at: str,
    include_rejected: bool,
) -> dict[str, Any]:
    if universe.get("schema_version") != CHARACTER_UNIVERSE_SCHEMA:
        raise PayloadBuildError("character universe schema is invalid")
    if universe.get("raw_messages_included") is not False:
        raise PayloadBuildError("character universe must not include raw messages")
    if universe.get("profile_fact_text_included") is not False:
        raise PayloadBuildError("character universe must not include profile fact text")
    policy = universe.get("creative_profile_candidate_policy") or {}
    if not isinstance(policy, dict):
        raise PayloadBuildError("creative profile candidate policy is invalid")
    if policy.get("public_surface_allowed") is not False:
        raise PayloadBuildError("creative profile candidates must not be public")
    candidates = universe.get("creative_profile_candidates")
    if not isinstance(candidates, list):
        raise PayloadBuildError("creative_profile_candidates must be an array")

    drafts: list[ReviewDraft] = []
    for index, item in enumerate(candidates, start=1):
        draft = _draft_from_candidate(item, index, include_rejected=include_rejected)
        if draft is not None:
            drafts.append(draft)
    if not drafts:
        raise PayloadBuildError("no eligible creative profile candidates found")

    return {
        "schema_version": 1,
        "source": PAYLOAD_SOURCE,
        "character_universe_schema_version": CHARACTER_UNIVERSE_SCHEMA,
        "reviewed_by": _safe_text(reviewed_by, "reviewed_by"),
        "reviewed_at": _reviewed_at(reviewed_at),
        "candidates": [draft.__dict__ for draft in drafts],
        "review_notes": {
            "person_id_required": True,
            "person_id_policy": "owner_reviewed_stable_uuid_only",
            "display_name_binding_allowed": False,
            "daily_note_only_default": "rejected",
            "eligible_for_review_default": "pending_review",
            "apply_requires_owner_approved": True,
            "public_surface_allowed": False,
            "raw_messages_included": False,
            "profile_fact_text_included": False,
        },
    }


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build a reviewed-payload draft from Xiaoman creative profile candidates"
    )
    parser.add_argument("--character-universe-json", required=True)
    parser.add_argument("--reviewed-by", required=True)
    parser.add_argument("--reviewed-at", default="")
    parser.add_argument("--include-rejected", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    try:
        payload = _build_payload(
            _load_json_file(args.character_universe_json),
            reviewed_by=args.reviewed_by,
            reviewed_at=args.reviewed_at,
            include_rejected=args.include_rejected,
        )
    except PayloadBuildError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
