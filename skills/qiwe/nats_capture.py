from __future__ import annotations

import asyncio
import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, Tuple
from urllib.parse import unquote, urlparse


DEFAULT_NATS_URL = "nats://127.0.0.1:4222"
DEFAULT_RAW_SUBJECT = "qintopia.qiwe.raw"
DEFAULT_MESSAGE_SUBJECT = "qintopia.qiwe.message"
DEFAULT_TIMEOUT_SECONDS = 0.5
QIWE_ASYNC_CALLBACK_COMMAND = 20_000


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
    raw_payload, callback_sanitized = _sanitize_qiwe_capture_payload(raw_payload)
    if callback_sanitized:
        message_id = _callback_event_id(message_id)
    raw_event = {
        "event_id": message_id,
        "received_at": received_at,
        "source": "qiwe",
        "payload": raw_payload,
    }

    conversation_type = str(getattr(parsed, "conversation_type", "") or "group")
    chat_id = "" if callback_sanitized else str(getattr(parsed, "chat_id", "") or "").strip()
    if not chat_id and not callback_sanitized:
        chat_id = str(getattr(parsed, "group_id", "") or getattr(parsed, "sender_id", "") or "").strip()

    parsed_sender_id = "" if callback_sanitized else str(getattr(parsed, "sender_id", "") or "").strip()
    parsed_sender_name = "" if callback_sanitized else str(getattr(parsed, "sender_name", "") or "").strip()
    identity_display_name = str(getattr(identity, "display_name", "") or "").strip() if identity is not None else ""
    identity_source = str(getattr(identity, "source", "") or "").strip() if identity is not None else ""
    if callback_sanitized:
        identity_display_name = ""
        identity_source = ""
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
        "sender_id": parsed_sender_id,
        "sender_name": sender_name,
        "sender_identity": sender_identity,
        "text": "" if callback_sanitized else str(getattr(parsed, "text", "") or ""),
        "message_kind": "system" if callback_sanitized else str(getattr(parsed, "message_kind", "") or "unsupported"),
        "is_mention_bot": False if callback_sanitized else bool(getattr(parsed, "is_mentioned", False)),
        "should_trigger": False if callback_sanitized else bool(getattr(parsed, "should_trigger", False)),
        "trigger_reason": "qiwe_async_callback_sanitized" if callback_sanitized else str(getattr(parsed, "reason", "") or ""),
        "sent_at": _datetime_to_iso(getattr(parsed, "timestamp", None)),
        "received_at": received_at,
        "raw": raw_payload if callback_sanitized else (getattr(parsed, "raw_event", {}) if isinstance(getattr(parsed, "raw_event", {}), dict) else {}),
        "mentions": [] if callback_sanitized else list(getattr(parsed, "at_list", []) or []),
        "attachments": [] if callback_sanitized else list(getattr(parsed, "attachments", []) or []),
        "content": "" if callback_sanitized else str(getattr(parsed, "content", "") or ""),
    }
    return raw_event, message_event, message_id


def _sanitize_qiwe_capture_payload(value: Any) -> Tuple[Any, bool]:
    if (
        isinstance(value, dict)
        and value.get("source") == "qiwe_async_callback"
        and value.get("credentials_redacted") is True
        and isinstance(value.get("callback_events"), list)
    ):
        return _canonicalize_sanitized_callback_payload(value), True
    callback_events: list[Dict[str, Any]] = []
    _collect_callback_events(value, callback_events)
    if not callback_events:
        return value, False
    return (
        {
            "callback_event_count": len(callback_events),
            "callback_events": callback_events,
            "credentials_redacted": True,
            "source": "qiwe_async_callback",
        },
        True,
    )


def _canonicalize_sanitized_callback_payload(value: Dict[str, Any]) -> Dict[str, Any]:
    callback_events = [
        canonical
        for event in value.get("callback_events", [])
        if (canonical := _canonicalize_sanitized_callback_event(event)) is not None
    ]
    return {
        "callback_event_count": len(callback_events),
        "callback_events": callback_events,
        "credentials_redacted": True,
        "source": "qiwe_async_callback",
    }


def _canonicalize_sanitized_callback_event(value: Any) -> Dict[str, Any] | None:
    if not isinstance(value, dict) or not _is_async_callback_event(value):
        return None
    request_id_sha256 = _value_for_key(value, "requestidsha256")
    if not _is_sha256_marker(request_id_sha256):
        request_id_sha256 = None
    else:
        request_id_sha256 = request_id_sha256.lower()
    return {
        "cmd": QIWE_ASYNC_CALLBACK_COMMAND,
        "credentials_redacted": True,
        "msg_data_summary": _canonicalize_callback_msg_data_summary(
            _value_for_key(value, "msgdatasummary")
        ),
        "request_id_sha256": request_id_sha256,
    }


def _canonicalize_callback_msg_data_summary(value: Any) -> Dict[str, Any]:
    if not isinstance(value, dict):
        return _callback_msg_data_summary(None)
    fields = _value_for_key(value, "fieldpresence")
    fields = fields if isinstance(fields, dict) else {}
    presence = {
        "cloud_url": _value_for_key(fields, "cloudurl") is True,
        "file_aes_key": _value_for_key(fields, "fileaeskey") is True,
        "file_id": _value_for_key(fields, "fileid") is True,
        "file_md5": _value_for_key(fields, "filemd5") is True,
        "file_size": _value_for_key(fields, "filesize") is True,
        "filename": _value_for_key(fields, "filename") is True,
    }
    unknown_field_count = _value_for_key(value, "unknownfieldcount")
    if not isinstance(unknown_field_count, int) or isinstance(unknown_field_count, bool):
        unknown_field_count = 0
    return {
        "field_presence": presence,
        "msg_data_object": _value_for_key(value, "msgdataobject") is True,
        "msg_data_present": _value_for_key(value, "msgdatapresent") is True,
        "required_fields_present": all(
            presence[field]
            for field in ("file_aes_key", "file_id", "file_md5", "file_size", "filename")
        ),
        "unknown_field_count": max(0, unknown_field_count),
    }


def _is_sha256_marker(value: Any) -> bool:
    if not isinstance(value, str) or not value.startswith("sha256:"):
        return False
    digest = value.removeprefix("sha256:")
    return len(digest) == 64 and all(char in "0123456789abcdefABCDEF" for char in digest)


def _collect_callback_events(value: Any, events: list[Dict[str, Any]]) -> None:
    if isinstance(value, dict):
        if _is_async_callback_event(value):
            events.append(_sanitize_callback_event(value))
            return
        for item in value.values():
            _collect_callback_events(item, events)
    elif isinstance(value, list):
        for item in value:
            _collect_callback_events(item, events)


def _is_async_callback_event(value: Dict[str, Any]) -> bool:
    command = _value_for_key(value, "cmd")
    try:
        return int(command) == QIWE_ASYNC_CALLBACK_COMMAND
    except (TypeError, ValueError):
        return False


def _sanitize_callback_event(value: Dict[str, Any]) -> Dict[str, Any]:
    request_id = _value_for_key(value, "requestid")
    request_id_text = str(request_id).strip() if isinstance(request_id, (str, int)) else ""
    return {
        "cmd": QIWE_ASYNC_CALLBACK_COMMAND,
        "credentials_redacted": True,
        "msg_data_summary": _callback_msg_data_summary(_value_for_key(value, "msgdata")),
        "request_id_sha256": f"sha256:{_sha256(request_id_text.encode('utf-8'))}" if request_id_text else None,
    }


def _callback_msg_data_summary(value: Any) -> Dict[str, Any]:
    if not isinstance(value, dict):
        return {
            "field_presence": {},
            "msg_data_object": False,
            "msg_data_present": value is not None,
            "required_fields_present": False,
            "unknown_field_count": 0,
        }
    normalized_keys = {_normalize_key(key) for key in value}
    known_fields = {"fileaeskey", "fileid", "filemd5", "filesize", "filename", "cloudurl"}
    presence = {
        "cloud_url": "cloudurl" in normalized_keys,
        "file_aes_key": "fileaeskey" in normalized_keys,
        "file_id": "fileid" in normalized_keys,
        "file_md5": "filemd5" in normalized_keys,
        "file_size": "filesize" in normalized_keys,
        "filename": "filename" in normalized_keys,
    }
    return {
        "field_presence": presence,
        "msg_data_object": True,
        "msg_data_present": True,
        "required_fields_present": all(
            presence[field]
            for field in ("file_aes_key", "file_id", "file_md5", "file_size", "filename")
        ),
        "unknown_field_count": len(normalized_keys - known_fields),
    }


def _value_for_key(value: Dict[str, Any], expected: str) -> Any:
    for key, item in value.items():
        if _normalize_key(key) == expected:
            return item
    return None


def _normalize_key(value: Any) -> str:
    return "".join(char.lower() for char in str(value) if char.isascii() and char.isalnum())


def _callback_event_id(value: str) -> str:
    prefix = "qiwe-callback:"
    if value.startswith(prefix):
        digest = value.removeprefix(prefix)
        if len(digest) == 64 and all(char in "0123456789abcdefABCDEF" for char in digest):
            return f"{prefix}{digest.lower()}"
    return f"qiwe-callback:{_sha256(value.encode('utf-8'))}"


def _sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


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
