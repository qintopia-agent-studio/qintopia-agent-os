from __future__ import annotations

import asyncio
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, Tuple
from urllib.parse import unquote, urlparse


DEFAULT_NATS_URL = "nats://127.0.0.1:4222"
DEFAULT_RAW_SUBJECT = "qintopia.qiwe.raw"
DEFAULT_MESSAGE_SUBJECT = "qintopia.qiwe.message"
DEFAULT_TIMEOUT_SECONDS = 0.5


@dataclass
class QiWeNatsCaptureConfig:
    enabled: bool = False
    url: str = DEFAULT_NATS_URL
    raw_subject: str = DEFAULT_RAW_SUBJECT
    message_subject: str = DEFAULT_MESSAGE_SUBJECT
    timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS


class QiWeNatsPublisher:
    def __init__(self, config: QiWeNatsCaptureConfig) -> None:
        self.config = config
        parsed = urlparse(config.url)
        if parsed.scheme not in {"nats", ""}:
            raise ValueError(f"unsupported NATS URL scheme: {parsed.scheme}")
        self.host = parsed.hostname or "127.0.0.1"
        self.port = parsed.port or 4222
        self.user = unquote(parsed.username) if parsed.username else ""
        self.password = unquote(parsed.password) if parsed.password else ""

    async def publish_capture(
        self,
        raw_event: Dict[str, Any],
        message_event: Dict[str, Any],
        *,
        message_id: str,
    ) -> None:
        await asyncio.gather(
            self.publish_json(
                self.config.raw_subject,
                raw_event,
                msg_id=f"raw:{message_id}",
                event_type="raw",
            ),
            self.publish_json(
                self.config.message_subject,
                message_event,
                msg_id=f"message:{message_id}",
                event_type="message",
            ),
        )

    async def publish_json(
        self,
        subject: str,
        payload: Dict[str, Any],
        *,
        msg_id: str,
        event_type: str,
    ) -> None:
        _validate_subject(subject)
        body = json.dumps(payload, ensure_ascii=False, separators=(",", ":"), default=_json_default).encode("utf-8")
        headers = _headers(
            {
                "Nats-Msg-Id": _header_value(msg_id),
                "Content-Type": "application/json",
                "Qintopia-Event-Type": _header_value(event_type),
            }
        )
        command = f"HPUB {subject} {len(headers)} {len(headers) + len(body)}\r\n".encode("ascii")

        reader, writer = await asyncio.wait_for(
            asyncio.open_connection(self.host, self.port),
            timeout=self.config.timeout_seconds,
        )
        try:
            info = await asyncio.wait_for(reader.readline(), timeout=self.config.timeout_seconds)
            if not info.startswith(b"INFO "):
                raise RuntimeError(f"unexpected NATS greeting: {info[:80]!r}")
            writer.write(_connect_payload(self.user, self.password))
            writer.write(command)
            writer.write(headers)
            writer.write(body)
            writer.write(b"\r\n")
            await asyncio.wait_for(writer.drain(), timeout=self.config.timeout_seconds)
        finally:
            writer.close()
            try:
                await asyncio.wait_for(writer.wait_closed(), timeout=self.config.timeout_seconds)
            except Exception:
                pass


def build_capture_events(parsed: Any, raw_body: bytes, identity: Any = None) -> Tuple[Dict[str, Any], Dict[str, Any], str]:
    received_at = _now_iso()
    message_id = str(getattr(parsed, "message_id", "") or "").strip()
    if not message_id:
        message_id = str(getattr(parsed, "event_id", "") or "").strip()
    if not message_id:
        raise ValueError("parsed QiWe message missing message_id")

    raw_payload = json.loads(raw_body.decode("utf-8"))
    raw_event = {
        "event_id": message_id,
        "received_at": received_at,
        "source": "qiwe",
        "payload": raw_payload,
    }

    conversation_type = str(getattr(parsed, "conversation_type", "") or "group")
    chat_id = str(getattr(parsed, "chat_id", "") or "").strip()
    if not chat_id:
        chat_id = str(getattr(parsed, "group_id", "") or getattr(parsed, "sender_id", "") or "").strip()

    parsed_sender_id = str(getattr(parsed, "sender_id", "") or "").strip()
    parsed_sender_name = str(getattr(parsed, "sender_name", "") or "").strip()
    identity_display_name = str(getattr(identity, "display_name", "") or "").strip() if identity is not None else ""
    identity_source = str(getattr(identity, "source", "") or "").strip() if identity is not None else ""
    if identity_source == "fallback" and identity_display_name == parsed_sender_id:
        identity_display_name = ""
        identity_source = ""
    if identity_source == "webhook" and not parsed_sender_name and identity_display_name == parsed_sender_id:
        identity_display_name = ""
        identity_source = ""
    if not identity_display_name and parsed_sender_name:
        identity_display_name = parsed_sender_name
        identity_source = "webhook"
    sender_name = identity_display_name
    sender_identity: Dict[str, Any] = {
        "platform": "qiwe",
        "chat_id": chat_id,
        "channel_user_id": parsed_sender_id,
        "display_name": identity_display_name,
        "identity_source": identity_source,
        "resolved_at": received_at,
    }
    if not identity_display_name:
        sender_identity["error"] = "display_name_unresolved"

    message_event = {
        "event_id": message_id,
        "message_id": message_id,
        "platform": "qiwe",
        "chat_id": chat_id,
        "chat_type": "direct" if conversation_type == "direct" else "group",
        "sender_id": str(getattr(parsed, "sender_id", "") or ""),
        "sender_name": sender_name,
        "sender_identity": sender_identity,
        "text": str(getattr(parsed, "text", "") or ""),
        "message_kind": str(getattr(parsed, "message_kind", "") or "unsupported"),
        "is_mention_bot": bool(getattr(parsed, "is_mentioned", False)),
        "should_trigger": bool(getattr(parsed, "should_trigger", False)),
        "trigger_reason": str(getattr(parsed, "reason", "") or ""),
        "sent_at": _datetime_to_iso(getattr(parsed, "timestamp", None)),
        "received_at": received_at,
        "raw": getattr(parsed, "raw_event", {}) if isinstance(getattr(parsed, "raw_event", {}), dict) else {},
        "mentions": list(getattr(parsed, "at_list", []) or []),
        "attachments": list(getattr(parsed, "attachments", []) or []),
        "content": str(getattr(parsed, "content", "") or ""),
    }
    return raw_event, message_event, message_id


def _connect_payload(user: str, password: str) -> bytes:
    payload: Dict[str, Any] = {
        "verbose": False,
        "pedantic": False,
        "headers": True,
        "no_responders": True,
        "lang": "python",
        "version": "qintopia-qiwe-plugin",
    }
    if user:
        payload["user"] = user
    if password:
        payload["pass"] = password
    return f"CONNECT {json.dumps(payload, separators=(',', ':'))}\r\n".encode("ascii")


def _headers(values: Dict[str, str]) -> bytes:
    lines = ["NATS/1.0"]
    for key, value in values.items():
        lines.append(f"{key}: {value}")
    return ("\r\n".join(lines) + "\r\n\r\n").encode("utf-8")


def _validate_subject(subject: str) -> None:
    if not subject or any(char.isspace() for char in subject):
        raise ValueError(f"invalid NATS subject: {subject!r}")


def _header_value(value: str) -> str:
    return str(value).replace("\r", " ").replace("\n", " ").strip()


def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _datetime_to_iso(value: Any) -> str | None:
    if not isinstance(value, datetime):
        return None
    if value.tzinfo is None:
        value = value.replace(tzinfo=timezone.utc)
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def _json_default(value: Any) -> str:
    if isinstance(value, datetime):
        return _datetime_to_iso(value) or ""
    return str(value)
