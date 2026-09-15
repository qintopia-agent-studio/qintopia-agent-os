#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import socketserver
import stat
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
PREFLIGHT = (
    REPO_ROOT
    / "deploy"
    / "sidecar"
    / "scripts"
    / "space-automation-nats-acl-preflight.py"
)
SUBJECT = "qintopia.qiwe.raw.authenticated"
STREAM = "QINTOPIA_QIWE_MESSAGES"
PRODUCER_USER = "fixture-producer"
PRODUCER_PASSWORD = "fixture-producer-secret"
CONSUMER_USER = "fixture-consumer"
CONSUMER_PASSWORD = "fixture-consumer-secret"


class FakeNatsServer(socketserver.ThreadingMixIn, socketserver.TCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, address: tuple[str, int], mode: dict[str, bool]) -> None:
        super().__init__(address, FakeNatsHandler)
        self.mode = mode
        self.subscribers: list[tuple["FakeNatsHandler", str, str]] = []
        self.subscriber_lock = threading.Lock()

    def register(self, handler: "FakeNatsHandler", subject: str, sid: str) -> None:
        with self.subscriber_lock:
            self.subscribers.append((handler, subject, sid))

    def unregister(self, handler: "FakeNatsHandler") -> None:
        with self.subscriber_lock:
            self.subscribers = [
                subscription
                for subscription in self.subscribers
                if subscription[0] is not handler
            ]

    def deliver_probe(self, body: bytes) -> None:
        with self.subscriber_lock:
            subscriptions = list(self.subscribers)
        for handler, subject, sid in subscriptions:
            if subject == SUBJECT:
                handler.send_hmsg(subject, sid, body)


class FakeNatsHandler(socketserver.StreamRequestHandler):
    server: FakeNatsServer

    def setup(self) -> None:
        super().setup()
        self.principal: str | None = None
        self.subscriptions: dict[str, str] = {}
        self.write_lock = threading.Lock()

    def finish(self) -> None:
        self.server.unregister(self)
        super().finish()

    def send_bytes(self, value: bytes) -> None:
        with self.write_lock:
            try:
                self.wfile.write(value)
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, OSError):
                return

    def send_msg(self, subject: str, sid: str, body: bytes) -> None:
        self.send_bytes(
            f"MSG {subject} {sid} {len(body)}\r\n".encode("ascii")
            + body
            + b"\r\n"
        )

    def send_hmsg(self, subject: str, sid: str, body: bytes) -> None:
        headers = b"NATS/1.0\r\nContent-Type: application/json\r\n\r\n"
        self.send_bytes(
            (
                f"HMSG {subject} {sid} {len(headers)} "
                f"{len(headers) + len(body)}\r\n"
            ).encode("ascii")
            + headers
            + body
            + b"\r\n"
        )

    def read_payload(self, size: int) -> bytes:
        value = self.rfile.read(size)
        if value is None or len(value) != size:
            raise ConnectionError
        return value

    def authenticate(self, payload: dict[str, Any]) -> bool:
        user = payload.get("user")
        password = payload.get("pass")
        if user == PRODUCER_USER and password == PRODUCER_PASSWORD:
            self.principal = "producer"
            return True
        if user == CONSUMER_USER and password == CONSUMER_PASSWORD:
            self.principal = "consumer"
            return True
        if user is None and password is None:
            if self.server.mode.get("anonymous_connect_denied", False):
                return False
            self.principal = "anonymous"
            return True
        return False

    def subscription_allowed(self, subject: str) -> bool:
        if self.principal == "producer":
            return subject.startswith("_INBOX.qintopia.acl.")
        if self.principal == "consumer":
            if subject.startswith("_INBOX.qintopia.acl.api."):
                return not self.server.mode.get("consumer_inbox_denied", False)
            return subject == SUBJECT and not self.server.mode.get(
                "consumer_subscribe_denied", False
            )
        return False

    def publish_allowed(self) -> bool:
        if self.principal == "producer":
            return True
        if self.principal == "consumer":
            return self.server.mode.get("consumer_publish_allowed", False)
        if self.principal == "anonymous":
            return self.server.mode.get("anonymous_publish_allowed", False)
        return False

    def valid_probe(self, body: bytes) -> bool:
        try:
            value = json.loads(body)
        except (UnicodeDecodeError, json.JSONDecodeError):
            return False
        return (
            isinstance(value, dict)
            and set(value) == {"event_id", "payload", "source"}
            and isinstance(value.get("event_id"), str)
            and value["event_id"].startswith("qintopia-nats-acl-probe-")
            and value.get("source") == "qintopia_nats_acl_preflight"
            and value.get("payload")
            == {"probe_type": "nats_acl_v1", "space_scoped": False}
        )

    def send_api_response(self, reply: str, body: dict[str, Any]) -> None:
        sid = self.subscriptions.get(reply)
        if sid is None:
            self.send_bytes(b"-ERR 'Missing Reply Subscription'\r\n")
            return
        payload = json.dumps(body, separators=(",", ":")).encode("ascii")
        self.send_msg(reply, sid, payload)

    def handle_plain_publish(self, parts: list[bytes]) -> bool:
        try:
            if len(parts) == 3:
                subject = parts[1].decode("ascii")
                reply = None
            elif len(parts) == 4:
                subject = parts[1].decode("ascii")
                reply = parts[2].decode("ascii")
            else:
                return False
            size = int(parts[-1])
            if size < 0 or size > 1_048_576:
                return False
            frame = self.read_payload(size + 2)
            if not frame.endswith(b"\r\n"):
                return False
        except (UnicodeDecodeError, ValueError, ConnectionError):
            return False

        api_modes = {
            f"$JS.API.STREAM.INFO.{STREAM}": "stream_info_denied",
            f"$JS.API.CONSUMER.INFO.{STREAM}.qintopia-message-sidecar": "consumer_info_denied",
            f"$JS.API.CONSUMER.CREATE.{STREAM}.qintopia-message-sidecar": "consumer_create_denied",
            f"$JS.API.CONSUMER.MSG.NEXT.{STREAM}.qintopia-message-sidecar": "consumer_next_denied",
        }
        if subject in api_modes:
            if self.principal != "consumer" or self.server.mode.get(
                api_modes[subject], False
            ):
                self.send_bytes(b"-ERR 'Permissions Violation for Publish'\r\n")
                return True
            if reply is None:
                return False
            if subject == f"$JS.API.STREAM.INFO.{STREAM}":
                subjects = (
                    ["qintopia.qiwe.raw", "qintopia.qiwe.message"]
                    if self.server.mode.get("stream_subject_missing", False)
                    else ["qintopia.qiwe.>"]
                )
                self.send_api_response(
                    reply,
                    {
                        "config": {"name": STREAM, "subjects": subjects},
                        "state": {"messages": 0},
                    },
                )
                return True
            if subject == (
                f"$JS.API.CONSUMER.INFO.{STREAM}.qintopia-message-sidecar"
            ):
                filters = [
                    "qintopia.qiwe.raw",
                    "qintopia.qiwe.message",
                    SUBJECT,
                ]
                if self.server.mode.get("consumer_filter_missing", False):
                    filters.pop()
                self.send_api_response(
                    reply,
                    {
                        "stream_name": STREAM,
                        "name": "qintopia-message-sidecar",
                        "config": {
                            "durable_name": "qintopia-message-sidecar",
                            "ack_policy": "explicit",
                            "filter_subjects": filters,
                        },
                    },
                )
                return True
            self.send_api_response(
                reply,
                {"error": {"code": 400, "description": "synthetic invalid request"}},
            )
            return True

        if subject.startswith(f"$JS.ACK.{STREAM}.qintopia-message-sidecar."):
            if self.principal != "consumer" or self.server.mode.get(
                "consumer_ack_denied", False
            ):
                self.send_bytes(b"-ERR 'Permissions Violation for Publish'\r\n")
            return True

        self.send_bytes(b"-ERR 'Permissions Violation for Publish'\r\n")
        return True

    def handle_publish(self, parts: list[bytes]) -> bool:
        try:
            if len(parts) == 4:
                subject = parts[1].decode("ascii")
                reply = None
            elif len(parts) == 5:
                subject = parts[1].decode("ascii")
                reply = parts[2].decode("ascii")
            else:
                return False
            header_bytes = int(parts[-2])
            total_bytes = int(parts[-1])
            if header_bytes < 0 or total_bytes < header_bytes or total_bytes > 1_048_576:
                return False
            frame = self.read_payload(total_bytes + 2)
            if not frame.endswith(b"\r\n"):
                return False
            body = frame[header_bytes:total_bytes]
        except (UnicodeDecodeError, ValueError, ConnectionError):
            return False

        if subject != SUBJECT or not self.publish_allowed():
            self.send_bytes(b"-ERR 'Permissions Violation for Publish'\r\n")
            return True
        if not self.valid_probe(body):
            self.send_bytes(b"-ERR 'Invalid Probe'\r\n")
            return True

        self.server.deliver_probe(body)
        if reply is None:
            return True
        if self.server.mode.get("close_before_ack", False):
            return False
        sid = self.subscriptions.get(reply)
        if sid is None:
            self.send_bytes(b"-ERR 'Missing Reply Subscription'\r\n")
            return True
        acknowledgement: dict[str, Any] = {
            "stream": (
                "WRONG_STREAM"
                if self.server.mode.get("wrong_stream_ack", False)
                else STREAM
            ),
            "seq": 1,
        }
        if self.server.mode.get("malformed_ack", False):
            acknowledgement["seq"] = 0
        body = json.dumps(acknowledgement, separators=(",", ":")).encode("ascii")
        self.send_msg(reply, sid, body)
        return True

    def handle(self) -> None:
        self.send_bytes(b'INFO {"headers":true,"jetstream":true}\r\n')
        while True:
            try:
                line = self.rfile.readline(16_385)
            except (ConnectionResetError, OSError):
                return
            if not line or len(line) > 16_384 or not line.endswith(b"\r\n"):
                return
            parts = line.rstrip(b"\r\n").split()
            if not parts:
                return
            command = parts[0]
            if command == b"CONNECT":
                try:
                    payload = json.loads(line[len(b"CONNECT ") : -2])
                except (UnicodeDecodeError, json.JSONDecodeError):
                    return
                if not isinstance(payload, dict) or not self.authenticate(payload):
                    self.send_bytes(b"-ERR 'Authorization Violation'\r\n")
                    return
            elif command == b"PING":
                if self.principal is None:
                    self.send_bytes(b"-ERR 'Authorization Violation'\r\n")
                    return
                self.send_bytes(b"PONG\r\n")
            elif command == b"PONG":
                continue
            elif command == b"SUB":
                if len(parts) != 3:
                    return
                try:
                    subject = parts[1].decode("ascii")
                    sid = parts[2].decode("ascii")
                except UnicodeDecodeError:
                    return
                if not self.subscription_allowed(subject):
                    self.send_bytes(b"-ERR 'Permissions Violation for Subscription'\r\n")
                    continue
                self.subscriptions[subject] = sid
                self.server.register(self, subject, sid)
            elif command == b"UNSUB":
                continue
            elif command == b"HPUB":
                if not self.handle_publish(parts):
                    return
            elif command == b"PUB":
                if not self.handle_plain_publish(parts):
                    return
            else:
                return


class NatsAclPreflightTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory(
            prefix="qintopia-nats-acl-preflight-"
        )
        self.root = Path(self.temporary_directory.name)
        self.producer_auth_file = self.root / "producer.json"
        self.consumer_auth_file = self.root / "consumer.json"
        self.write_auth(
            self.producer_auth_file, PRODUCER_USER, PRODUCER_PASSWORD
        )
        self.write_auth(
            self.consumer_auth_file, CONSUMER_USER, CONSUMER_PASSWORD
        )

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    @staticmethod
    def write_auth(path: Path, username: str, password: str) -> None:
        path.write_text(
            json.dumps(
                {"version": 1, "username": username, "password": password},
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )
        path.chmod(stat.S_IRUSR | stat.S_IWUSR)

    def run_preflight(
        self,
        mode: dict[str, bool] | None = None,
        *,
        explicit_test_mode: bool = True,
        producer_auth_file: Path | None = None,
        consumer_auth_file: Path | None = None,
    ) -> subprocess.CompletedProcess[str]:
        server = FakeNatsServer(("127.0.0.1", 0), mode or {})
        server_thread = threading.Thread(target=server.serve_forever, daemon=True)
        server_thread.start()
        try:
            environment = {
                "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
                "PYTHONDONTWRITEBYTECODE": "1",
                "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_PORT": str(
                    server.server_address[1]
                ),
                "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_PRODUCER_AUTH_FILE": str(
                    producer_auth_file or self.producer_auth_file
                ),
                "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_CONSUMER_AUTH_FILE": str(
                    consumer_auth_file or self.consumer_auth_file
                ),
            }
            if explicit_test_mode:
                environment[
                    "QINTOPIA_SPACE_AUTOMATION_NATS_ACL_PREFLIGHT_TEST_MODE"
                ] = "1"
            return subprocess.run(
                [sys.executable, str(PREFLIGHT)],
                cwd=REPO_ROOT,
                env=environment,
                capture_output=True,
                text=True,
                timeout=12,
                check=False,
            )
        finally:
            server.shutdown()
            server.server_close()
            server_thread.join(timeout=2)

    def assert_sanitized_output(
        self, result: subprocess.CompletedProcess[str]
    ) -> None:
        output = f"{result.stdout}\n{result.stderr}"
        for forbidden in (
            PRODUCER_USER,
            PRODUCER_PASSWORD,
            CONSUMER_USER,
            CONSUMER_PASSWORD,
            "nats://",
            SUBJECT,
            str(self.producer_auth_file),
            str(self.consumer_auth_file),
        ):
            self.assertNotIn(forbidden, output)

    def assert_failed(self, result: subprocess.CompletedProcess[str]) -> None:
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Space automation NATS ACL preflight failed", result.stderr)
        self.assertNotIn("space_automation_nats_acl_preflight=passed", result.stdout)
        self.assert_sanitized_output(result)

    def test_proves_expected_acl_and_puback(self) -> None:
        result = self.run_preflight()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout.strip(), "space_automation_nats_acl_preflight=passed"
        )
        self.assertEqual(result.stderr, "")
        self.assert_sanitized_output(result)

    def test_rejects_consumer_publish_permission(self) -> None:
        self.assert_failed(
            self.run_preflight({"consumer_publish_allowed": True})
        )

    def test_rejects_anonymous_publish_permission(self) -> None:
        self.assert_failed(
            self.run_preflight({"anonymous_publish_allowed": True})
        )

    def test_rejects_missing_consumer_subscription_permission(self) -> None:
        self.assert_failed(
            self.run_preflight({"consumer_subscribe_denied": True})
        )

    def test_rejects_missing_jetstream_api_or_ack_permissions(self) -> None:
        for mode in (
            "consumer_inbox_denied",
            "stream_info_denied",
            "consumer_info_denied",
            "consumer_create_denied",
            "consumer_next_denied",
            "consumer_ack_denied",
        ):
            with self.subTest(mode=mode):
                self.assert_failed(self.run_preflight({mode: True}))

    def test_rejects_stream_or_consumer_without_trusted_subject(self) -> None:
        self.assert_failed(self.run_preflight({"stream_subject_missing": True}))
        self.assert_failed(self.run_preflight({"consumer_filter_missing": True}))

    def test_rejects_missing_or_wrong_puback(self) -> None:
        self.assert_failed(self.run_preflight({"close_before_ack": True}))
        self.assert_failed(self.run_preflight({"wrong_stream_ack": True}))
        self.assert_failed(self.run_preflight({"malformed_ack": True}))

    def test_rejects_shared_principal_and_unknown_auth_fields(self) -> None:
        self.write_auth(
            self.consumer_auth_file, PRODUCER_USER, "different-fixture-password"
        )
        self.assert_failed(self.run_preflight())

        self.consumer_auth_file.write_text(
            '{"version":1,"username":"fixture-consumer",'
            '"password":"fixture-consumer-secret","extra":true}',
            encoding="utf-8",
        )
        self.consumer_auth_file.chmod(stat.S_IRUSR | stat.S_IWUSR)
        self.assert_failed(self.run_preflight())

    def test_rejects_test_overrides_without_explicit_test_mode(self) -> None:
        self.assert_failed(self.run_preflight(explicit_test_mode=False))


if __name__ == "__main__":
    unittest.main(verbosity=2)
