import asyncio
import json
import os
import re
import tempfile
import unittest
from unittest.mock import patch

import nats_capture as nats_capture_module
from nats_capture import QiWeNatsCaptureConfig, QiWeNatsPublisher


class FakeNatsWriter:
    def __init__(self, reader, acknowledgement):
        self.reader = reader
        self.acknowledgement = acknowledgement
        self.buffer = bytearray()
        self.acknowledgement_sent = False
        self.closed = False

    def write(self, data):
        self.buffer.extend(data)

    async def drain(self):
        if self.acknowledgement_sent or b"HPUB " not in self.buffer:
            return
        self.acknowledgement_sent = True
        match = re.search(rb"SUB (_INBOX\.[^ ]+) ([^\r]+)\r\n", self.buffer)
        if match is None or self.acknowledgement is None:
            return
        if isinstance(self.acknowledgement, bytes):
            self.reader.feed_data(self.acknowledgement)
            return
        body = json.dumps(self.acknowledgement, separators=(",", ":")).encode(
            "utf-8"
        )
        self.reader.feed_data(
            b"MSG "
            + match.group(1)
            + b" "
            + match.group(2)
            + b" "
            + str(len(body)).encode("ascii")
            + b"\r\n"
            + body
            + b"\r\n"
        )

    def close(self):
        self.closed = True

    async def wait_closed(self):
        return None


def fake_connection(acknowledgement):
    reader = asyncio.StreamReader()
    reader.feed_data(b"INFO {}\r\n")
    writer = FakeNatsWriter(reader, acknowledgement)

    async def open_connection(_host, _port):
        return reader, writer

    return open_connection, writer


class DurableNatsCaptureTests(unittest.TestCase):
    def publisher(self, *, timeout_seconds=0.05):
        with tempfile.TemporaryDirectory() as directory:
            auth_file = os.path.join(directory, "producer.json")
            with open(auth_file, "w", encoding="utf-8") as handle:
                json.dump(
                    {
                        "version": 1,
                        "username": "qiwe-producer",
                        "password": "synthetic-producer-secret",
                    },
                    handle,
                )
            os.chmod(auth_file, 0o600)
            return QiWeNatsPublisher(
                QiWeNatsCaptureConfig(
                    enabled=True,
                    auth_file=auth_file,
                    timeout_seconds=timeout_seconds,
                )
            )

    def test_raw_publish_waits_for_valid_jetstream_ack(self):
        async def run_case():
            open_connection, writer = fake_connection(
                {"stream": "QINTOPIA_QIWE_MESSAGES", "seq": 42}
            )
            with patch.object(
                nats_capture_module.asyncio,
                "open_connection",
                open_connection,
            ):
                await self.publisher().publish_raw_durable(
                    {"event_id": "event-1"}, message_id="event-1"
                )
            return writer

        writer = asyncio.run(run_case())
        published = bytes(writer.buffer)
        self.assertIn(b"SUB _INBOX.qintopia.", published)
        self.assertRegex(
            published,
            rb"HPUB qintopia\.qiwe\.raw\.authenticated _INBOX\.qintopia\.[0-9a-f]{32} ",
        )
        self.assertIn(b"Nats-Msg-Id: raw:event-1\r\n", published)
        self.assertIn(b"Qintopia-Event-Type: authenticated_raw\r\n", published)
        self.assertIn(b'"user":"qiwe-producer"', published)
        self.assertIn(b'"pass":"synthetic-producer-secret"', published)
        self.assertTrue(writer.closed)

    def test_nats_url_rejects_embedded_credentials(self):
        with self.assertRaisesRegex(ValueError, "userinfo is forbidden"):
            QiWeNatsPublisher(
                QiWeNatsCaptureConfig(
                    enabled=True,
                    url="nats://user:secret@127.0.0.1:4222",
                )
            )

    def test_durable_publish_requires_file_credentials(self):
        async def run_case():
            publisher = QiWeNatsPublisher(QiWeNatsCaptureConfig(enabled=True))
            await publisher.publish_raw_durable(
                {"event_id": "missing-auth"}, message_id="missing-auth"
            )

        with self.assertRaisesRegex(RuntimeError, "credentials are required"):
            asyncio.run(run_case())

    def test_raw_publish_rejects_jetstream_error_ack(self):
        async def run_case():
            open_connection, _ = fake_connection(
                {"error": {"code": 503, "description": "stream unavailable"}}
            )
            with patch.object(
                nats_capture_module.asyncio,
                "open_connection",
                open_connection,
            ):
                await self.publisher().publish_raw_durable(
                    {"event_id": "event-2"}, message_id="event-2"
                )

        with self.assertRaisesRegex(RuntimeError, "JetStream rejected"):
            asyncio.run(run_case())

    def test_raw_publish_rejects_ack_without_stream_sequence(self):
        async def run_case():
            open_connection, _ = fake_connection(
                {"stream": "QINTOPIA_QIWE_MESSAGES"}
            )
            with patch.object(
                nats_capture_module.asyncio,
                "open_connection",
                open_connection,
            ):
                await self.publisher().publish_raw_durable(
                    {"event_id": "event-3"}, message_id="event-3"
                )

        with self.assertRaisesRegex(RuntimeError, "invalid JetStream"):
            asyncio.run(run_case())

    def test_raw_publish_times_out_without_ack(self):
        async def run_case():
            open_connection, _ = fake_connection(None)
            with patch.object(
                nats_capture_module.asyncio,
                "open_connection",
                open_connection,
            ):
                await self.publisher(timeout_seconds=0.01).publish_raw_durable(
                    {"event_id": "event-4"}, message_id="event-4"
                )

        with self.assertRaises(asyncio.TimeoutError):
            asyncio.run(run_case())


if __name__ == "__main__":
    unittest.main()
