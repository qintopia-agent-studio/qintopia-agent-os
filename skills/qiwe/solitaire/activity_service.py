from __future__ import annotations

import asyncio
import logging
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, List

from .feishu_writer import FeishuActivityWriter, FeishuWriteResult
from .llm_parser import SolitaireContentParser
from .parser import ActivityRecord, parse_activity_record
from .repository import ActivityRepository, ActivityUpsertResult, ReminderJob

logger = logging.getLogger(__name__)


@dataclass
class ActivityServiceResult:
    handled: bool
    activity_id: str = ""
    is_new_activity: bool = False
    participant_count: int = 0
    activity_subject: str = ""
    start_time: str = ""
    time_normalization_note: str = ""
    participant_names: List[str] | None = None
    immediate_reminder: bool = False
    feishu_success: bool = False
    error: str = ""


class ActivityService:
    def __init__(self, repository: ActivityRepository, writer: FeishuActivityWriter, content_parser: SolitaireContentParser | None = None):
        self.repository = repository
        self.writer = writer
        self.content_parser = content_parser

    def upsert_activity(self, activity: ActivityRecord) -> ActivityUpsertResult:
        upsert = self.repository.upsert_activity(activity)
        self.repository.upsert_reminders(activity)
        self.repository.enqueue_feishu_sync(activity)
        return upsert

    async def upsert_from_solitaire(self, event: Any) -> ActivityServiceResult:
        activity = await parse_activity_record(event, self.content_parser)
        if activity is None:
            diagnostic = getattr(self.content_parser, "last_diagnostic", None) if self.content_parser is not None else None
            self.repository.record_parse_attempt(event, handled=False, reason="not_activity_or_invalid_parse", diagnostic=diagnostic)
            return ActivityServiceResult(handled=False)

        upsert = self.upsert_activity(activity)
        diagnostic = getattr(self.content_parser, "last_diagnostic", None) if self.content_parser is not None else None
        self.repository.record_parse_attempt(event, handled=True, reason="activity_parsed", activity_id=activity.activity_id, diagnostic=diagnostic)
        self.repository.record_message(activity, event)
        self.schedule_feishu_sync(activity.activity_id)

        logger.info(
            "[qiwe] solitaire activity handled activity_id=%s participants=%s added=%s removed=%s feishu_sync=queued",
            activity.activity_id,
            activity.participant_count,
            upsert.added_participants,
            upsert.removed_participants,
        )
        return ActivityServiceResult(
            handled=True,
            activity_id=activity.activity_id,
            is_new_activity=not bool(upsert.previous),
            participant_count=activity.participant_count,
            activity_subject=activity.activity_subject,
            start_time=activity.start_time,
            time_normalization_note=getattr(activity, "time_normalization_note", ""),
            participant_names=list(activity.participant_names),
            immediate_reminder=self._needs_immediate_reminder(activity, upsert),
        )

    def _needs_immediate_reminder(self, activity: ActivityRecord, upsert: ActivityUpsertResult) -> bool:
        if upsert.previous:
            return False
        start_at = self.repository._parse_start_time(activity.start_time)
        if start_at is None:
            return False
        seen_at = self.repository._parse_iso_datetime(getattr(activity, "first_seen_at", "") or getattr(activity, "last_seen_at", ""))
        current = seen_at or datetime.now(timezone.utc)
        return current < start_at and not self.repository._build_reminder_jobs(activity)

    def schedule_feishu_sync(self, activity_id: str) -> None:
        try:
            loop = asyncio.get_running_loop()
        except RuntimeError:
            self.sync_feishu(activity_id)
            return

        task = loop.create_task(asyncio.to_thread(self.sync_feishu, activity_id))
        task.add_done_callback(lambda done: self._log_feishu_sync_task(activity_id, done))

    def sync_feishu(self, activity_id: str) -> FeishuWriteResult:
        activity = self.repository.get_activity(activity_id)
        if not activity:
            result = FeishuWriteResult(success=False, mode=self.writer.mapping.mode, error="activity not found", retryable=False)
            self.repository.record_feishu_result(activity_id, result)
            return result

        record_id = self.repository.get_feishu_record_id(activity_id)
        result = self.writer.write(activity, record_id=record_id)
        self.repository.record_feishu_result(activity_id, result)
        self.repository.mark_feishu_sync_attempt(activity_id, result)
        if result.record_id:
            self.repository.set_feishu_record_id(activity_id, result.record_id)
        if not result.success and result.retryable:
            self.repository.enqueue_feishu_retry(activity_id, activity.get("source_message_id", ""), result.error)
        return result

    def _log_feishu_sync_task(self, activity_id: str, task: asyncio.Task) -> None:
        if task.cancelled():
            logger.debug("[qiwe] Feishu sync task cancelled activity_id=%s", activity_id)
            return
        try:
            result = task.result()
        except Exception as exc:
            logger.warning("[qiwe] Feishu sync task failed activity_id=%s error=%s", activity_id, exc, exc_info=True)
            return
        logger.info(
            "[qiwe] Feishu sync finished activity_id=%s success=%s mode=%s record_id=%s",
            activity_id,
            bool(getattr(result, "success", False)),
            getattr(result, "mode", ""),
            getattr(result, "record_id", ""),
        )

    def due_reminders(self, now: datetime) -> List[ReminderJob]:
        return self.repository.due_reminders(now)

    def mark_reminder_sent(self, job_id: str, result: dict) -> None:
        self.repository.mark_reminder_sent(job_id, result)
