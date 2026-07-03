from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Dict, List
from zoneinfo import ZoneInfo

from .parser import ActivityRecord, stable_activity_body
from .reminder_policy import ReminderPolicy


@dataclass
class ActivityUpsertResult:
    activity: ActivityRecord
    added_participants: List[str] = field(default_factory=list)
    removed_participants: List[str] = field(default_factory=list)
    previous: Dict[str, Any] = field(default_factory=dict)


@dataclass
class ReminderJob:
    job_id: str
    activity_id: str
    reminder_type: str
    group_id: str
    due_at: str
    start_time: str
    source_message_ref: Dict[str, Any] = field(default_factory=dict)
    sent: bool = False
    status: str = "pending"
    send_result: Dict[str, Any] = field(default_factory=dict)


class ActivityRepository:
    def __init__(self, state_dir: str, reminder_policy: ReminderPolicy | None = None):
        self.state_dir = Path(state_dir).expanduser() if state_dir else None
        self.reminder_policy = reminder_policy or ReminderPolicy.from_env()
        self._memory_activities: Dict[str, Dict[str, Any]] = {}
        self._memory_record_ids: Dict[str, str] = {}
        self._memory_reminders: Dict[str, Dict[str, Any]] = {}

    def upsert_activity(self, activity: ActivityRecord) -> ActivityUpsertResult:
        activities = self._load_json("activities.json", default={})
        if not isinstance(activities, dict):
            activities = {}
        activity = self._merge_activity_identity(activity, activities)
        previous = activities.get(activity.activity_id, {}) if isinstance(activities, dict) else {}
        if isinstance(previous, dict) and previous.get("first_seen_at"):
            activity.first_seen_at = str(previous.get("first_seen_at") or activity.first_seen_at)
        if isinstance(previous, dict) and previous.get("solitaire_created_at"):
            activity.solitaire_created_at = str(previous.get("solitaire_created_at") or activity.solitaire_created_at)
        old_names = set(previous.get("participant_names", [])) if isinstance(previous, dict) else set()
        new_names = set(activity.participant_names)
        activities[activity.activity_id] = activity.to_internal_fields()
        self._save_json("activities.json", activities)
        return ActivityUpsertResult(
            activity=activity,
            added_participants=sorted(new_names - old_names),
            removed_participants=sorted(old_names - new_names),
            previous=previous if isinstance(previous, dict) else {},
        )

    def _merge_activity_identity(self, activity: ActivityRecord, activities: Dict[str, Any]) -> ActivityRecord:
        if activity.activity_id in activities:
            return activity
        merged = self._merge_by_stable_identity(activity, activities)
        if merged is not None:
            return merged
        if not _is_time_only(activity.start_time):
            return activity
        subject_key = _compact(activity.activity_subject)
        group_id = str(activity.source_group_id or "")
        for activity_id, previous in activities.items():
            if not isinstance(previous, dict):
                continue
            if str(previous.get("source_group_id") or "") != group_id:
                continue
            if _compact(str(previous.get("activity_subject") or "")) != subject_key:
                continue
            previous_start = str(previous.get("start_time") or "")
            if not previous_start or _is_time_only(previous_start):
                continue
            if not _same_planned_occurrence(activity.start_time, previous_start):
                continue
            activity.activity_id = str(activity_id)
            activity.start_time = previous_start
            return activity
        return activity

    def _merge_by_stable_identity(self, activity: ActivityRecord, activities: Dict[str, Any]) -> ActivityRecord | None:
        group_id = str(activity.source_group_id or "")
        fingerprint = _compact(getattr(activity, "stable_body_fingerprint", ""))
        identity = _compact(getattr(activity, "activity_identity", ""))
        if not group_id or not (fingerprint or identity):
            return None
        for activity_id, previous in activities.items():
            if not isinstance(previous, dict):
                continue
            if str(previous.get("source_group_id") or "") != group_id:
                continue
            previous_fingerprint = _compact(str(previous.get("stable_body_fingerprint") or ""))
            if not previous_fingerprint:
                previous_fingerprint = _hash_compact(stable_activity_body(str(previous.get("raw_summary") or "")))
            previous_identity = _compact(str(previous.get("activity_identity") or ""))
            same_solitaire = _same_solitaire_thread(activity, previous)
            if not same_solitaire and not _same_planned_occurrence(activity.start_time, str(previous.get("start_time") or "")):
                continue
            if fingerprint and previous_fingerprint and fingerprint == previous_fingerprint:
                activity.activity_id = str(activity_id)
                self._preserve_specific_start_time(activity, previous, force=same_solitaire)
                return activity
            if identity and previous_identity and identity == previous_identity:
                activity.activity_id = str(activity_id)
                self._preserve_specific_start_time(activity, previous, force=same_solitaire)
                return activity
        return None

    def _preserve_specific_start_time(self, activity: ActivityRecord, previous: Dict[str, Any], *, force: bool = False) -> None:
        previous_start = str(previous.get("start_time") or "")
        if previous_start and (force or _is_time_only(activity.start_time)) and not _is_time_only(previous_start):
            activity.start_time = previous_start

    def get_activity(self, activity_id: str) -> Dict[str, Any]:
        activities = self._load_json("activities.json", default={})
        if not isinstance(activities, dict):
            return {}
        activity = activities.get(activity_id, {})
        return dict(activity) if isinstance(activity, dict) else {}

    def record_message(self, activity: ActivityRecord, event: Any) -> None:
        self._append_jsonl(
            "messages.jsonl",
            {
                "activity_id": activity.activity_id,
                "event_id": getattr(event, "event_id", ""),
                "group_id": getattr(event, "group_id", ""),
                "sender_id": getattr(event, "sender_id", ""),
                "message_kind": getattr(event, "message_kind", ""),
                "created_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def record_parse_attempt(
        self,
        event: Any,
        *,
        handled: bool,
        reason: str,
        activity_id: str = "",
        diagnostic: Dict[str, Any] | None = None,
    ) -> None:
        payload = {
            "activity_id": activity_id,
            "created_at": datetime.now(timezone.utc).isoformat(),
            "event_id": getattr(event, "event_id", ""),
            "group_id": getattr(event, "group_id", ""),
            "handled": bool(handled),
            "message_kind": getattr(event, "message_kind", ""),
            "reason": str(reason or ""),
        }
        if isinstance(diagnostic, dict):
            diagnostic_reason = str(diagnostic.get("reason") or "")
            if diagnostic_reason:
                payload["diagnostic_reason"] = diagnostic_reason
            for source_key, target_key in (
                ("response_preview", "diagnostic_preview"),
                ("error", "diagnostic_error"),
            ):
                value = str(diagnostic.get(source_key) or "")
                if value:
                    payload[target_key] = value
        self._append_jsonl("parse_attempts.jsonl", payload)

    def enqueue_feishu_sync(self, activity: ActivityRecord) -> None:
        self._append_jsonl(
            "feishu_sync_jobs.jsonl",
            {
                "activity_id": activity.activity_id,
                "source_message_id": activity.source_message_id,
                "status": "queued",
                "created_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def mark_feishu_sync_attempt(self, activity_id: str, result: Any) -> None:
        self._append_jsonl(
            "feishu_sync_jobs.jsonl",
            {
                "activity_id": activity_id,
                "status": "succeeded" if bool(getattr(result, "success", False)) else "failed",
                "mode": getattr(result, "mode", ""),
                "record_id": getattr(result, "record_id", ""),
                "error": getattr(result, "error", ""),
                "attempted_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def enqueue_feishu_retry(self, activity_id: str, source_message_id: str = "", error: str = "") -> None:
        self._append_jsonl(
            "feishu_retry.jsonl",
            {
                "activity_id": activity_id,
                "source_message_id": source_message_id,
                "error": error,
                "created_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def record_feishu_result(self, activity: ActivityRecord | str, result: Any) -> None:
        activity_id = activity.activity_id if isinstance(activity, ActivityRecord) else str(activity or "")
        self._append_jsonl(
            "feishu_writes.jsonl",
            {
                "activity_id": activity_id,
                "success": bool(getattr(result, "success", False)),
                "mode": getattr(result, "mode", ""),
                "skipped": bool(getattr(result, "skipped", False)),
                "record_id": getattr(result, "record_id", ""),
                "error": getattr(result, "error", ""),
                "created_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def get_feishu_record_id(self, activity_id: str) -> str:
        records = self._load_json("feishu_record_ids.json", default={})
        return str(records.get(activity_id, "") if isinstance(records, dict) else "")

    def set_feishu_record_id(self, activity_id: str, record_id: str) -> None:
        if not activity_id or not record_id:
            return
        records = self._load_json("feishu_record_ids.json", default={})
        if not isinstance(records, dict):
            records = {}
        records[activity_id] = record_id
        self._save_json("feishu_record_ids.json", records)

    def upsert_reminders(self, activity: ActivityRecord) -> List[Dict[str, Any]]:
        reminders = self._load_json("reminders.json", default={})
        if not isinstance(reminders, dict):
            reminders = {}
        jobs = self._build_reminder_jobs(activity)
        job_ids = {str(job.get("job_id") or "") for job in jobs}
        for existing_id, existing in list(reminders.items()):
            if not isinstance(existing, dict):
                continue
            if str(existing.get("activity_id") or "") != activity.activity_id:
                continue
            if existing_id in job_ids or bool(existing.get("sent", False)):
                continue
            reminders.pop(existing_id, None)
        for job in jobs:
            existing = reminders.get(job["job_id"])
            if isinstance(existing, dict):
                if not bool(existing.get("sent", False)) and self._reminder_status(existing) in {"pending", "pending_retry", "sending"}:
                    existing.update(
                        {
                            "group_id": job["group_id"],
                            "due_at": job["due_at"],
                            "start_time": job["start_time"],
                            "activity_type": job["activity_type"],
                            "source_message_ref": dict(job.get("source_message_ref") or {}),
                        }
                    )
                    reminders[job["job_id"]] = existing
                continue
            reminders[job["job_id"]] = job
        self._save_json("reminders.json", reminders)
        return jobs

    def due_reminders(self, now: datetime) -> List[ReminderJob]:
        now_utc = now.astimezone(timezone.utc) if now.tzinfo else now.replace(tzinfo=timezone.utc)
        reminders = self._load_json("reminders.json", default={})
        if not isinstance(reminders, dict):
            return []
        due: List[ReminderJob] = []
        for payload in reminders.values():
            if not isinstance(payload, dict):
                continue
            status = self._reminder_status(payload)
            if status in {"sent", "failed"}:
                continue
            if status == "sending" and not self._reminder_sending_timed_out(payload, now_utc):
                continue
            due_at = self._parse_iso_datetime(str(payload.get("due_at") or ""))
            if due_at is None or due_at > now_utc:
                continue
            activity = self.get_activity(str(payload.get("activity_id") or ""))
            if not activity:
                continue
            if str(activity.get("status") or "active") != "active":
                continue
            if str(activity.get("start_time") or "") != str(payload.get("start_time") or ""):
                continue
            start_at = self._parse_start_time(str(activity.get("start_time") or ""))
            if start_at is None or start_at <= now_utc:
                continue
            due.append(
                ReminderJob(
                    job_id=str(payload.get("job_id") or ""),
                    activity_id=str(payload.get("activity_id") or ""),
                    reminder_type=str(payload.get("reminder_type") or ""),
                    group_id=str(payload.get("group_id") or ""),
                    due_at=str(payload.get("due_at") or ""),
                    start_time=str(payload.get("start_time") or ""),
                    source_message_ref=dict(payload.get("source_message_ref") or {}) if isinstance(payload.get("source_message_ref"), dict) else {},
                    sent=bool(payload.get("sent", False)),
                    status=status,
                    send_result=dict(payload.get("send_result") or {}) if isinstance(payload.get("send_result"), dict) else {},
                )
            )
        return due

    def mark_reminder_sending(self, job_id: str, *, now: datetime | None = None) -> bool:
        if not job_id:
            return False
        reminders = self._load_json("reminders.json", default={})
        if not isinstance(reminders, dict):
            return False
        job = reminders.get(job_id)
        if not isinstance(job, dict):
            return False
        current = (now or datetime.now(timezone.utc)).astimezone(timezone.utc)
        status = self._reminder_status(job)
        if status == "sent" or (status == "sending" and not self._reminder_sending_timed_out(job, current)):
            return False
        job["sent"] = False
        job["status"] = "sending"
        job["sending_at"] = current.isoformat()
        job["last_attempt_at"] = current.isoformat()
        job["attempt_count"] = int(job.get("attempt_count") or 0) + 1
        job.pop("error", None)
        job.pop("retryable", None)
        reminders[job_id] = job
        self._save_json("reminders.json", reminders)
        return True

    def mark_reminder_sent(self, job_id: str, result: Dict[str, Any]) -> None:
        if not job_id:
            return
        reminders = self._load_json("reminders.json", default={})
        if not isinstance(reminders, dict):
            return
        job = reminders.get(job_id)
        if not isinstance(job, dict):
            return
        job["sent"] = True
        job["status"] = "sent"
        job["sent_at"] = datetime.now(timezone.utc).isoformat()
        job["send_result"] = dict(result or {})
        reminders[job_id] = job
        self._save_json("reminders.json", reminders)

    def mark_reminder_failed(self, job_id: str, result: Dict[str, Any]) -> None:
        if not job_id:
            return
        reminders = self._load_json("reminders.json", default={})
        if not isinstance(reminders, dict):
            return
        job = reminders.get(job_id)
        if not isinstance(job, dict):
            return
        retryable = bool((result or {}).get("retryable", False))
        job["sent"] = False
        job["status"] = "pending_retry" if retryable else "failed"
        job["last_attempt_at"] = datetime.now(timezone.utc).isoformat()
        job["error"] = str((result or {}).get("error") or "")
        job["retryable"] = retryable
        job["send_result"] = dict(result or {})
        reminders[job_id] = job
        self._save_json("reminders.json", reminders)

    def record_reminder_attempt(self, job: ReminderJob, result: Dict[str, Any]) -> None:
        self._append_jsonl(
            "reminder_sends.jsonl",
            {
                "job_id": job.job_id,
                "activity_id": job.activity_id,
                "reminder_type": job.reminder_type,
                "group_id": job.group_id,
                "result": result,
                "created_at": datetime.now(timezone.utc).isoformat(),
            },
        )

    def _build_reminder_jobs(self, activity: ActivityRecord) -> List[Dict[str, Any]]:
        start_at = self._parse_start_time(activity.start_time)
        if start_at is None:
            return []
        first_seen_at = None
        if _has_explicit_time(activity.start_time):
            first_seen_at = self._parse_iso_datetime(getattr(activity, "first_seen_at", "") or getattr(activity, "last_seen_at", ""))
        jobs = []
        for offset in self.reminder_policy.offsets_for(activity.activity_type):
            due_at = start_at - offset.delta
            if first_seen_at is not None and due_at <= first_seen_at:
                continue
            job_id = f"{activity.activity_id}:{_reminder_start_slug(activity.start_time)}:{offset.label}"
            jobs.append(
                {
                    "job_id": job_id,
                    "activity_id": activity.activity_id,
                    "reminder_type": offset.label,
                    "group_id": activity.source_group_id,
                    "due_at": due_at.astimezone(timezone.utc).isoformat(),
                    "start_time": activity.start_time,
                    "activity_type": activity.activity_type,
                    "source_message_ref": dict(activity.source_message_ref),
                    "sent": False,
                    "status": "pending",
                    "created_at": datetime.now(timezone.utc).isoformat(),
                }
            )
        return jobs

    def _parse_start_time(self, value: str) -> datetime | None:
        text = str(value or "").strip()
        if not text:
            return None
        for fmt in ("%Y-%m-%d %H:%M", "%Y-%m-%d"):
            try:
                parsed = datetime.strptime(text, fmt)
                zone = ZoneInfo(os.getenv("QIWE_ACTIVITY_TIMEZONE", "Asia/Shanghai"))
                return parsed.replace(tzinfo=zone).astimezone(timezone.utc)
            except ValueError:
                continue
        return None

    def _reminder_status(self, job: Dict[str, Any]) -> str:
        status = str(job.get("status") or "").strip()
        if status:
            return status
        return "sent" if bool(job.get("sent", False)) else "pending"

    def _reminder_sending_timed_out(self, job: Dict[str, Any], now: datetime) -> bool:
        sending_at = self._parse_iso_datetime(str(job.get("sending_at") or ""))
        if sending_at is None:
            return True
        return now - sending_at >= timedelta(minutes=5)

    def _parse_iso_datetime(self, value: str) -> datetime | None:
        text = str(value or "").strip()
        if not text:
            return None
        try:
            parsed = datetime.fromisoformat(text)
        except ValueError:
            return None
        return parsed.astimezone(timezone.utc) if parsed.tzinfo else parsed.replace(tzinfo=timezone.utc)

    def _path(self, name: str) -> Path | None:
        if self.state_dir is None:
            return None
        return self.state_dir / "solitaire" / name

    def _load_json(self, name: str, *, default: Any) -> Any:
        path = self._path(name)
        if path is None:
            if name == "activities.json":
                return dict(self._memory_activities)
            if name == "feishu_record_ids.json":
                return dict(self._memory_record_ids)
            if name == "reminders.json":
                return dict(self._memory_reminders)
            return default
        try:
            if not path.exists():
                return default
            return json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            return default

    def _save_json(self, name: str, value: Any) -> None:
        path = self._path(name)
        if path is None:
            if name == "activities.json":
                self._memory_activities = dict(value)
            elif name == "feishu_record_ids.json":
                self._memory_record_ids = dict(value)
            elif name == "reminders.json":
                self._memory_reminders = dict(value)
            return
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = path.with_suffix(path.suffix + f".{os.getpid()}.tmp")
        tmp_path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True), encoding="utf-8")
        tmp_path.replace(path)

    def _append_jsonl(self, name: str, value: Dict[str, Any]) -> None:
        path = self._path(name)
        if path is None:
            return
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(value, ensure_ascii=False, sort_keys=True) + "\n")


def _compact(value: str) -> str:
    return "".join(str(value or "").split()).lower()


def _hash_compact(value: str) -> str:
    import hashlib

    compact = _compact(value)
    if not compact:
        return ""
    return hashlib.sha256(compact.encode("utf-8")).hexdigest()[:20]


def _is_time_only(value: str) -> bool:
    text = str(value or "").strip()
    if not text:
        return False
    for fmt in ("%H:%M", "%H:%M:%S"):
        try:
            datetime.strptime(text, fmt)
            return True
        except ValueError:
            continue
    return False


def _same_planned_occurrence(current: str, previous: str) -> bool:
    current_key = _planned_occurrence_key(current)
    previous_key = _planned_occurrence_key(previous)
    if current_key and previous_key:
        return current_key == previous_key
    return True


def _same_solitaire_thread(activity: ActivityRecord, previous: Dict[str, Any]) -> bool:
    current_ref = activity.source_message_ref if isinstance(activity.source_message_ref, dict) else {}
    previous_ref = previous.get("source_message_ref") if isinstance(previous.get("source_message_ref"), dict) else {}
    current_author = _compact(str(current_ref.get("solitaireAuthorId") or ""))
    previous_author = _compact(str(previous_ref.get("solitaireAuthorId") or ""))
    if not current_author or not previous_author or current_author != previous_author:
        return False
    current_created = _compact(getattr(activity, "solitaire_created_at", ""))
    previous_created = _compact(str(previous.get("solitaire_created_at") or ""))
    if not current_created or not previous_created or current_created != previous_created:
        return False
    current_fingerprint = _compact(getattr(activity, "stable_body_fingerprint", ""))
    previous_fingerprint = _compact(str(previous.get("stable_body_fingerprint") or ""))
    if not previous_fingerprint:
        previous_fingerprint = _hash_compact(stable_activity_body(str(previous.get("raw_summary") or "")))
    if current_fingerprint and previous_fingerprint and current_fingerprint == previous_fingerprint:
        return True
    current_identity = _compact(getattr(activity, "activity_identity", ""))
    previous_identity = _compact(str(previous.get("activity_identity") or ""))
    return bool(current_identity and previous_identity and current_identity == previous_identity)


def _planned_occurrence_key(value: str) -> str:
    text = str(value or "").strip()
    if not text or _is_time_only(text):
        return ""
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%d", "%Y/%m/%d %H:%M:%S", "%Y/%m/%d %H:%M", "%Y/%m/%d"):
        try:
            return datetime.strptime(text, fmt).strftime("%Y-%m-%d")
        except ValueError:
            continue
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return ""
    return parsed.strftime("%Y-%m-%d")


def _reminder_start_slug(value: str) -> str:
    text = str(value or "").strip()
    if not text:
        return "unscheduled"
    for fmt, output in (
        ("%Y-%m-%d %H:%M:%S", "%Y%m%dT%H%M%S"),
        ("%Y-%m-%d %H:%M", "%Y%m%dT%H%M"),
        ("%Y-%m-%d", "%Y%m%d"),
        ("%Y/%m/%d %H:%M:%S", "%Y%m%dT%H%M%S"),
        ("%Y/%m/%d %H:%M", "%Y%m%dT%H%M"),
        ("%Y/%m/%d", "%Y%m%d"),
    ):
        try:
            return datetime.strptime(text, fmt).strftime(output)
        except ValueError:
            continue
    compact = re.sub(r"[^0-9A-Za-z]+", "", text)
    return compact[:32] or "unscheduled"


def _has_explicit_time(value: str) -> bool:
    return bool(re.search(r"\d{1,2}:\d{2}", str(value or "")))
