from __future__ import annotations

import asyncio
import logging
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Awaitable, Callable, Dict, Iterable, List

from .activity_service import ActivityService
from .repository import ReminderJob

logger = logging.getLogger(__name__)

SendReminder = Callable[..., Awaitable[Any]]


@dataclass
class ReminderWorkerConfig:
    enabled: bool = False
    dry_run: bool = True
    scan_interval_seconds: int = 60
    allowed_groups: List[str] = field(default_factory=list)


@dataclass
class ReminderRunResult:
    scanned: int = 0
    sent: int = 0
    failed: int = 0
    skipped: int = 0


class ReminderWorker:
    def __init__(self, config: ReminderWorkerConfig, activity_service: ActivityService, send_func: SendReminder):
        self.config = config
        self.activity_service = activity_service
        self.send_func = send_func
        self._task: asyncio.Task | None = None
        self._stopping = asyncio.Event()

    @property
    def enabled(self) -> bool:
        return self.config.enabled

    def start(self) -> None:
        if not self.enabled or self._task is not None:
            return
        self._stopping.clear()
        self._task = asyncio.create_task(self._run_loop())

    async def stop(self) -> None:
        if self._task is None:
            return
        self._stopping.set()
        self._task.cancel()
        try:
            await self._task
        except asyncio.CancelledError:
            pass
        self._task = None

    async def run_once(self, now: datetime | None = None) -> ReminderRunResult:
        if not self.enabled:
            return ReminderRunResult()
        current = now or datetime.now(timezone.utc)
        jobs = self.activity_service.due_reminders(current)
        result = ReminderRunResult(scanned=len(jobs))
        for job in jobs:
            send_result = await self._handle_job(job)
            if send_result.get("skipped"):
                result.skipped += 1
                continue
            if send_result.get("success") is False:
                result.failed += 1
                continue
            result.sent += 1
        return result

    async def _run_loop(self) -> None:
        while not self._stopping.is_set():
            try:
                await self.run_once()
            except Exception as exc:
                logger.warning("[qiwe] reminder worker scan failed: %s", exc, exc_info=True)
            try:
                await asyncio.wait_for(self._stopping.wait(), timeout=max(1, self.config.scan_interval_seconds))
            except asyncio.TimeoutError:
                continue

    async def _handle_job(self, job: ReminderJob) -> Dict[str, Any]:
        activity = self.activity_service.repository.get_activity(job.activity_id)
        if not activity:
            return self._record_skipped(job, "activity_not_found")
        if self.config.allowed_groups and job.group_id not in set(self.config.allowed_groups):
            return self._record_skipped(job, "group_not_allowed")

        text = render_reminder_text(activity)
        if self.config.dry_run:
            payload = {
                "dry_run": True,
                "group_id": job.group_id,
                "text": text,
                "job_id": job.job_id,
                "reminder_type": job.reminder_type,
            }
            self.activity_service.repository.record_reminder_attempt(job, payload)
            self.activity_service.mark_reminder_sent(job.job_id, payload)
            return payload

        if not self.activity_service.repository.mark_reminder_sending(job.job_id):
            return self._record_skipped(job, "not_claimed")
        send_result = await self._send(job, text)
        payload = {
            "dry_run": False,
            "success": bool(getattr(send_result, "success", False)),
            "message_id": getattr(send_result, "message_id", "") or "",
            "error": getattr(send_result, "error", "") or "",
            "retryable": bool(getattr(send_result, "retryable", False)),
        }
        self.activity_service.repository.record_reminder_attempt(job, payload)
        if payload["success"]:
            self.activity_service.mark_reminder_sent(job.job_id, payload)
        else:
            self.activity_service.repository.mark_reminder_failed(job.job_id, payload)
        return payload

    async def _send(self, job: ReminderJob, text: str) -> Any:
        try:
            return await self.send_func(job.group_id, text, source_message_ref=job.source_message_ref)
        except TypeError:
            return await self.send_func(job.group_id, text)

    def _record_skipped(self, job: ReminderJob, reason: str) -> Dict[str, Any]:
        payload = {"skipped": True, "reason": reason, "job_id": job.job_id}
        self.activity_service.repository.record_reminder_attempt(job, payload)
        return payload


def render_reminder_text(activity: Dict[str, Any]) -> str:
    names = _participant_names(activity.get("participant_names"))
    parts = [
        f"活动提醒：{activity.get('activity_subject') or '未命名活动'}",
        f"时间：{activity.get('start_time') or '未识别'}",
        f"地点/详情：{activity.get('activity_detail') or '未提供'}",
        f"当前报名：{activity.get('participant_count') or 0} 人",
        f"参与人：{names or '暂无'}",
    ]
    return "\n".join(parts)


def _participant_names(value: Any) -> str:
    if isinstance(value, str):
        return value
    if isinstance(value, Iterable):
        return "、".join(str(item) for item in value if str(item).strip())
    return ""
