from __future__ import annotations

import json
import logging
import os
import re
from dataclasses import dataclass, field
from datetime import timedelta
from pathlib import Path
from typing import Any, Dict, List

logger = logging.getLogger(__name__)

FEISHU_ACTIVITY_TYPES = [
    "知识分享类",
    "疗愈冥想类",
    "户外运动🏃‍♀️",
    "自然探索",
    "传统节日类",
    "桌游棋牌类",
    "书音影分享",
    "手工创意类",
    "美食研究",
    "自我探索",
    "运动娱乐",
]


@dataclass(frozen=True)
class ReminderOffset:
    label: str
    delta: timedelta


@dataclass
class ReminderPolicy:
    default: List[str] = field(default_factory=lambda: ["30m"])
    by_activity_type: Dict[str, List[str]] = field(default_factory=dict)

    @classmethod
    def from_env(cls) -> "ReminderPolicy":
        path = os.getenv("QIWE_ACTIVITY_REMINDER_POLICY", "").strip()
        if not path:
            return cls()
        return cls.load(path)

    @classmethod
    def load(cls, path: str) -> "ReminderPolicy":
        payload = json.loads(Path(path).expanduser().read_text(encoding="utf-8"))
        if not isinstance(payload, dict):
            raise ValueError("Reminder policy must be a JSON object")
        default = _string_list(payload.get("default")) or ["30m"]
        by_type_payload = payload.get("byActivityType") if isinstance(payload.get("byActivityType"), dict) else {}
        by_activity_type = {str(key): _string_list(value) for key, value in by_type_payload.items()}
        return cls(default=default, by_activity_type=by_activity_type)

    def offsets_for(self, activity_type: str) -> List[ReminderOffset]:
        key = str(activity_type or "").strip()
        raw_offsets = self.by_activity_type.get(key)
        if raw_offsets is None:
            if key:
                logger.warning("[qiwe] reminder policy missing activity_type=%s; using default", key)
            raw_offsets = self.default
        offsets = []
        for raw in raw_offsets:
            parsed = parse_offset(raw)
            if parsed is not None:
                offsets.append(parsed)
        return offsets

    def missing_activity_types(self, activity_types: List[str] | None = None) -> List[str]:
        expected = activity_types or FEISHU_ACTIVITY_TYPES
        return [item for item in expected if item not in self.by_activity_type]


_OFFSET_RE = re.compile(r"^\s*(\d+)\s*([mhd])\s*$", re.IGNORECASE)


def parse_offset(value: Any) -> ReminderOffset | None:
    text = str(value or "").strip().lower()
    match = _OFFSET_RE.match(text)
    if not match:
        logger.warning("[qiwe] invalid reminder offset skipped: %s", value)
        return None
    amount = int(match.group(1))
    unit = match.group(2)
    if amount <= 0:
        logger.warning("[qiwe] non-positive reminder offset skipped: %s", value)
        return None
    if unit == "m":
        delta = timedelta(minutes=amount)
    elif unit == "h":
        delta = timedelta(hours=amount)
    else:
        delta = timedelta(days=amount)
    return ReminderOffset(label=f"before_{amount}{unit}", delta=delta)


def _string_list(value: Any) -> List[str]:
    if isinstance(value, list):
        return [str(item).strip() for item in value if str(item).strip()]
    if isinstance(value, str) and value.strip():
        return [value.strip()]
    return []
