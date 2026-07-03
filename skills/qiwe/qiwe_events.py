from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional


@dataclass
class NormalizedMessageEvent:
    event_id: str
    group_id: str = ""
    sender_id: str = ""
    sender_name: str = ""
    timestamp: Optional[datetime] = None
    conversation_type: str = "group"
    is_mentioned: bool = False
    message_kind: str = "text"
    text: str = ""
    attachments: List[Dict[str, Any]] = field(default_factory=list)
    quoted_message: Dict[str, Any] = field(default_factory=dict)
    raw_protocol_type: str = ""
    raw_event_ref: Dict[str, Any] = field(default_factory=dict)
    payload_ref: Dict[str, Any] = field(default_factory=dict)

    @property
    def chat_id(self) -> str:
        return self.group_id if self.conversation_type == "group" else self.sender_id


def normalized_event_from_parsed(parsed: Any) -> NormalizedMessageEvent:
    raw_event = getattr(parsed, "raw_event", {}) if isinstance(getattr(parsed, "raw_event", {}), dict) else {}
    payload = getattr(parsed, "payload", {}) if isinstance(getattr(parsed, "payload", {}), dict) else {}
    protocol_type = str(raw_event.get("newMsgType") or payload.get("commonMsgType") or raw_event.get("msgType") or "").strip()
    quoted = {}
    msg_data = raw_event.get("msgData") if isinstance(raw_event.get("msgData"), dict) else {}
    if isinstance(msg_data.get("reply"), dict):
        quoted = dict(msg_data.get("reply") or {})
    return NormalizedMessageEvent(
        event_id=str(getattr(parsed, "message_id", "") or ""),
        group_id=str(getattr(parsed, "group_id", "") or ""),
        sender_id=str(getattr(parsed, "sender_id", "") or ""),
        sender_name=str(getattr(parsed, "sender_name", "") or ""),
        timestamp=getattr(parsed, "timestamp", None),
        conversation_type=str(getattr(parsed, "conversation_type", "") or "group"),
        is_mentioned=bool(getattr(parsed, "is_mentioned", False)),
        message_kind=str(getattr(parsed, "message_kind", "") or "unsupported"),
        text=str(getattr(parsed, "text", "") or ""),
        attachments=list(getattr(parsed, "attachments", []) or []),
        quoted_message=quoted,
        raw_protocol_type=protocol_type,
        raw_event_ref=raw_event,
        payload_ref=payload,
    )
