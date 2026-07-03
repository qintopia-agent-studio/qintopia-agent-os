from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import Any

from .activity_service import ActivityService

logger = logging.getLogger(__name__)


@dataclass
class SolitaireProcessResult:
    handled: bool
    activity_id: str = ""
    is_new_activity: bool = False
    participant_count: int = 0
    activity_subject: str = ""
    start_time: str = ""
    time_normalization_note: str = ""
    participant_names: list[str] | None = None
    immediate_reminder: bool = False
    feishu_success: bool = False
    error: str = ""


class SolitaireActivityProcessor:
    def __init__(self, activity_service: ActivityService):
        self.activity_service = activity_service

    async def handle(self, event: Any) -> SolitaireProcessResult:
        result = await self.activity_service.upsert_from_solitaire(event)
        if not result.handled:
            return SolitaireProcessResult(handled=False)
        return SolitaireProcessResult(
            handled=True,
            activity_id=result.activity_id,
            is_new_activity=bool(getattr(result, "is_new_activity", False)),
            participant_count=result.participant_count,
            activity_subject=getattr(result, "activity_subject", ""),
            start_time=getattr(result, "start_time", ""),
            time_normalization_note=getattr(result, "time_normalization_note", ""),
            participant_names=list(getattr(result, "participant_names", []) or []),
            immediate_reminder=bool(getattr(result, "immediate_reminder", False)),
            feishu_success=result.feishu_success,
            error=result.error,
        )
