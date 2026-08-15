from __future__ import annotations

import asyncio
import hashlib
import json
import math
import os
import secrets
import stat
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, Tuple
from urllib.parse import urlparse


DEFAULT_NATS_URL = "nats://127.0.0.1:4222"
DEFAULT_RAW_SUBJECT = "qintopia.qiwe.raw"
DEFAULT_AUTHENTICATED_RAW_SUBJECT = "qintopia.qiwe.raw.authenticated"
DEFAULT_MESSAGE_SUBJECT = "qintopia.qiwe.message"
DEFAULT_TIMEOUT_SECONDS = 0.5
MAX_NATS_AUTH_FILE_BYTES = 4_096
MAX_PUB_ACK_BYTES = 8_192
MAX_NATS_CONTROL_LINE_BYTES = 1_024
QIWE_ASYNC_CALLBACK_COMMAND = 20_000
DEFAULT_STRICT_JSON_MAX_BYTES = 1_048_576
DEFAULT_STRICT_JSON_MAX_DEPTH = 64
DEFAULT_STRICT_JSON_MAX_NODES = 65_536
DEFAULT_STRICT_JSON_MAX_STRING_BYTES = 524_288
DEFAULT_STRICT_JSON_MAX_KEY_BYTES = 1_024
MAX_ROOM_DISPLAY_NAME_CHARS = 200


class StrictJsonError(ValueError):
    pass


def _unique_json_object(pairs: list[tuple[str, Any]]) -> Dict[str, Any]:
    value: Dict[str, Any] = {}
    for key, child in pairs:
        if key in value:
            raise StrictJsonError("duplicate JSON object key")
        value[key] = child
    return value


def _reject_non_finite_constant(_value: str) -> None:
    raise StrictJsonError("non-finite JSON number")


def validate_bounded_json_value(
    value: Any,
    *,
    max_depth: int = DEFAULT_STRICT_JSON_MAX_DEPTH,
    max_nodes: int = DEFAULT_STRICT_JSON_MAX_NODES,
    max_string_bytes: int = DEFAULT_STRICT_JSON_MAX_STRING_BYTES,
    max_key_bytes: int = DEFAULT_STRICT_JSON_MAX_KEY_BYTES,
) -> None:
    stack = [(value, 0)]
    nodes = 0
    while stack:
        current, depth = stack.pop()
        nodes += 1
        if nodes > max_nodes:
            raise StrictJsonError("JSON node limit exceeded")
        if depth > max_depth:
            raise StrictJsonError("JSON depth limit exceeded")
        if isinstance(current, str):
            if len(current.encode("utf-8")) > max_string_bytes:
                raise StrictJsonError("JSON string limit exceeded")
        elif isinstance(current, float):
            if not math.isfinite(current):
                raise StrictJsonError("non-finite JSON number")
        elif isinstance(current, list):
            stack.extend((child, depth + 1) for child in current)
        elif isinstance(current, dict):
            for key, child in current.items():
                if not isinstance(key, str) or len(key.encode("utf-8")) > max_key_bytes:
                    raise StrictJsonError("JSON key limit exceeded")
                stack.append((child, depth + 1))
        elif current is None or isinstance(current, (bool, int)):
            continue
        else:
            raise StrictJsonError("unsupported JSON value")


def parse_strict_bounded_json(
    raw: bytes | str,
    *,
    max_bytes: int = DEFAULT_STRICT_JSON_MAX_BYTES,
    max_depth: int = DEFAULT_STRICT_JSON_MAX_DEPTH,
    max_nodes: int = DEFAULT_STRICT_JSON_MAX_NODES,
    max_string_bytes: int = DEFAULT_STRICT_JSON_MAX_STRING_BYTES,
    max_key_bytes: int = DEFAULT_STRICT_JSON_MAX_KEY_BYTES,
) -> Any:
    if isinstance(raw, bytes):
        if len(raw) > max_bytes:
            raise StrictJsonError("JSON byte limit exceeded")
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise StrictJsonError("JSON is not UTF-8") from exc
    elif isinstance(raw, str):
        if len(raw.encode("utf-8")) > max_bytes:
            raise StrictJsonError("JSON byte limit exceeded")
        text = raw
    else:
        raise StrictJsonError("JSON input must be bytes or text")
    try:
        value = json.loads(
            text,
            object_pairs_hook=_unique_json_object,
            parse_constant=_reject_non_finite_constant,
        )
    except (json.JSONDecodeError, RecursionError) as exc:
        raise StrictJsonError("invalid JSON") from exc
    validate_bounded_json_value(
        value,
        max_depth=max_depth,
        max_nodes=max_nodes,
        max_string_bytes=max_string_bytes,
        max_key_bytes=max_key_bytes,
    )
    return value


@dataclass
class QiWeNatsCaptureConfig:
    enabled: bool = False
    url: str = DEFAULT_NATS_URL
    auth_file: str = ""
    raw_subject: str = DEFAULT_RAW_SUBJECT
    authenticated_raw_subject: str = DEFAULT_AUTHENTICATED_RAW_SUBJECT
    message_subject: str = DEFAULT_MESSAGE_SUBJECT
    timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS


class QiWeNatsPublisher:
    def __init__(self, config: QiWeNatsCaptureConfig) -> None:
        self.config = config
        parsed = urlparse(config.url)
        if parsed.scheme not in {"nats", ""}:
            raise ValueError(f"unsupported NATS URL scheme: {parsed.scheme}")
        if parsed.username is not None or parsed.password is not None:
            raise ValueError("NATS URL userinfo is forbidden; use the NATS auth file")
        self.host = parsed.hostname or "127.0.0.1"
        self.port = parsed.port or 4222
        self.user, self.password = _load_nats_auth_file(config.auth_file)
        if config.authenticated_raw_subject == config.raw_subject:
            raise ValueError("authenticated and legacy raw NATS subjects must differ")

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

    async def publish_raw_durable(
        self,
        raw_event: Dict[str, Any],
        *,
        message_id: str,
    ) -> None:
        if not self.config.auth_file:
            raise RuntimeError("authenticated NATS publisher credentials are required")
        await self.publish_json_with_ack(
            self.config.authenticated_raw_subject,
            raw_event,
            msg_id=f"raw:{message_id}",
            event_type="authenticated_raw",
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

    async def publish_json_with_ack(
        self,
        subject: str,
        payload: Dict[str, Any],
        *,
        msg_id: str,
        event_type: str,
    ) -> None:
        _validate_subject(subject)
        body = json.dumps(
            payload,
            ensure_ascii=False,
            separators=(",", ":"),
            default=_json_default,
        ).encode("utf-8")
        headers = _headers(
            {
                "Nats-Msg-Id": _header_value(msg_id),
                "Content-Type": "application/json",
                "Qintopia-Event-Type": _header_value(event_type),
            }
        )
        inbox = f"_INBOX.qintopia.{secrets.token_hex(16)}"
        subscription_id = "1"
        command = (
            f"HPUB {subject} {inbox} {len(headers)} {len(headers) + len(body)}\r\n"
        ).encode("ascii")

        reader, writer = await asyncio.wait_for(
            asyncio.open_connection(self.host, self.port),
            timeout=self.config.timeout_seconds,
        )
        try:
            info = await asyncio.wait_for(
                reader.readline(), timeout=self.config.timeout_seconds
            )
            if not info.startswith(b"INFO "):
                raise RuntimeError("unexpected NATS greeting")
            writer.write(_connect_payload(self.user, self.password))
            writer.write(f"SUB {inbox} {subscription_id}\r\n".encode("ascii"))
            writer.write(f"UNSUB {subscription_id} 1\r\n".encode("ascii"))
            writer.write(command)
            writer.write(headers)
            writer.write(body)
            writer.write(b"\r\n")
            await asyncio.wait_for(
                writer.drain(), timeout=self.config.timeout_seconds
            )
            await _read_jetstream_pub_ack(
                reader,
                writer,
                inbox=inbox,
                subscription_id=subscription_id,
                timeout_seconds=self.config.timeout_seconds,
            )
        finally:
            writer.close()
            try:
                await asyncio.wait_for(
                    writer.wait_closed(), timeout=self.config.timeout_seconds
                )
            except Exception:
                pass


async def _read_jetstream_pub_ack(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    *,
    inbox: str,
    subscription_id: str,
    timeout_seconds: float,
) -> None:
    while True:
        line = await asyncio.wait_for(reader.readline(), timeout=timeout_seconds)
        if not line:
            raise RuntimeError("NATS connection closed before publish acknowledgement")
        if len(line) > MAX_NATS_CONTROL_LINE_BYTES:
            raise RuntimeError("NATS publish acknowledgement control line is too large")
        if line == b"PING\r\n":
            writer.write(b"PONG\r\n")
            await asyncio.wait_for(writer.drain(), timeout=timeout_seconds)
            continue
        if line.startswith(b"INFO ") or line in {b"+OK\r\n", b"PONG\r\n"}:
            continue
        if line.startswith(b"-ERR"):
            raise RuntimeError("NATS server rejected durable publish")

        parts = line.rstrip(b"\r\n").split()
        if not parts or parts[0] not in {b"MSG", b"HMSG"}:
            raise RuntimeError("unexpected NATS publish acknowledgement frame")
        expected_inbox = inbox.encode("ascii")
        expected_subscription_id = subscription_id.encode("ascii")
        if len(parts) < 4 or parts[1] != expected_inbox or parts[2] != expected_subscription_id:
            raise RuntimeError("NATS publish acknowledgement binding mismatch")

        try:
            if parts[0] == b"MSG":
                if len(parts) not in {4, 5}:
                    raise ValueError
                header_bytes = 0
                total_bytes = int(parts[-1])
            else:
                if len(parts) not in {5, 6}:
                    raise ValueError
                header_bytes = int(parts[-2])
                total_bytes = int(parts[-1])
        except ValueError as exc:
            raise RuntimeError("invalid NATS publish acknowledgement length") from exc
        if (
            header_bytes < 0
            or total_bytes <= 0
            or header_bytes > total_bytes
            or total_bytes > MAX_PUB_ACK_BYTES
        ):
            raise RuntimeError("invalid NATS publish acknowledgement size")

        frame = await asyncio.wait_for(
            reader.readexactly(total_bytes + 2), timeout=timeout_seconds
        )
        if not frame.endswith(b"\r\n"):
            raise RuntimeError("invalid NATS publish acknowledgement terminator")
        payload = frame[header_bytes:total_bytes]
        try:
            acknowledgement = json.loads(payload.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise RuntimeError("invalid JetStream publish acknowledgement") from exc
        if not isinstance(acknowledgement, dict) or acknowledgement.get("error") is not None:
            raise RuntimeError("JetStream rejected durable publish")
        stream = acknowledgement.get("stream")
        sequence = acknowledgement.get("seq")
        if (
            not isinstance(stream, str)
            or not stream
            or not isinstance(sequence, int)
            or isinstance(sequence, bool)
            or sequence <= 0
        ):
            raise RuntimeError("invalid JetStream publish acknowledgement")
        return


def build_capture_events(
    parsed: Any,
    raw_body: bytes,
    identity: Any = None,
    *,
    conversation_display_name: str = "",
    ingress_auth_verified: bool = False,
) -> Tuple[Dict[str, Any], Dict[str, Any], str]:
    received_at = _now_iso()
    message_id = str(getattr(parsed, "message_id", "") or "").strip()
    if not message_id:
        message_id = str(getattr(parsed, "event_id", "") or "").strip()
    if not message_id:
        raise ValueError("parsed QiWe message missing message_id")

    raw_payload = parse_strict_bounded_json(raw_body)
    if not isinstance(raw_payload, dict):
        raise StrictJsonError("QiWe capture payload must be a JSON object")
    raw_payload, callback_sanitized = _sanitize_qiwe_capture_payload(raw_payload)
    if callback_sanitized:
        message_id = _callback_event_id(message_id)
    raw_event = {
        "event_id": message_id,
        "received_at": received_at,
        "source": "qiwe",
        # Authentication provenance is transport-owned. Consumers must ignore any
        # publisher-supplied value and derive trust from the protected NATS subject.
        "ingress_auth_verified": False,
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
    room_display_name = str(conversation_display_name or "").strip()
    if (
        not callback_sanitized
        and conversation_type == "group"
        and room_display_name
        and len(room_display_name) <= MAX_ROOM_DISPLAY_NAME_CHARS
        and room_display_name.isprintable()
    ):
        message_event["conversation_display_name"] = room_display_name
        message_event["conversation_display_name_source"] = "qiwe_room_detail"
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
            for field in ("file_aes_key", "file_id", "file_md5", "file_size")
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
            for field in ("file_aes_key", "file_id", "file_md5", "file_size")
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


def _load_nats_auth_file(path: str) -> Tuple[str, str]:
    if not path:
        return "", ""
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
        try:
            metadata = os.fstat(descriptor)
            if not stat.S_ISREG(metadata.st_mode):
                raise ValueError
            if metadata.st_mode & stat.S_IWOTH or metadata.st_mode & stat.S_IROTH:
                raise ValueError
            if metadata.st_size <= 0 or metadata.st_size > MAX_NATS_AUTH_FILE_BYTES:
                raise ValueError
            raw = os.read(descriptor, MAX_NATS_AUTH_FILE_BYTES + 1)
        finally:
            os.close(descriptor)
        value = parse_strict_bounded_json(raw, max_bytes=MAX_NATS_AUTH_FILE_BYTES)
        if not isinstance(value, dict) or set(value) != {"version", "username", "password"}:
            raise ValueError
        if value["version"] != 1 or isinstance(value["version"], bool):
            raise ValueError
        username = value["username"]
        password = value["password"]
        if not _valid_nats_auth_value(username) or not _valid_nats_auth_value(password):
            raise ValueError
        return username, password
    except (OSError, StrictJsonError, ValueError, TypeError) as exc:
        raise ValueError("invalid NATS auth file") from exc


def _valid_nats_auth_value(value: Any) -> bool:
    return (
        isinstance(value, str)
        and 1 <= len(value.encode("utf-8")) <= 256
        and not any(char.isspace() or ord(char) < 32 or ord(char) == 127 for char in value)
    )


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
