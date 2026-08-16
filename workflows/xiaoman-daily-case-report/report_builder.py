"""Deterministic report building for the Xiaoman daily case-report pipeline.

Owns the private review bundles (character universe, quote map, wiki bundle,
draft bundle, run manifest) plus the ReportData assembly.  Rendering lives in
renderer.py; the CLI entry point lives in daily_case_report.py.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import os
import sys
from datetime import datetime, time, timedelta
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

from models import (
    DEFAULT_MIN_CASE_MESSAGES,
    DEFAULT_SUSPECT_LIMIT,
    DEFAULT_WINDOW_HOURS,
    REVIEW_DRAFT_REVIEWED_BY,
    TEMPLATE_VERSION,
    CaseCard,
    CharacterCard,
    HotTopic,
    ReportData,
    ReportMessage,
    clean_text,
)
from collector import (
    fetch_character_memory,
    fetch_creative_profile_memory,
    fetch_messages,
    load_fixture,
    require_read_through,
)
from analyzer import (
    case_storyline_label,
    cluster_cases,
    compute_characters,
    compute_suspects,
    discussion_messages,
    extract_highlight,
    hourly_timeline,
    hot_topics,
    node_key,
    profile_upgrade_reason,
    profile_upgrade_status,
)



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


def _source_chat_ref(chat_id: str | None) -> dict[str, str] | None:
    if not chat_id:
        return None
    digest = hashlib.sha256(chat_id.encode("utf-8")).hexdigest()
    return {"kind": "sha256", "value": f"sha256:{digest}"}


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


def _build_character_universe(
    cases: list[CaseCard],
    hot_topics: list[HotTopic],
    characters: list[CharacterCard],
    report_date: str,
) -> dict[str, Any]:
    def character_key(character: CharacterCard) -> str:
        return character.node_key or node_key(character.name)

    def character_anchor(character: CharacterCard) -> str:
        return character.evidence_anchor or f"daily_character_note:{character_key(character)}"

    def character_evidence_count(character: CharacterCard) -> int:
        return character.profile_evidence_count or (1 if character.message_count >= 2 else 0)

    def character_upgrade_status(character: CharacterCard) -> str:
        return character.profile_upgrade_status or profile_upgrade_status(
            character_evidence_count(character)
        )

    def character_upgrade_reason(character: CharacterCard) -> str:
        if character.profile_upgrade_reason:
            return character.profile_upgrade_reason
        return profile_upgrade_reason(
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
            "expressive_label": character.expressive_label,
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
            "key": node_key(topic.keyword),
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
            "key": node_key(case.title),
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
            "key": node_key(case.title),
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
        key = node_key(label)
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
        key = node_key(label)
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
            "key": node_key(f"{character.node_key}-{character.role_label}-callback"),
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
            "key": node_key(f"{character.node_key}-{character.role_label}-creative-profile"),
            "profile_kind": "creative_profile",
            "profile_version": "daily-character-v1",
            "related_person": character_key(character),
            "candidate_role_label": character.role_label,
            "story_function": character.story_function,
            "daily_arc": character.arc_label,
            "memory_weight_label": character.memory_weight_label,
            "meme_seed": character.meme_seed,
            "callback_hint": character.callback_hint,
            "expressive_label": character.expressive_label,
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
                "key": node_key("-".join(relation_key)),
                "source": source,
                "target": target,
                "relation": "co_discusses_topic",
                "label": character.relationship_hint,
                "topic": topic,
                "risk": "public_safe_summary",
            }
        )
    creative_meme_candidates = [
        {
            "type": "creative_meme_candidate",
            "key": node_key(f"creative-meme-{item.get('key', '')}"),
            "label": item.get("label", ""),
            "related_people": item.get("related_people", []),
            "source": item.get("source", ""),
            "lookback_days": [7, 14, 30],
            "evidence_policy": "daily_meme_candidate_or_quote_map",
            "status": "pending_review",
            "public_surface_allowed": False,
            "risk": "internal_review_required",
        }
        for item in memes[:8]
    ]
    creative_relationship_candidates = [
        {
            "type": "creative_relationship_candidate",
            "key": node_key(f"creative-relationship-{item.get('key', '')}"),
            "source": item.get("source", ""),
            "target": item.get("target", ""),
            "topic": item.get("topic", ""),
            "candidate_label": item.get("label", ""),
            "evidence_policy": "same_topic_co_presence_only",
            "status": "pending_review",
            "public_surface_allowed": False,
            "risk": "internal_review_required",
        }
        for item in relationships[:8]
    ]
    creative_timeline_candidates = [
        {
            "type": "creative_timeline_candidate",
            "key": node_key(f"creative-timeline-{item.get('key', '')}"),
            "label": item.get("label", ""),
            "last_seen": item.get("last_seen", report_date),
            "related_event": item.get("related_event", ""),
            "lookback_days": [7, 14, 30],
            "candidate_arc": f"连续观察「{item.get('label', '')}」是否复现",
            "evidence_policy": "daily_storyline_or_wiki_timeline",
            "status": "pending_review",
            "public_surface_allowed": False,
            "risk": "internal_review_required",
        }
        for item in storyline_candidates[:8]
        if item.get("label")
    ]
    creative_universe_candidate_sets = {
        "cross_day_memes": creative_meme_candidates,
        "relationship_labels": creative_relationship_candidates,
        "timeline_threads": creative_timeline_candidates,
    }
    creative_universe_candidate_count = sum(
        len(items) for items in creative_universe_candidate_sets.values()
    )
    expressive_label_candidates = [
        {
            "type": "expressive_label_candidate",
            "key": node_key(f"expressive-{character_key(character)}-{character.role_label}"),
            "related_person": character_key(character),
            "candidate_label": (
                character.expressive_label
                or character.meme_seed
                or character.relationship_hint
                or character.callback_hint
            ),
            "label_kind": "reviewed_public"
            if character.expressive_label
            else "draft_requires_owner_review",
            "review_status": "reviewed" if character.expressive_label else "candidate",
            "public_surface_allowed": bool(character.expressive_label),
            "evidence_anchor": character_anchor(character),
            "risk": "field_level_owner_review_required",
        }
        for character in characters
        if character.role_label
        and (
            character.expressive_label
            or character.meme_seed
            or character.relationship_hint
            or character.callback_hint
        )
    ]
    edges: list[dict[str, Any]] = []
    for character in characters:
        character_key_value = character_key(character)
        for case in cases:
            if character.name == case.top_speaker or character.name in " ".join(case.bullets):
                edges.append(
                    {
                        "source": character_key_value,
                        "target": node_key(case.title),
                        "relation": "appears_in",
                        "evidence": case.case_no,
                    }
                )
        for topic in hot_topics:
            if topic.keyword in character.evidence:
                edges.append(
                    {
                        "source": character_key_value,
                        "target": node_key(topic.keyword),
                        "relation": "mentions_topic",
                        "evidence": "daily_character_note",
                    }
                )
        if character.meme_seed:
            edges.append(
                {
                    "source": character_key_value,
                    "target": node_key(character.meme_seed),
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
        "expressive_label_candidates": expressive_label_candidates,
        "creative_profile_candidates": creative_profile_candidates,
        "creative_universe_candidates": {
            "schema_version": "xiaoman-daily-creative-universe-candidates-v1",
            "source": "daily_case_report_second_pass",
            "apply_mode": "candidate_only",
            "public_surface_allowed": False,
            "review_required": True,
            "raw_messages_included": False,
            "profile_fact_text_included": False,
            "writes_member_profile_snapshots": False,
            "candidate_count": creative_universe_candidate_count,
            "candidate_sets": creative_universe_candidate_sets,
        },
        "creative_profile_candidate_policy": {
            "profile_kind": "creative_profile",
            "apply_mode": "candidate_only",
            "writes_member_profile_snapshots": False,
            "public_surface_allowed": False,
            "evidence_policy": "daily_character_note_or_quote_map",
            "review_required": True,
        },
        "expressive_label_policy": {
            "apply_mode": "candidate_only",
            "public_surface_allowed_requires_owner_review": True,
            "public_render_requires_reviewed_safe_reply_hints": True,
            "writes_member_profile_snapshots": False,
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
    cleaned = clean_text(excerpt)
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
                related_topics=[node_key(topic.keyword) for topic in report.hot_topics[:2]],
            )
        )
        next_index += 1

    for character in report.characters:
        person_key = character.node_key or node_key(character.name)
        add(
            _quote_entry(
                next_index,
                "daily_character_note",
                character.evidence,
                speaker_label=character.name,
                speaker_key=person_key,
                related_memes=[node_key(character.meme_seed)] if character.meme_seed else [],
                source_anchor=character.evidence_anchor,
            )
        )
        next_index += 1

    for case in report.cases:
        event_key = node_key(case.title)
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
            key = node_key(f"{character.node_key or character.name}-{seed}-{days}d")
            if key in seen:
                continue
            seen.add(key)
            callbacks.append(
                {
                    "key": key,
                    "lookback_days": days,
                    "label": f"{character.name}的「{seed}」{days}天回看候选",
                    "related_person": character.node_key or node_key(character.name),
                    "trigger": character.callback_hint or character.arc_label,
                    "status": "candidate",
                    "risk": "internal_review_required",
                }
            )
            if len(callbacks) >= 9:
                return callbacks
    for case in report.cases:
        label = case_storyline_label(case)
        if not label:
            continue
        key = node_key(f"{label}-7d-lookback")
        if key in seen:
            continue
        seen.add(key)
        callbacks.append(
            {
                "key": key,
                "lookback_days": 7,
                "label": f"「{label}」7天回看候选",
                "related_event": node_key(case.title),
                "trigger": case.summary,
                "status": "candidate",
                "risk": "internal_review_required",
            }
        )
    return callbacks


def _ordinary_digest_topic_cards(report: ReportData) -> list[dict[str, Any]]:
    cards: list[dict[str, Any]] = []
    for case in report.cases:
        cards.append(
            {
                "title": case_storyline_label(case),
                "participants": case.participant_count,
                "message_count": case.message_count,
                "summary": case.summary,
                "anchors": case.bullets[:3],
                "message_ids": [],
                "attachment_pointers": [],
                "media_links": [],
                "media_notes": {
                    "status": "omitted_no_reviewed_attachment_source",
                    "raw_message_payload_read": False,
                },
                "top_speaker": case.top_speaker,
                "status": "candidate",
            }
        )
    if cards:
        return cards[:6]
    for topic in report.hot_topics:
        cards.append(
            {
                "title": topic.keyword,
                "participants": topic.participant_count,
                "message_count": topic.message_count,
                "summary": f"{topic.message_count} 条消息，{topic.participant_count} 人参与",
                "anchors": [],
                "message_ids": [],
                "attachment_pointers": [],
                "media_links": [],
                "media_notes": {
                    "status": "omitted_no_reviewed_attachment_source",
                    "raw_message_payload_read": False,
                },
                "top_speaker": "",
                "status": "candidate",
            }
        )
    return cards[:6]


def _ordinary_digest_people_notes(report: ReportData) -> list[dict[str, Any]]:
    return [
        {
            "person_key": character.node_key or node_key(character.name),
            "display_label": character.name,
            "role_label": character.role_label,
            "story_function": character.story_function,
            "daily_arc": character.arc_label,
            "evidence_anchor": character.evidence_anchor,
            "quote": character.evidence,
            "memory_weight_label": character.memory_weight_label,
            "status": "candidate",
        }
        for character in report.characters
    ]


def _ordinary_digest_open_questions(report: ReportData) -> list[str]:
    questions: list[str] = []
    seen: set[str] = set()
    for case in report.cases:
        for bullet in case.bullets:
            text = clean_text(bullet)
            if "?" in text or "？" in text or any(
                word in text for word in ("求助", "请问", "有没有", "怎么")
            ):
                question = text[:120]
                if question and question not in seen:
                    seen.add(question)
                    questions.append(question)
            if len(questions) >= 5:
                return questions
    for character in report.characters:
        if character.role_label == "问题发射台":
            question = character.evidence[:120]
            if question and question not in seen:
                seen.add(question)
                questions.append(question)
        if len(questions) >= 5:
            break
    return questions


def _ordinary_digest_local_life_notes(report: ReportData) -> list[dict[str, str]]:
    local_life_hints = (
        "活动",
        "饭局",
        "聚餐",
        "茶",
        "酒",
        "咖啡",
        "店",
        "地点",
        "场地",
        "本地",
        "社区",
        "市集",
        "报名",
        "接龙",
        "天气",
        "路线",
        "交通",
    )
    notes: list[dict[str, str]] = []
    seen: set[str] = set()
    for case in report.cases:
        candidate_texts = [case_storyline_label(case), *case.bullets[:3]]
        for text in candidate_texts:
            cleaned = clean_text(text)
            if not cleaned or not any(hint in cleaned for hint in local_life_hints):
                continue
            label = cleaned[:80]
            if label in seen:
                continue
            seen.add(label)
            notes.append(
                {
                    "label": label,
                    "source": case.case_no,
                    "status": "candidate",
                }
            )
            if len(notes) >= 5:
                return notes
    return notes


def _ordinary_digest_candidate_topics(report: ReportData) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    main_storyline = _main_storyline_label(report)
    if report.case_count > 0:
        candidates.append(
            {
                "title": f"{main_storyline}的一天",
                "source": "daily_storyline",
                "reason": "当天已有可归档主线和 quote-map 候选证据",
                "review_required": True,
            }
        )
    for callback in _meme_callback_candidates(report, limit=3):
        label = callback.split("：", 1)[0].strip("「」")
        if label:
            candidates.append(
                {
                    "title": f"围绕{label}的群聊回看",
                    "source": "meme_callback_candidate",
                    "reason": "梗或回调候选需要人工判断是否适合公开文章",
                    "review_required": True,
                }
            )
    return candidates[:5]


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
    local_life_notes = _ordinary_digest_local_life_notes(report)
    open_questions = _ordinary_digest_open_questions(report)
    character_cards = [
        {
            "person_key": character.node_key or node_key(character.name),
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
    ordinary_topic_cards = _ordinary_digest_topic_cards(report)
    ordinary_people_notes = _ordinary_digest_people_notes(report)
    ordinary_local_life_notes = _ordinary_digest_local_life_notes(report)
    ordinary_open_questions = _ordinary_digest_open_questions(report)
    ordinary_candidate_topics = _ordinary_digest_candidate_topics(report)
    storyline_timeline = [
        {
            "date": report.report_date,
            "case_no": case.case_no,
            "storyline": case_storyline_label(case),
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
            "weather_context": {
                "status": "omitted_no_reviewed_weather_source",
                "public_surface_allowed": False,
            },
            "one_sentence_summary": _daily_opening_line(report),
            "main_topics": ordinary_topic_cards,
            "people_notes": ordinary_people_notes,
            "local_life_notes": ordinary_local_life_notes,
            "open_questions": ordinary_open_questions,
            "risk_items": [
                "所有直接引用必须回溯到 quote-map 后才能公开使用",
                "人物动态只作为今日出场，不自动升级为长期画像",
                "公众号候选文发布前必须人工审核隐私和人物边界",
            ],
            "candidate_public_topics": ordinary_candidate_topics,
            "section_keys": [
                "天气背景",
                "今日一句话",
                "主要话题",
                "人物动态",
                "地点/本地生活线索",
                "待解决问题",
                "不可公开/需人工复核素材",
                "候选公众号选题",
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
        "ordinary_digest_topic_count": len(ordinary_topic_cards),
        "ordinary_digest_people_note_count": len(ordinary_people_notes),
        "ordinary_digest_local_life_note_count": len(ordinary_local_life_notes),
        "ordinary_digest_open_question_count": len(ordinary_open_questions),
        "ordinary_digest_candidate_public_topic_count": len(ordinary_candidate_topics),
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
            "key": node_key(f"{report.report_date}-{case.case_no}"),
            "date": report.report_date,
            "case_no": case.case_no,
            "label": case_storyline_label(case),
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
    creative_universe_candidates = universe.get("creative_universe_candidates") or {}
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
        "reference_workshop_steps": {
            "attachment_index": "omitted_no_reviewed_attachment_source",
            "media_prepare": "omitted_no_reviewed_attachment_source",
            "media_notes": "omitted_no_reviewed_attachment_source",
            "media_link_check": "omitted_no_reviewed_attachment_source",
            "weather_context": "omitted_no_reviewed_weather_source",
            "history_profiles": "reviewed_creative_profiles_or_member_fact_counts_only",
            "traceability": "quote_map_and_private_manifest_only",
            "raw_message_payload_read": False,
            "attachment_public_surface_allowed": False,
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
            "expressive_label_candidate_count": len(
                universe.get("expressive_label_candidates") or []
            ),
            "reviewed_public_expressive_label_count": sum(
                1
                for item in universe.get("expressive_label_candidates") or []
                if item.get("public_surface_allowed") is True
                and item.get("review_status") == "reviewed"
            ),
            "creative_universe_candidate_count": int(
                creative_universe_candidates.get("candidate_count") or 0
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
            "creative_universe_public_surface_allowed": (
                creative_universe_candidates.get("public_surface_allowed") is True
            ),
            "unreviewed_expressive_labels_public_surface_allowed": any(
                item.get("public_surface_allowed") is True
                and item.get("review_status") != "reviewed"
                for item in universe.get("expressive_label_candidates") or []
            ),
            "writes_member_profile_snapshots": False,
            "raw_message_payload_read": False,
            "attachment_public_surface_allowed": False,
        },
        "review_required": True,
    }


def _public_output_style_contract() -> dict[str, Any]:
    return {
        "schema_version": "xiaoman-daily-public-output-style-v1",
        "source": "wx_cli_style_daily_migration",
        "character_daily_layout": True,
        "storyline_first": True,
        "cast_notes_enabled": True,
        "meme_callback_section_enabled": True,
        "relationship_section_enabled": True,
        "owner_reviewed_expressive_labels_only": True,
        "image_first_delivery": True,
        "pdf_default_delivery": False,
        "roast_review_boundary": True,
        "private_draft_only": True,
        "public_surface_contains_private_draft": False,
    }


def _character_universe_summary(universe: dict[str, Any]) -> dict[str, Any]:
    creative_universe_candidates = universe.get("creative_universe_candidates") or {}
    expressive_label_candidates = universe.get("expressive_label_candidates") or []
    return {
        "schema_version": universe.get("schema_version", ""),
        "source": universe.get("source", ""),
        "retained_source_policy": universe.get("retained_source_policy", ""),
        "raw_messages_included": universe.get("raw_messages_included") is True,
        "profile_fact_text_included": universe.get("profile_fact_text_included") is True,
        "people_count": len(universe.get("people") or []),
        "topic_count": len(universe.get("topics") or []),
        "event_count": len(universe.get("events") or []),
        "meme_count": len(universe.get("memes") or []),
        "callback_count": len(universe.get("callbacks") or []),
        "relationship_count": len(universe.get("relationships") or []),
        "expressive_label_candidate_count": len(expressive_label_candidates),
        "reviewed_public_expressive_label_count": sum(
            1
            for item in expressive_label_candidates
            if item.get("public_surface_allowed") is True
            and item.get("review_status") == "reviewed"
        ),
        "creative_profile_candidate_count": len(
            universe.get("creative_profile_candidates") or []
        ),
        "creative_profile_public_surface_allowed": (
            (universe.get("creative_profile_candidate_policy") or {}).get(
                "public_surface_allowed"
            )
            is True
        ),
        "creative_universe_candidate_count": int(
            creative_universe_candidates.get("candidate_count") or 0
        ),
        "creative_universe_public_surface_allowed": (
            creative_universe_candidates.get("public_surface_allowed") is True
        ),
        "unreviewed_expressive_labels_public_surface_allowed": any(
            item.get("public_surface_allowed") is True
            and item.get("review_status") != "reviewed"
            for item in expressive_label_candidates
        ),
        "storyline_candidate_count": len(universe.get("storyline_candidates") or []),
        "edge_count": len(universe.get("edges") or []),
    }


def _summary_result_json(result: dict[str, Any]) -> dict[str, Any]:
    allowed_keys = {
        "success",
        "skill",
        "external_send_executed",
        "requires_human_confirmation",
        "auto_publish_ready",
        "group_name",
        "report_date",
        "time_range",
        "message_count",
        "participant_count",
        "case_count",
        "character_count",
        "suspect_count",
        "deliverable_path",
        "image_path",
        "image_format",
        "image_mime_type",
        "png_path",
        "html_path",
        "daily_report_markdown_path",
        "character_universe_path",
        "quote_map_path",
        "wiki_bundle_path",
        "draft_bundle_path",
        "run_manifest_path",
        "review_report_path",
        "creative_profile_review_payload_path",
        "public_output_style",
        "character_universe_summary",
        "private_review_bundle",
        "artifact_candidate",
    }
    return {key: value for key, value in result.items() if key in allowed_keys}


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
    creative_universe_candidates = universe.get("creative_universe_candidates") or {}
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
        f"- 创作资产候选：{creative_universe_candidates.get('candidate_count', 0)} 条（梗 / 关系标签 / 时间线，仅供审核）",
        f"- 已审核公开表达标签：{run_manifest['counts']['reviewed_public_expressive_label_count']} 条",
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
        "- [ ] 附件/图片素材步骤是否仍为 omitted，未读取 raw payload 或猜测图片内容",
        "",
        "## 隐私边界",
        "",
        f"- raw_message_rows_included={str(run_manifest['privacy']['raw_message_rows_included']).lower()}",
        f"- profile_fact_text_included={str(run_manifest['privacy']['profile_fact_text_included']).lower()}",
        f"- creative_profile_public_surface_allowed={str(run_manifest['privacy']['creative_profile_public_surface_allowed']).lower()}",
        f"- creative_universe_public_surface_allowed={str(run_manifest['privacy']['creative_universe_public_surface_allowed']).lower()}",
        f"- unreviewed_expressive_labels_public_surface_allowed={str(run_manifest['privacy']['unreviewed_expressive_labels_public_surface_allowed']).lower()}",
        f"- writes_member_profile_snapshots={str(run_manifest['privacy']['writes_member_profile_snapshots']).lower()}",
        f"- raw_message_payload_read={str(run_manifest['privacy']['raw_message_payload_read']).lower()}",
        f"- attachment_public_surface_allowed={str(run_manifest['privacy']['attachment_public_surface_allowed']).lower()}",
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


def _main_storyline_label(report: ReportData) -> str:
    lead = case_storyline_label(report.cases[0]) if report.cases else ""
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
        label = case_storyline_label(case)
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


def _build_report(args: argparse.Namespace) -> ReportData:
    start, end, display_date = _report_date(args)
    report_zone = _report_timezone(args.timezone)

    if args.fixture:
        messages = load_fixture(args.fixture)
    elif args.dry_run:
        messages = _sample_messages(start)
    else:
        if not require_read_through():
            raise RuntimeError(
                "database read-through is disabled; set "
                "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1 or use --fixture/--dry-run"
            )
        messages = fetch_messages(args.chat_id, start, end)
    messages = _normalize_message_times(messages, report_zone)

    if not messages and not args.dry_run and not args.fixture:
        # Empty day is a normal result, not an error.
        pass

    filtered_messages = discussion_messages(messages)
    unique_senders = {m.sender_id for m in filtered_messages}
    cases = cluster_cases(filtered_messages)
    hot_topic_list = hot_topics(filtered_messages, cases)
    suspects = compute_suspects(filtered_messages)
    character_memory = {}
    creative_profile_memory = {}
    if not args.dry_run and not args.fixture:
        person_ids = {message.person_id for message in filtered_messages if message.person_id}
        try:
            character_memory = fetch_character_memory(person_ids, end)
        except Exception:
            character_memory = {}
        try:
            creative_profile_memory = fetch_creative_profile_memory(person_ids, end)
        except Exception:
            creative_profile_memory = {}
    characters = compute_characters(filtered_messages, character_memory, creative_profile_memory)
    character_universe = _build_character_universe(cases, hot_topic_list, characters, display_date)
    hourly = hourly_timeline(messages, start)
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
        highlight=extract_highlight(filtered_messages),
        hot_topics=hot_topic_list,
        character_count=len(characters),
        characters=characters,
        character_universe=character_universe,
        window_start=start.isoformat(),
        window_end=end.isoformat(),
        timezone=args.timezone,
        messages=messages,
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
