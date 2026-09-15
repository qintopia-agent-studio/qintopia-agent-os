import asyncio
import json
import os
import unittest
from unittest.mock import patch

import adapter as adapter_module
from adapter import QiWeAdapter, parse_qiwe_payloads
from image_callback_bridge import classify_async_image_callback
from nats_capture import StrictJsonError, parse_strict_bounded_json


ROOM_ID = "7000000000000001"
SENDER_ID = "8000000000000001"
BOT_ID = "9000000000000001"


class FakeWeb:
    @staticmethod
    def json_response(data, status=200):
        return type(
            "Response",
            (),
            {"status": status, "text": json.dumps(data, ensure_ascii=False)},
        )()


class FakeRequest:
    def __init__(self, body, *, headers=None, fail_on_read=False):
        self.body = body
        self.headers = headers or {}
        self.fail_on_read = fail_on_read
        self.read_called = False

    async def read(self):
        self.read_called = True
        if self.fail_on_read:
            raise AssertionError("unauthenticated webhook body must not be read")
        return self.body


def text_event(message_id, *, content="@二花 hello", room_id=ROOM_ID):
    return {
        "cmd": 15000,
        "newMsgType": "TEXT",
        "msgType": 0,
        "msgUniqueIdentifier": message_id,
        "fromRoomId": room_id,
        "senderId": SENDER_ID,
        "receiverId": BOT_ID,
        "timestamp": 1_787_000_000,
        "msgData": {
            "content": content,
            "atList": [{"userId": BOT_ID, "nickname": "二花"}],
        },
    }


def member_event(message_id, *, cmd, version):
    event = {
        "cmd": cmd,
        "msgUniqueIdentifier": message_id,
        "fromRoomId": ROOM_ID,
        "senderId": SENDER_ID,
        "timestamp": 1_787_000_000,
        "msgData": {
            "changedMemberList": "ODEwMDAwMDAwMDAwMDAwMQ==",
        },
    }
    if version == "v1":
        event["msgType"] = 1002
    else:
        event["newMsgType"] = "GROUP_MEMBER_ADD"
    return event


def envelope(events):
    return {
        "eventCode": "group_msg_event",
        "fromGroup": ROOM_ID,
        "data": events,
    }


def encoded(payload):
    return json.dumps(payload, ensure_ascii=False).encode("utf-8")


def callback_event():
    return {
        "requestId": "synthetic-callback-request",
        "cmd": 20000,
        "msgData": {
            "fileAesKey": "synthetic-aes-key",
            "fileId": "synthetic-file-id",
            "fileMd5": "00000000000000000000000000000000",
            "fileSize": 123,
        },
    }


class WebhookEnvelopeTests(unittest.TestCase):
    def setUp(self):
        self._saved_auth_env = {
            name: os.environ.get(name)
            for name in (
                "QIWE_WEBHOOK_AUTH_REQUIRED",
                "QIWE_WEBHOOK_AUTH_TOKEN",
                "QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED",
            )
        }
        os.environ.pop("QIWE_WEBHOOK_AUTH_REQUIRED", None)
        os.environ.pop("QIWE_WEBHOOK_AUTH_TOKEN", None)
        os.environ.pop("QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED", None)

    def tearDown(self):
        for name, value in self._saved_auth_env.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value

    def test_parse_qiwe_payloads_processes_every_data_item_in_order(self):
        parsed = parse_qiwe_payloads(
            envelope([text_event("event-1"), text_event("event-2")]),
            bot_names=["二花"],
            bot_user_id=BOT_ID,
        )

        self.assertEqual([item.message_id for item in parsed], ["event-1", "event-2"])
        self.assertTrue(all(item.should_trigger for item in parsed))
        self.assertEqual(parsed[0].payload["data"]["msgUniqueIdentifier"], "event-1")
        self.assertEqual(parsed[1].payload["data"]["msgUniqueIdentifier"], "event-2")

    def test_parse_qiwe_payloads_rejects_oversized_or_malformed_arrays(self):
        with self.assertRaises(ValueError):
            parse_qiwe_payloads(envelope([text_event(str(index)) for index in range(65)]))
        with self.assertRaises(ValueError):
            parse_qiwe_payloads(envelope([]))
        with self.assertRaises(ValueError):
            parse_qiwe_payloads(envelope([text_event("valid"), "not-an-object"]))
        with self.assertRaises(ValueError):
            parse_qiwe_payloads([envelope([text_event("event")])])

    def test_member_add_v1_v2_accept_both_documented_cmd_values(self):
        for version in ("v1", "v2"):
            for cmd in (15000, 15500):
                with self.subTest(version=version, cmd=cmd):
                    parsed = parse_qiwe_payloads(
                        envelope([member_event(f"{version}-{cmd}", cmd=cmd, version=version)])
                    )[0]
                    self.assertTrue(parsed.accepted)
                    self.assertFalse(parsed.should_trigger)
                    self.assertEqual(parsed.message_kind, "system")
                    self.assertEqual(parsed.reason, "non_text_system")

    def test_large_numeric_ids_remain_exact_strings(self):
        event = text_event(9_007_199_254_740_993, room_id=7_000_000_000_000_001)
        event["senderId"] = 8_000_000_000_000_001
        payload = envelope([event])
        payload["fromGroup"] = 7_000_000_000_000_001

        parsed = parse_qiwe_payloads(payload, bot_names=["二花"], bot_user_id=BOT_ID)[0]

        self.assertEqual(parsed.group_id, "7000000000000001")
        self.assertEqual(parsed.sender_id, "8000000000000001")
        self.assertEqual(parsed.message_id, "9007199254740993")

    def test_strict_json_accepts_finite_floats_and_preserves_large_integers(self):
        parsed = parse_strict_bounded_json(
            b'{"latitude":31.2304,"opaque_id":9007199254740993123456789}'
        )

        self.assertEqual(parsed["latitude"], 31.2304)
        self.assertEqual(parsed["opaque_id"], 9_007_199_254_740_993_123_456_789)
        for token in (b"NaN", b"Infinity", b"-Infinity"):
            with self.subTest(token=token):
                with self.assertRaises(StrictJsonError):
                    parse_strict_bounded_json(b'{"coordinate":' + token + b"}")

    def test_required_auth_rejects_missing_or_wrong_header_before_body_read(self):
        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = QiWeAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "token": "synthetic-api-token",
                            "webhook_auth_required": True,
                            "webhook_auth_token": "synthetic-ingress-token",
                            "send_enabled": False,
                        }
                    },
                )()
            )
            try:
                missing = FakeRequest(b"secret body", fail_on_read=True)
                wrong = FakeRequest(
                    b"secret body",
                    headers={
                        "X-Qintopia-Qiwe-Ingress-Auth": "wrong-token"
                    },
                    fail_on_read=True,
                )
                missing_response = await adapter._handle_webhook(missing)
                wrong_response = await adapter._handle_webhook(wrong)
            finally:
                adapter_module.web = old_web
            return missing, wrong, missing_response, wrong_response

        missing, wrong, missing_response, wrong_response = asyncio.run(run_case())
        self.assertFalse(missing.read_called)
        self.assertFalse(wrong.read_called)
        self.assertEqual(missing_response.status, 401)
        self.assertEqual(wrong_response.status, 401)
        self.assertNotIn("synthetic-ingress-token", missing_response.text)
        self.assertNotIn("synthetic-ingress-token", wrong_response.text)

    def test_required_auth_with_empty_config_rejects_before_body_read(self):
        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = QiWeAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "token": "synthetic-api-token",
                            "webhook_auth_required": True,
                            "webhook_auth_token": "",
                        }
                    },
                )()
            )
            request = FakeRequest(b"secret body", fail_on_read=True)
            try:
                response = await adapter._handle_webhook(request)
            finally:
                adapter_module.web = old_web
            return request, response

        request, response = asyncio.run(run_case())
        self.assertFalse(request.read_called)
        self.assertEqual(response.status, 401)

    def test_correct_auth_is_accepted_and_default_remains_disabled(self):
        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            try:
                secured = QiWeAdapter(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "token": "synthetic-api-token",
                                "webhook_auth_required": True,
                                "webhook_auth_token": "synthetic-ingress-token",
                                "send_enabled": False,
                            }
                        },
                    )()
                )
                secured_request = FakeRequest(
                    encoded(envelope([member_event("secured", cmd=15000, version="v2")])),
                    headers={
                        "X-Qintopia-Qiwe-Ingress-Auth": "synthetic-ingress-token"
                    },
                )
                secured_response = await secured._handle_webhook(secured_request)

                compatible = QiWeAdapter(
                    type("Config", (), {"extra": {"token": "synthetic-api-token"}})()
                )
                compatible_request = FakeRequest(
                    encoded(envelope([member_event("compatible", cmd=15000, version="v1")]))
                )
                compatible_response = await compatible._handle_webhook(compatible_request)
            finally:
                adapter_module.web = old_web
            return secured_request, compatible_request, secured_response, compatible_response

        secured_request, compatible_request, secured_response, compatible_response = asyncio.run(
            run_case()
        )
        self.assertTrue(secured_request.read_called)
        self.assertTrue(compatible_request.read_called)
        self.assertEqual(secured_response.status, 200)
        self.assertEqual(compatible_response.status, 200)

    def test_duplicate_keys_fail_before_callback_classification_or_nats(self):
        class NoSideEffectsAdapter(QiWeAdapter):
            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                raise AssertionError("ambiguous JSON must not enter NATS")

            def _schedule_image_callback_processing(self, body):
                raise AssertionError("ambiguous JSON must not enter callback processing")

        ordinary_duplicate = (
            '{"eventCode":"group_msg_event","fromGroup":"%s","data":[{'
            '"cmd":15000,"newMsgType":"TEXT","msgUniqueIdentifier":"duplicate",'
            '"fromRoomId":"%s","fromRoomId":"other-room","senderId":"%s",'
            '"receiverId":"%s","timestamp":1787000000,'
            '"msgData":{"content":"hello"}}]}'
            % (ROOM_ID, ROOM_ID, SENDER_ID, BOT_ID)
        ).encode("utf-8")
        nested_duplicate = encoded(
            {
                "eventCode": "group_msg_event",
                "fromGroup": ROOM_ID,
                "data": (
                    '{"cmd":15000,"newMsgType":"TEXT",'
                    '"msgUniqueIdentifier":"nested-duplicate",'
                    f'"fromRoomId":"{ROOM_ID}","fromRoomId":"other-room",'
                    f'"senderId":"{SENDER_ID}","receiverId":"{BOT_ID}",'
                    '"timestamp":1787000000,"msgData":{"content":"hello"}}'
                ),
            }
        )
        callback_duplicate = (
            b'{"code":0,"data":[{"requestId":"callback-duplicate","cmd":20000,'
            b'"msgData":{"fileAesKey":"aes","fileId":"first","fileId":"second",'
            b'"fileMd5":"00000000000000000000000000000000","fileSize":123}}]}'
        )

        async def run_case(body):
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = NoSideEffectsAdapter(
                type(
                    "Config",
                    (),
                    {"extra": {"token": "synthetic-api-token", "send_enabled": False}},
                )()
            )
            try:
                return await adapter._handle_webhook(FakeRequest(body))
            finally:
                adapter_module.web = old_web

        for body in (ordinary_duplicate, nested_duplicate, callback_duplicate):
            with self.subTest(body=body[:48]):
                response = asyncio.run(run_case(body))
                self.assertEqual(response.status, 400)
                self.assertIn("invalid_envelope", response.text)
        self.assertEqual(classify_async_image_callback(callback_duplicate), "none")

    def test_connect_fails_closed_when_required_auth_token_is_missing(self):
        class RecordingAdapter(QiWeAdapter):
            def __init__(self):
                self.fatal_errors = []
                super().__init__(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "token": "synthetic-api-token",
                                "webhook_auth_required": True,
                                "webhook_auth_token": "",
                            }
                        },
                    )()
                )

            def _set_fatal_error(self, code, message, *, retryable):
                self.fatal_errors.append((code, message, retryable))

        adapter = RecordingAdapter()
        self.assertFalse(asyncio.run(adapter.connect()))
        self.assertEqual(adapter.fatal_errors[0][0], "config_missing")
        self.assertFalse(adapter.fatal_errors[0][2])

    def test_authenticated_system_batch_waits_for_every_durable_raw_ack(self):
        class BlockingPublisher:
            def __init__(self):
                self.calls = []
                self.all_started = asyncio.Event()
                self.release = asyncio.Event()

            async def publish_raw_durable(self, raw_event, *, message_id):
                self.calls.append((raw_event, message_id))
                if len(self.calls) == 2:
                    self.all_started.set()
                await self.release.wait()

        class StrictAdapter(QiWeAdapter):
            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                return None

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            publisher = BlockingPublisher()
            adapter = StrictAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "token": "synthetic-api-token",
                            "send_enabled": False,
                            "webhook_auth_required": True,
                            "webhook_auth_token": "synthetic-ingress-token",
                            "nats_capture_enabled": True,
                            "system_event_durable_capture_enabled": True,
                        }
                    },
                )()
            )
            adapter._nats_capture = publisher
            request = FakeRequest(
                encoded(
                    envelope(
                        [
                            member_event("durable-1", cmd=15000, version="v1"),
                            member_event("durable-2", cmd=15500, version="v2"),
                        ]
                    )
                ),
                headers={
                    "X-Qintopia-Qiwe-Ingress-Auth": "synthetic-ingress-token"
                },
            )
            try:
                handler = asyncio.create_task(adapter._handle_webhook(request))
                await asyncio.wait_for(publisher.all_started.wait(), timeout=0.1)
                self.assertFalse(handler.done())
                publisher.release.set()
                response = await handler
            finally:
                adapter_module.web = old_web
            return publisher, response

        publisher, response = asyncio.run(run_case())
        self.assertEqual(response.status, 200)
        self.assertEqual(
            [message_id for _, message_id in publisher.calls],
            ["durable-1", "durable-2"],
        )
        self.assertTrue(
            all(
                raw_event["ingress_auth_verified"] is False
                for raw_event, _ in publisher.calls
            )
        )

    def test_authenticated_system_batch_returns_bounded_503_when_one_ack_fails(self):
        class FailingPublisher:
            async def publish_raw_durable(self, _raw_event, *, message_id):
                if message_id == "durable-fail":
                    raise RuntimeError("synthetic NATS detail must not leak")

        class StrictAdapter(QiWeAdapter):
            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                raise AssertionError("failed durable batch must not schedule capture")

        async def run_case():
            old_web = adapter_module.web
            old_disabled = adapter_module.logger.disabled
            adapter_module.web = FakeWeb
            adapter_module.logger.disabled = True
            adapter = StrictAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "token": "synthetic-api-token",
                            "send_enabled": False,
                            "webhook_auth_required": True,
                            "webhook_auth_token": "synthetic-ingress-token",
                            "nats_capture_enabled": True,
                            "system_event_durable_capture_enabled": True,
                        }
                    },
                )()
            )
            adapter._nats_capture = FailingPublisher()
            try:
                return await adapter._handle_webhook(
                    FakeRequest(
                        encoded(
                            envelope(
                                [
                                    member_event(
                                        "durable-ok", cmd=15000, version="v1"
                                    ),
                                    member_event(
                                        "durable-fail", cmd=15500, version="v2"
                                    ),
                                ]
                            )
                        ),
                        headers={
                            "X-Qintopia-Qiwe-Ingress-Auth": "synthetic-ingress-token"
                        },
                    )
                )
            finally:
                adapter_module.web = old_web
                adapter_module.logger.disabled = old_disabled

        response = asyncio.run(run_case())
        self.assertEqual(response.status, 503)
        self.assertIn("system_event_capture_unavailable", response.text)
        self.assertNotIn("synthetic NATS detail", response.text)

    def test_authenticated_system_capture_uses_one_envelope_timeout_budget(self):
        class HangingPublisher:
            async def publish_raw_durable(self, _raw_event, *, message_id):
                await asyncio.Event().wait()

        class StrictAdapter(QiWeAdapter):
            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                raise AssertionError("timed-out durable batch must not schedule capture")

        async def run_case():
            old_web = adapter_module.web
            old_disabled = adapter_module.logger.disabled
            adapter_module.web = FakeWeb
            adapter_module.logger.disabled = True
            adapter = StrictAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "token": "synthetic-api-token",
                            "send_enabled": False,
                            "webhook_auth_required": True,
                            "webhook_auth_token": "synthetic-ingress-token",
                            "nats_capture_enabled": True,
                            "system_event_durable_capture_enabled": True,
                        }
                    },
                )()
            )
            adapter._nats_capture = HangingPublisher()
            try:
                with patch.object(
                    adapter_module,
                    "SYSTEM_EVENT_DURABLE_CAPTURE_TIMEOUT_SECONDS",
                    0.01,
                ):
                    return await adapter._handle_webhook(
                        FakeRequest(
                            encoded(
                                envelope(
                                    [
                                        member_event(
                                            "durable-timeout-1",
                                            cmd=15000,
                                            version="v1",
                                        ),
                                        member_event(
                                            "durable-timeout-2",
                                            cmd=15500,
                                            version="v2",
                                        ),
                                    ]
                                )
                            ),
                            headers={
                                "X-Qintopia-Qiwe-Ingress-Auth": "synthetic-ingress-token"
                            },
                        )
                    )
            finally:
                adapter_module.web = old_web
                adapter_module.logger.disabled = old_disabled

        response = asyncio.run(run_case())
        self.assertEqual(response.status, 503)
        self.assertIn("system_event_capture_unavailable", response.text)

    def test_strict_mode_keeps_authenticated_text_capture_best_effort(self):
        class RejectDurablePublisher:
            async def publish_raw_durable(self, _raw_event, *, message_id):
                raise AssertionError("ordinary messages must not wait for durable capture")

        class StrictAdapter(QiWeAdapter):
            def __init__(self):
                super().__init__(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "token": "synthetic-api-token",
                                "send_enabled": False,
                                "webhook_auth_required": True,
                                "webhook_auth_token": "synthetic-ingress-token",
                                "nats_capture_enabled": True,
                                "system_event_durable_capture_enabled": True,
                            }
                        },
                    )()
                )
                self.best_effort_scheduled = []

            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                self.best_effort_scheduled.append(parsed.message_id)

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = StrictAdapter()
            adapter._nats_capture = RejectDurablePublisher()
            ordinary = text_event("ordinary-strict", content="hello")
            ordinary["msgData"]["atList"] = []
            try:
                response = await adapter._handle_webhook(
                    FakeRequest(
                        encoded(envelope([ordinary])),
                        headers={
                            "X-Qintopia-Qiwe-Ingress-Auth": "synthetic-ingress-token"
                        },
                    )
                )
                await asyncio.sleep(0)
            finally:
                adapter_module.web = old_web
            return adapter, response

        adapter, response = asyncio.run(run_case())
        self.assertEqual(response.status, 200)
        self.assertEqual(adapter.best_effort_scheduled, ["ordinary-strict"])

    def test_durable_capture_enable_rejects_ambiguous_flag(self):
        with self.assertRaises(ValueError):
            QiWeAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "system_event_durable_capture_enabled": "true",
                        }
                    },
                )()
            )

    def test_auth_required_rejects_ambiguous_boolean_config(self):
        with self.assertRaises(ValueError):
            QiWeAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "token": "synthetic-api-token",
                            "webhook_auth_required": "yes",
                            "webhook_auth_token": "synthetic-ingress-token",
                        }
                    },
                )()
            )

    def test_batch_handler_captures_and_dispatches_each_item_independently(self):
        class RecordingAdapter(QiWeAdapter):
            def __init__(self):
                super().__init__(
                    type(
                        "Config",
                        (),
                        {"extra": {"token": "synthetic-api-token", "send_enabled": False}},
                    )()
                )
                self.captures = []
                self.dispatched = []

            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                self.captures.append(
                    (parsed.message_id, json.loads(body), ingress_auth_verified)
                )

            async def _dispatch_message_safe(self, parsed):
                self.dispatched.append(parsed.message_id)

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = RecordingAdapter()
            try:
                response = await adapter._handle_webhook(
                    FakeRequest(encoded(envelope([text_event("event-1"), text_event("event-2")])))
                )
                await asyncio.sleep(0)
            finally:
                adapter_module.web = old_web
            return adapter, response

        adapter, response = asyncio.run(run_case())
        body = json.loads(response.text)
        self.assertEqual(response.status, 200)
        self.assertEqual(body["accepted_count"], 2)
        self.assertEqual(body["triggered_count"], 2)
        self.assertEqual(adapter.dispatched, ["event-1", "event-2"])
        self.assertEqual([item[0] for item in adapter.captures], ["event-1", "event-2"])
        self.assertTrue(all(isinstance(item[1]["data"], dict) for item in adapter.captures))
        self.assertTrue(all(item[2] is False for item in adapter.captures))

    def test_only_successfully_authenticated_request_marks_nats_capture(self):
        class RecordingAdapter(QiWeAdapter):
            def __init__(self, *, auth_required):
                super().__init__(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "token": "synthetic-api-token",
                                "send_enabled": False,
                                "webhook_auth_required": auth_required,
                                "webhook_auth_token": "synthetic-ingress-token",
                            }
                        },
                    )()
                )
                self.auth_facts = []

            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                self.auth_facts.append(ingress_auth_verified)

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            secured = RecordingAdapter(auth_required=True)
            compatible = RecordingAdapter(auth_required=False)
            try:
                secured_response = await secured._handle_webhook(
                    FakeRequest(
                        encoded(
                            envelope(
                                [
                                    member_event(
                                        "secured-auth-fact", cmd=15000, version="v2"
                                    )
                                ]
                            )
                        ),
                        headers={
                            "X-Qintopia-Qiwe-Ingress-Auth": "synthetic-ingress-token"
                        },
                    )
                )
                compatible_response = await compatible._handle_webhook(
                    FakeRequest(
                        encoded(
                            envelope(
                                [
                                    member_event(
                                        "compat-auth-fact", cmd=15000, version="v1"
                                    )
                                ]
                            )
                        )
                    )
                )
            finally:
                adapter_module.web = old_web
            return secured, compatible, secured_response, compatible_response

        secured, compatible, secured_response, compatible_response = asyncio.run(
            run_case()
        )
        self.assertEqual(secured_response.status, 200)
        self.assertEqual(compatible_response.status, 200)
        self.assertEqual(secured.auth_facts, [True])
        self.assertEqual(compatible.auth_facts, [False])

    def test_batch_handler_deduplicates_each_event_id(self):
        class RecordingAdapter(QiWeAdapter):
            def __init__(self):
                super().__init__(
                    type("Config", (), {"extra": {"token": "synthetic-api-token"}})()
                )
                self.dispatched = []

            async def _dispatch_message_safe(self, parsed):
                self.dispatched.append(parsed.message_id)

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = RecordingAdapter()
            try:
                response = await adapter._handle_webhook(
                    FakeRequest(encoded(envelope([text_event("same-id"), text_event("same-id")])))
                )
                await asyncio.sleep(0)
            finally:
                adapter_module.web = old_web
            return adapter, response

        adapter, response = asyncio.run(run_case())
        body = json.loads(response.text)
        self.assertEqual(body["triggered_count"], 1)
        self.assertEqual(body["results"][1]["reason"], "duplicate_message")
        self.assertEqual(adapter.dispatched, ["same-id"])

    def test_mixed_image_callback_envelope_fails_closed_without_side_effects(self):
        mixed = {
            "code": 0,
            "eventCode": "group_msg_event",
            "fromGroup": ROOM_ID,
            "data": [callback_event(), text_event("ordinary-event")],
        }

        class RejectingBridge:
            async def process(self, body):
                raise AssertionError("mixed envelope must not reach callback processor")

        class RecordingAdapter(QiWeAdapter):
            def __init__(self):
                super().__init__(
                    type("Config", (), {"extra": {"token": "synthetic-api-token"}})()
                )
                self.captures = []
                self._image_callback_bridge = RejectingBridge()

            def _schedule_nats_capture(
                self, parsed, body, *, ingress_auth_verified
            ):
                self.captures.append(parsed.message_id)

            async def _dispatch_message_safe(self, parsed):
                raise AssertionError("mixed envelope must not dispatch")

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = RecordingAdapter()
            try:
                response = await adapter._handle_webhook(FakeRequest(encoded(mixed)))
            finally:
                adapter_module.web = old_web
            return adapter, response

        adapter, response = asyncio.run(run_case())
        self.assertEqual(classify_async_image_callback(encoded(mixed)), "mixed")
        self.assertEqual(response.status, 503)
        self.assertIn("mixed_image_callback_envelope", response.text)
        self.assertEqual(adapter.captures, [])

    def test_all_image_callbacks_remain_classified_as_callback_only(self):
        payload = {"code": 0, "data": [callback_event(), callback_event()]}
        self.assertEqual(classify_async_image_callback(encoded(payload)), "all")

    def test_image_callback_ack_does_not_wait_for_background_processor(self):
        class SlowBridge:
            enabled = True
            configuration_valid = True

            def __init__(self):
                self.started = asyncio.Event()
                self.release = asyncio.Event()

            async def process(self, _body):
                self.started.set()
                await self.release.wait()
                return type(
                    "Result",
                    (),
                    {
                        "enabled": True,
                        "processed": True,
                        "reason": "processor_completed",
                        "action_status": "image_send_completed",
                        "callback_credential_schema": "fixture",
                        "callback_additional_field_count": 0,
                        "external_send_executed": True,
                    },
                )()

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = QiWeAdapter(
                type("Config", (), {"extra": {"token": "synthetic-api-token"}})()
            )
            bridge = SlowBridge()
            adapter._image_callback_bridge = bridge
            try:
                response = await asyncio.wait_for(
                    adapter._handle_webhook(
                        FakeRequest(encoded({"code": 0, "data": [callback_event()]}))
                    ),
                    timeout=0.1,
                )
                await asyncio.sleep(0)
                self.assertTrue(bridge.started.is_set())
                self.assertTrue(adapter._dispatch_tasks)
                bridge.release.set()
                await asyncio.gather(*tuple(adapter._dispatch_tasks))
            finally:
                adapter_module.web = old_web
            return response

        response = asyncio.run(run_case())
        self.assertEqual(response.status, 200)
        self.assertIn("qiwe_image_callback_processing_scheduled", response.text)

    def test_ordinary_group_ack_does_not_wait_for_room_name_refresh(self):
        class RecordingPublisher:
            def __init__(self):
                self.messages = []

            async def publish_capture(
                self, _raw_event, message_event, *, message_id
            ):
                self.messages.append((message_id, message_event))

        class SlowRoomNameAdapter(QiWeAdapter):
            def __init__(self):
                super().__init__(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "token": "synthetic-api-token",
                                "send_enabled": False,
                            }
                        },
                    )()
                )
                self.started = asyncio.Event()
                self.release = asyncio.Event()

            async def _resolve_group_display_name(self, _parsed):
                self.started.set()
                await self.release.wait()
                return "一栋住户群"

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = SlowRoomNameAdapter()
            publisher = RecordingPublisher()
            adapter._nats_capture = publisher
            ordinary = text_event("ordinary-room-name", content="hello")
            ordinary["msgData"]["atList"] = []
            try:
                response = await asyncio.wait_for(
                    adapter._handle_webhook(
                        FakeRequest(encoded(envelope([ordinary])))
                    ),
                    timeout=0.1,
                )
                await asyncio.wait_for(adapter.started.wait(), timeout=0.1)
                self.assertTrue(adapter._dispatch_tasks)
                self.assertEqual(publisher.messages, [])
                adapter.release.set()
                await asyncio.gather(*tuple(adapter._dispatch_tasks))
            finally:
                adapter_module.web = old_web
            return response, publisher

        response, publisher = asyncio.run(run_case())
        self.assertEqual(response.status, 200)
        self.assertEqual(len(publisher.messages), 1)
        self.assertEqual(
            publisher.messages[0][1]["conversation_display_name"], "一栋住户群"
        )


if __name__ == "__main__":
    unittest.main()
