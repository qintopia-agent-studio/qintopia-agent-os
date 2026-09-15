#!/usr/bin/env python3
"""Prove the fixed production NATS subject ACL before Space runtime activation."""

from __future__ import annotations

import json
import os
import secrets
import socket
import stat
import sys
import time
from dataclasses import dataclass
from typing import Any


NATS_HOST = "127.0.0.1"
NATS_PORT = 4222
TRUSTED_SUBJECT = "qintopia.qiwe.raw.authenticated"
EXPECTED_STREAM = "QINTOPIA_QIWE_MESSAGES"
EXPECTED_CONSUMER = "qintopia-message-sidecar"
LEGACY_RAW_SUBJECT = "qintopia.qiwe.raw"
MESSAGE_SUBJECT = "qintopia.qiwe.message"
PRODUCER_AUTH_FILE = "/etc/qintopia/nats/qiwe-adapter.json"
CONSUMER_AUTH_FILE = "/etc/qintopia/nats/message-sidecar.json"

STREAM_INFO_SUBJECT = f"$JS.API.STREAM.INFO.{EXPECTED_STREAM}"
CONSUMER_INFO_SUBJECT = (
    f"$JS.API.CONSUMER.INFO.{EXPECTED_STREAM}.{EXPECTED_CONSUMER}"
)
CONSUMER_CREATE_SUBJECT = (
    f"$JS.API.CONSUMER.CREATE.{EXPECTED_STREAM}.{EXPECTED_CONSUMER}"
)
CONSUMER_NEXT_SUBJECT = (
    f"$JS.API.CONSUMER.MSG.NEXT.{EXPECTED_STREAM}.{EXPECTED_CONSUMER}"
)
CONSUMER_ACK_PROBE_SUBJECT = (
    f"$JS.ACK.{EXPECTED_STREAM}.{EXPECTED_CONSUMER}.0.0.0.0.0"
)

TEST_MODE_KEY = "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_MODE"
TEST_PORT_KEY = "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_PORT"
TEST_PRODUCER_AUTH_FILE_KEY = (
    "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_PRODUCER_AUTH_FILE"
)
TEST_CONSUMER_AUTH_FILE_KEY = (
    "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_CONSUMER_AUTH_FILE"
)
TEST_OVERRIDE_KEYS = (
    TEST_PORT_KEY,
    TEST_PRODUCER_AUTH_FILE_KEY,
    TEST_CONSUMER_AUTH_FILE_KEY,
)

MAX_AUTH_FILE_BYTES = 4_096
MAX_AUTH_VALUE_BYTES = 256
MAX_CONTROL_LINE_BYTES = 16_384
MAX_FRAME_BYTES = 1_048_576
MAX_FRAMES = 32
TOTAL_TIMEOUT_SECONDS = 8.0
SOCKET_TIMEOUT_SECONDS = 2.0


class PreflightError(Exception):
    pass


class ServerDenied(PreflightError):
    pass


@dataclass(frozen=True)
class Settings:
    port: int
    producer_auth_file: str
    consumer_auth_file: str
    require_root_owner: bool


@dataclass(frozen=True)
class Auth:
    username: str
    password: str


def _reject_constant(_: str) -> None:
    raise ValueError


def _strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError
        value[key] = item
    return value


def _strict_json(raw: bytes) -> Any:
    try:
        return json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=_strict_object,
            parse_constant=_reject_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError, TypeError) as exc:
        raise PreflightError from exc


def _settings_from_environment() -> Settings:
    test_mode = os.environ.get(TEST_MODE_KEY, "")
    if test_mode not in {"", "1"}:
        raise PreflightError

    overrides_present = any(os.environ.get(key, "") for key in TEST_OVERRIDE_KEYS)
    if test_mode != "1":
        if overrides_present:
            raise PreflightError
        return Settings(
            port=NATS_PORT,
            producer_auth_file=PRODUCER_AUTH_FILE,
            consumer_auth_file=CONSUMER_AUTH_FILE,
            require_root_owner=True,
        )

    port_text = os.environ.get(TEST_PORT_KEY, "")
    producer_path = os.environ.get(TEST_PRODUCER_AUTH_FILE_KEY, "")
    consumer_path = os.environ.get(TEST_CONSUMER_AUTH_FILE_KEY, "")
    if not port_text.isascii() or not port_text.isdigit():
        raise PreflightError
    port = int(port_text, 10)
    if not 1 <= port <= 65_535:
        raise PreflightError
    if (
        not producer_path
        or not consumer_path
        or not os.path.isabs(producer_path)
        or not os.path.isabs(consumer_path)
        or producer_path == consumer_path
    ):
        raise PreflightError
    return Settings(
        port=port,
        producer_auth_file=producer_path,
        consumer_auth_file=consumer_path,
        require_root_owner=False,
    )


def _valid_auth_value(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    encoded = value.encode("utf-8")
    return (
        1 <= len(encoded) <= MAX_AUTH_VALUE_BYTES
        and not any(character.isspace() or ord(character) < 32 or ord(character) == 127 for character in value)
    )


def _load_auth_file(path: str, *, require_root_owner: bool) -> Auth:
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
        try:
            metadata = os.fstat(descriptor)
            if (
                not stat.S_ISREG(metadata.st_mode)
                or metadata.st_size <= 0
                or metadata.st_size > MAX_AUTH_FILE_BYTES
                or metadata.st_mode & 0o027
                or (require_root_owner and metadata.st_uid != 0)
            ):
                raise PreflightError
            raw = os.read(descriptor, MAX_AUTH_FILE_BYTES + 1)
            if len(raw) != metadata.st_size:
                raise PreflightError
        finally:
            os.close(descriptor)
    except (OSError, PreflightError) as exc:
        raise PreflightError from exc

    value = _strict_json(raw)
    if not isinstance(value, dict) or set(value) != {
        "version",
        "username",
        "password",
    }:
        raise PreflightError
    if value["version"] != 1 or isinstance(value["version"], bool):
        raise PreflightError
    if not _valid_auth_value(value["username"]) or not _valid_auth_value(
        value["password"]
    ):
        raise PreflightError
    return Auth(username=value["username"], password=value["password"])


def _remaining(deadline: float) -> float:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise PreflightError
    return min(remaining, SOCKET_TIMEOUT_SECONDS)


class NatsClient:
    def __init__(self, port: int, deadline: float) -> None:
        self.deadline = deadline
        self.buffer = bytearray()
        try:
            self.sock = socket.create_connection(
                (NATS_HOST, port), timeout=_remaining(deadline)
            )
            self.sock.settimeout(_remaining(deadline))
        except OSError as exc:
            raise PreflightError from exc
        try:
            greeting = self.read_line()
            if not greeting.startswith(b"INFO "):
                raise PreflightError
        except Exception:
            self.close()
            raise

    def close(self) -> None:
        try:
            self.sock.close()
        except OSError:
            pass

    def send(self, payload: bytes) -> None:
        try:
            self.sock.settimeout(_remaining(self.deadline))
            self.sock.sendall(payload)
        except OSError as exc:
            raise PreflightError from exc

    def read_line(self) -> bytes:
        while True:
            boundary = self.buffer.find(b"\r\n")
            if boundary >= 0:
                if boundary + 2 > MAX_CONTROL_LINE_BYTES:
                    raise PreflightError
                line = bytes(self.buffer[: boundary + 2])
                del self.buffer[: boundary + 2]
                return line
            if len(self.buffer) >= MAX_CONTROL_LINE_BYTES:
                raise PreflightError
            try:
                self.sock.settimeout(_remaining(self.deadline))
                chunk = self.sock.recv(4_096)
            except (OSError, TimeoutError) as exc:
                raise PreflightError from exc
            if not chunk:
                raise PreflightError
            self.buffer.extend(chunk)

    def read_exact(self, size: int) -> bytes:
        if size < 0 or size > MAX_FRAME_BYTES:
            raise PreflightError
        while len(self.buffer) < size:
            try:
                self.sock.settimeout(_remaining(self.deadline))
                chunk = self.sock.recv(min(65_536, size - len(self.buffer)))
            except (OSError, TimeoutError) as exc:
                raise PreflightError from exc
            if not chunk:
                raise PreflightError
            self.buffer.extend(chunk)
        value = bytes(self.buffer[:size])
        del self.buffer[:size]
        return value

    def connect(self, auth: Auth | None) -> None:
        payload: dict[str, Any] = {
            "verbose": False,
            "pedantic": True,
            "tls_required": False,
            "name": "qintopia-space-automation-nats-acl-preflight",
            "lang": "python",
            "version": "1",
            "protocol": 1,
            "echo": False,
            "headers": True,
            "no_responders": True,
        }
        if auth is not None:
            payload["user"] = auth.username
            payload["pass"] = auth.password
        command = json.dumps(payload, separators=(",", ":"), ensure_ascii=True)
        self.send(f"CONNECT {command}\r\n".encode("ascii"))

    def flush(self) -> None:
        self.send(b"PING\r\n")
        for _ in range(MAX_FRAMES):
            line = self.read_line()
            if line == b"PING\r\n":
                self.send(b"PONG\r\n")
            elif line == b"PONG\r\n":
                return
            elif line.startswith(b"-ERR"):
                raise ServerDenied
            elif line.startswith(b"INFO ") or line == b"+OK\r\n":
                continue
            else:
                raise PreflightError
        raise PreflightError

    def read_message(self) -> tuple[str, str, bytes] | None:
        for _ in range(MAX_FRAMES):
            line = self.read_line()
            if line == b"PING\r\n":
                self.send(b"PONG\r\n")
                continue
            if line.startswith(b"INFO ") or line in {b"+OK\r\n", b"PONG\r\n"}:
                continue
            if line.startswith(b"-ERR"):
                raise ServerDenied

            parts = line.rstrip(b"\r\n").split()
            if not parts or parts[0] not in {b"MSG", b"HMSG"}:
                raise PreflightError
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
                raise PreflightError from exc
            if (
                header_bytes < 0
                or total_bytes < 0
                or header_bytes > total_bytes
                or total_bytes > MAX_FRAME_BYTES
            ):
                raise PreflightError
            frame = self.read_exact(total_bytes + 2)
            if not frame.endswith(b"\r\n"):
                raise PreflightError
            try:
                subject = parts[1].decode("ascii")
                sid = parts[2].decode("ascii")
            except UnicodeDecodeError as exc:
                raise PreflightError from exc
            return subject, sid, frame[header_bytes:total_bytes]
        raise PreflightError


def _headers(message_id: str) -> bytes:
    return (
        "NATS/1.0\r\n"
        f"Nats-Msg-Id: {message_id}\r\n"
        "Content-Type: application/json\r\n"
        "Qintopia-Event-Type: nats_acl_probe\r\n"
        "\r\n"
    ).encode("ascii")


def _subject_pattern_matches(pattern: str, subject: str) -> bool:
    pattern_tokens = pattern.split(".")
    subject_tokens = subject.split(".")
    subject_index = 0
    for index, token in enumerate(pattern_tokens):
        if token == ">":
            return index == len(pattern_tokens) - 1
        if subject_index >= len(subject_tokens):
            return False
        if token != "*" and token != subject_tokens[subject_index]:
            return False
        subject_index += 1
    return subject_index == len(subject_tokens)


def _api_request(
    client: NatsClient,
    *,
    api_subject: str,
    payload: bytes,
    sid: str,
) -> dict[str, Any]:
    inbox = f"_INBOX.qintopia.acl.api.{secrets.token_hex(16)}"
    client.send(f"SUB {inbox} {sid}\r\nUNSUB {sid} 1\r\n".encode("ascii"))
    client.flush()
    client.send(
        f"PUB {api_subject} {inbox} {len(payload)}\r\n".encode("ascii")
        + payload
        + b"\r\n"
    )
    for _ in range(MAX_FRAMES):
        message = client.read_message()
        if message is None:
            continue
        subject, received_sid, body = message
        if subject == TRUSTED_SUBJECT and received_sid == "1":
            continue
        if subject != inbox or received_sid != sid:
            raise PreflightError
        response = _strict_json(body)
        if not isinstance(response, dict):
            raise PreflightError
        return response
    raise PreflightError


def _require_api_validation_error(response: dict[str, Any]) -> None:
    error = response.get("error")
    if not isinstance(error, dict):
        raise PreflightError
    code = error.get("code")
    if not isinstance(code, int) or isinstance(code, bool) or not 400 <= code < 500:
        raise PreflightError


def _prove_consumer_jetstream_access(client: NatsClient) -> None:
    stream_info = _api_request(
        client,
        api_subject=STREAM_INFO_SUBJECT,
        payload=b"{}",
        sid="2",
    )
    stream_config = stream_info.get("config")
    if (
        stream_info.get("error") is not None
        or not isinstance(stream_config, dict)
        or stream_config.get("name") != EXPECTED_STREAM
    ):
        raise PreflightError
    stream_subjects = stream_config.get("subjects")
    if not isinstance(stream_subjects, list) or not any(
        isinstance(pattern, str)
        and _subject_pattern_matches(pattern, TRUSTED_SUBJECT)
        for pattern in stream_subjects
    ):
        raise PreflightError

    consumer_info = _api_request(
        client,
        api_subject=CONSUMER_INFO_SUBJECT,
        payload=b"{}",
        sid="3",
    )
    consumer_config = consumer_info.get("config")
    if (
        consumer_info.get("error") is not None
        or consumer_info.get("stream_name") != EXPECTED_STREAM
        or consumer_info.get("name") != EXPECTED_CONSUMER
        or not isinstance(consumer_config, dict)
        or consumer_config.get("durable_name") != EXPECTED_CONSUMER
        or consumer_config.get("ack_policy") != "explicit"
    ):
        raise PreflightError
    filter_subjects = consumer_config.get("filter_subjects")
    if not isinstance(filter_subjects, list) or set(filter_subjects) != {
        LEGACY_RAW_SUBJECT,
        MESSAGE_SUBJECT,
        TRUSTED_SUBJECT,
    }:
        raise PreflightError

    create_probe = _api_request(
        client,
        api_subject=CONSUMER_CREATE_SUBJECT,
        payload=b'{"stream_name":null}',
        sid="4",
    )
    _require_api_validation_error(create_probe)
    next_probe = _api_request(
        client,
        api_subject=CONSUMER_NEXT_SUBJECT,
        payload=b'{"batch":"invalid"}',
        sid="5",
    )
    _require_api_validation_error(next_probe)

    client.send(
        f"PUB {CONSUMER_ACK_PROBE_SUBJECT} 4\r\n+ACK\r\n".encode("ascii")
    )
    client.flush()


def _publish_command(
    *, reply: str | None, headers: bytes, body: bytes
) -> bytes:
    if reply is None:
        line = f"HPUB {TRUSTED_SUBJECT} {len(headers)} {len(headers) + len(body)}\r\n"
    else:
        line = (
            f"HPUB {TRUSTED_SUBJECT} {reply} {len(headers)} "
            f"{len(headers) + len(body)}\r\n"
        )
    return line.encode("ascii") + headers + body + b"\r\n"


def _assert_publish_denied(
    *,
    port: int,
    deadline: float,
    auth: Auth | None,
    allow_connection_denial: bool,
    headers: bytes,
    body: bytes,
) -> None:
    client = NatsClient(port, deadline)
    try:
        client.connect(auth)
        try:
            client.flush()
        except ServerDenied:
            if allow_connection_denial:
                return
            raise PreflightError
        client.send(_publish_command(reply=None, headers=headers, body=body))
        client.send(b"PING\r\n")
        for _ in range(MAX_FRAMES):
            line = client.read_line()
            if line == b"PING\r\n":
                client.send(b"PONG\r\n")
            elif line.startswith(b"-ERR"):
                return
            elif line == b"PONG\r\n":
                raise PreflightError
            elif line.startswith(b"INFO ") or line == b"+OK\r\n":
                continue
            else:
                raise PreflightError
        raise PreflightError
    finally:
        client.close()


def _subscribe_consumer(
    *, port: int, deadline: float, auth: Auth
) -> NatsClient:
    client = NatsClient(port, deadline)
    try:
        client.connect(auth)
        client.flush()
        client.send(f"SUB {TRUSTED_SUBJECT} 1\r\n".encode("ascii"))
        client.flush()
        return client
    except Exception:
        client.close()
        raise


def _publish_with_ack(
    *,
    port: int,
    deadline: float,
    auth: Auth,
    headers: bytes,
    body: bytes,
) -> None:
    client = NatsClient(port, deadline)
    inbox = f"_INBOX.qintopia.acl.{secrets.token_hex(16)}"
    try:
        client.connect(auth)
        client.flush()
        client.send(f"SUB {inbox} 1\r\nUNSUB 1 1\r\n".encode("ascii"))
        client.flush()
        client.send(
            _publish_command(reply=inbox, headers=headers, body=body)
        )
        for _ in range(MAX_FRAMES):
            message = client.read_message()
            if message is None:
                continue
            subject, sid, payload = message
            if subject != inbox or sid != "1":
                raise PreflightError
            acknowledgement = _strict_json(payload)
            if (
                not isinstance(acknowledgement, dict)
                or acknowledgement.get("error") is not None
                or acknowledgement.get("stream") != EXPECTED_STREAM
                or not isinstance(acknowledgement.get("seq"), int)
                or isinstance(acknowledgement.get("seq"), bool)
                or acknowledgement["seq"] <= 0
            ):
                raise PreflightError
            return
        raise PreflightError
    finally:
        client.close()


def _receive_probe(client: NatsClient, expected_body: bytes) -> None:
    total_bytes = 0
    for _ in range(MAX_FRAMES):
        message = client.read_message()
        if message is None:
            continue
        subject, sid, body = message
        if subject != TRUSTED_SUBJECT or sid != "1":
            raise PreflightError
        total_bytes += len(body)
        if total_bytes > MAX_FRAME_BYTES:
            raise PreflightError
        if body == expected_body:
            return
    raise PreflightError


def run_preflight() -> None:
    settings = _settings_from_environment()
    producer = _load_auth_file(
        settings.producer_auth_file,
        require_root_owner=settings.require_root_owner,
    )
    consumer = _load_auth_file(
        settings.consumer_auth_file,
        require_root_owner=settings.require_root_owner,
    )
    if producer.username == consumer.username:
        raise PreflightError

    nonce = secrets.token_hex(16)
    message_id = f"qintopia-nats-acl-probe-{nonce}"
    body = json.dumps(
        {
            "event_id": message_id,
            "payload": {
                "probe_type": "nats_acl_v1",
                "space_scoped": False,
            },
            "source": "qintopia_nats_acl_preflight",
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    headers = _headers(message_id)
    deadline = time.monotonic() + TOTAL_TIMEOUT_SECONDS

    subscriber = _subscribe_consumer(
        port=settings.port, deadline=deadline, auth=consumer
    )
    try:
        _prove_consumer_jetstream_access(subscriber)
        _assert_publish_denied(
            port=settings.port,
            deadline=deadline,
            auth=consumer,
            allow_connection_denial=False,
            headers=headers,
            body=body,
        )
        _assert_publish_denied(
            port=settings.port,
            deadline=deadline,
            auth=None,
            allow_connection_denial=True,
            headers=headers,
            body=body,
        )
        _publish_with_ack(
            port=settings.port,
            deadline=deadline,
            auth=producer,
            headers=headers,
            body=body,
        )
        _receive_probe(subscriber, body)
    finally:
        subscriber.close()


def main() -> int:
    if len(sys.argv) != 1:
        print("Space automation NATS ACL preflight failed", file=sys.stderr)
        return 1
    try:
        run_preflight()
    except Exception:
        print("Space automation NATS ACL preflight failed", file=sys.stderr)
        return 1
    print("space_automation_nats_acl_preflight=passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
