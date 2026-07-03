from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

try:
    from .solitaire.activity_service import ActivityService
    from .solitaire.feishu_writer import FeishuActivityWriter
    from .solitaire.llm_parser import SolitaireContentParser
    from .solitaire.processor import SolitaireActivityProcessor
    from .solitaire.repository import ActivityRepository
except ImportError:  # pragma: no cover - local tests import modules directly
    from solitaire.activity_service import ActivityService
    from solitaire.feishu_writer import FeishuActivityWriter
    from solitaire.llm_parser import SolitaireContentParser
    from solitaire.processor import SolitaireActivityProcessor
    from solitaire.repository import ActivityRepository

logger = logging.getLogger(__name__)


@dataclass
class PassivePipelineConfig:
    enabled: bool = False
    passive_enabled: bool = False
    solitaire_enabled: bool = False
    state_dir: str = ""
    allowed_groups: list[str] = field(default_factory=list)


class PassiveEventPipeline:
    def __init__(self, config: PassivePipelineConfig, content_parser: SolitaireContentParser | None = None):
        self.config = config
        repository = ActivityRepository(config.state_dir)
        writer = FeishuActivityWriter.from_env()
        self.activity_service = ActivityService(repository, writer, content_parser)
        self._solitaire = SolitaireActivityProcessor(self.activity_service)

    @property
    def enabled(self) -> bool:
        return self.config.enabled and self.config.passive_enabled

    async def handle(self, event: Any) -> Any:
        if not self.enabled:
            return None
        if self.config.allowed_groups and str(getattr(event, "group_id", "")) not in set(self.config.allowed_groups):
            logger.debug(
                "[qiwe] passive pipeline skipped group_id=%s event_id=%s",
                getattr(event, "group_id", ""),
                getattr(event, "event_id", ""),
            )
            return None
        if event.message_kind == "solitaire" and self.config.solitaire_enabled:
            return await self._solitaire.handle(event)
        logger.debug("[qiwe] passive pipeline skipped kind=%s event_id=%s", event.message_kind, event.event_id)
        return None
