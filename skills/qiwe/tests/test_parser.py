import copy
import asyncio
import json
import os
import tempfile
import time
import unittest
from pathlib import Path
from datetime import datetime, timedelta, timezone

import adapter as adapter_module
from adapter import (
    _CONTACT_REQUEST_TOOL_SEEN,
    _DIRECT_TOOL_SEEN,
    _HUMAN_HANDOFF_TOOL_SEEN,
    _LOCATION_TOOL_SEEN,
    _REVOKE_MESSAGE_TOOL_SEEN,
    _RICH_MESSAGE_TOOL_SEEN,
    _VOICE_TO_TEXT_TOOL_SEEN,
    _RECENT_QIWE_MESSAGE_CONTEXTS,
    _RECENT_QIWE_MESSAGE_REFS,
    _store_recent_message_context,
    _handle_qiwe_handoff_to_human,
    _handle_qiwe_revoke_message,
    _handle_qiwe_request_direct_contact,
    _handle_qiwe_send_direct_message,
    _handle_qiwe_send_location_card,
    _handle_qiwe_send_rich_message,
    _handle_qiwe_voice_to_text,
    QiWeAdapter,
    SendResult,
    _answer_context_from_mcp_stdout,
    _answer_context_mcp_request,
    _member_context_channel_prompt,
    _mentioned_member_names_from_at_list,
    _training_note_mcp_request,
    parse_qiwe_payload,
    register,
    _standalone_send,
)
from passive_pipeline import PassiveEventPipeline, PassivePipelineConfig
from nats_capture import build_capture_events
from qiwe_events import normalized_event_from_parsed
from solitaire.activity_service import ActivityService
from solitaire.feishu_writer import FeishuActivityMapping, FeishuActivityWriter
from solitaire.llm_parser import HermesSolitaireContentParser
from solitaire.parser import build_activity_record_from_fields, normalize_start_time_from_event, parse_activity_record, solitaire_created_at_from_event, stable_activity_body
from solitaire.reminder import ReminderWorker, ReminderWorkerConfig
from solitaire.reminder_policy import FEISHU_ACTIVITY_TYPES, ReminderPolicy
from solitaire.repository import ActivityRepository

FIXTURES = Path(__file__).parent / "fixtures"


class FakeSolitaireContentParser:
    def __init__(
        self,
        *,
        subject: str = "接龙数据格式测试",
        activity_type: str = "社区活动",
        activity_identity: str = "",
        detail: str = "剪鸭村·秦托邦数字游民社区(鄠邑区石井街道太土路457号)",
        start_time: str = "2026-06-11",
        participants: list[str] | None = None,
        promo_text: str = "",
    ) -> None:
        self.subject = subject
        self.activity_type = activity_type
        self.activity_identity = activity_identity
        self.detail = detail
        self.start_time = start_time
        self.participants = participants or ["弦默"]
        self.promo_text = promo_text

    async def parse(self, event):
        return build_activity_record_from_fields(
            event,
            getattr(event, "text", ""),
            activity_subject=self.subject,
            activity_type=self.activity_type,
            activity_identity=self.activity_identity,
            activity_detail=self.detail,
            start_time=self.start_time,
            participant_names=self.participants,
            promo_text=self.promo_text,
        )


class FakeStructuredResult:
    def __init__(self, parsed):
        self.parsed = parsed


class FakeCompleteResult:
    def __init__(self, text):
        self.text = text


class BlockingFeishuWriter(FeishuActivityWriter):
    def __init__(self, mapping=None):
        super().__init__(mapping or FeishuActivityMapping())
        self.loop = None
        self.started = asyncio.Event()
        self.release = asyncio.Event()
        self.calls = []

    def write(self, internal_fields, *, record_id=""):
        if self.loop is None:
            raise RuntimeError("test loop was not configured")
        loop = asyncio.run_coroutine_threadsafe(self._wait_for_release(internal_fields), self.loop)
        loop.result(timeout=2)
        return super().write(internal_fields, record_id=record_id)

    async def _wait_for_release(self, internal_fields):
        self.calls.append(internal_fields.get("activity_id", ""))
        self.started.set()
        await self.release.wait()


class FakeHermesLlm:
    def __init__(self, parsed=None, text=None):
        self.parsed = parsed
        self.text = text
        self.calls = []

    async def acomplete_structured(self, **kwargs):
        self.calls.append(kwargs)
        return FakeStructuredResult(self.parsed)

    async def acomplete(self, **kwargs):
        self.calls.append(kwargs)
        if self.text is not None:
            return FakeCompleteResult(self.text)
        return FakeCompleteResult(json.dumps(self.parsed, ensure_ascii=False))


def load_fixture(name: str) -> dict:
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


class QiWeParserTests(unittest.TestCase):
    def test_group_mention_triggers_and_uses_inner_room_id(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_mention.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, True)
        self.assertEqual(parsed.reason, "mentioned")
        self.assertEqual(parsed.group_id, "10733506388826175")
        self.assertEqual(parsed.sender_id, "7881303308049798")
        self.assertEqual(parsed.message_id, "7044924045046088437")
        self.assertEqual(parsed.text, "hi")

    def test_group_mention_with_wave_punctuation_triggers(self) -> None:
        payload = copy.deepcopy(load_fixture("group_mention.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["content"] = "@二花～ 我们回不来了…"
        raw_event["msgData"]["atList"] = []
        raw_event["msgUniqueIdentifier"] = "wave-mention-msg-001"
        payload["content"] = raw_event["msgData"]["content"]
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(
            payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, True)
        self.assertIs(parsed.is_mentioned, True)
        self.assertEqual(parsed.reason, "mentioned")
        self.assertEqual(parsed.text, "我们回不来了…")

    def test_group_normal_is_accepted_but_does_not_trigger(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_normal.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "not_mentioned")
        self.assertEqual(parsed.group_id, "10789255155259073")
        self.assertEqual(parsed.text, parsed.content)

    def test_group_cue_triggers_without_at_list(self) -> None:
        payload = copy.deepcopy(load_fixture("group_normal.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["content"] = "二花帮我查一下附近吃饭的地方"
        raw_event["msgData"]["atList"] = []
        raw_event["msgUniqueIdentifier"] = "cue-msg-001"
        payload["content"] = raw_event["msgData"]["content"]
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(
            payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            active_attachment_preprocess_enabled=True,
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, True)
        self.assertEqual(parsed.reason, "cued")

    def test_group_slash_command_is_blocked_before_hermes(self) -> None:
        payload = copy.deepcopy(load_fixture("group_mention.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["content"] = "/reset @二花"
        raw_event["msgUniqueIdentifier"] = "slash-msg-001"
        payload["content"] = raw_event["msgData"]["content"]
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(
            payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            active_attachment_preprocess_enabled=True,
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "blocked_slash_command")

    def test_direct_message_triggers_when_allowed(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("direct_text.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            direct_allowed_users=["7881303308049798"],
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, True)
        self.assertEqual(parsed.reason, "direct_message")
        self.assertEqual(parsed.conversation_type, "direct")
        self.assertEqual(parsed.chat_id, "7881303308049798")

    def test_direct_message_respects_allowlist(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("direct_text.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            direct_allowed_users=["someone-else"],
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "direct_not_allowed")

    def test_direct_message_defaults_to_allowlist_gate(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("direct_text.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "direct_not_allowed")

    def test_direct_message_can_be_explicitly_allowed_for_all(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("direct_text.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            direct_allow_all=True,
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, True)
        self.assertEqual(parsed.reason, "direct_message")

    def test_direct_link_card_is_parsed_as_link_attachment(self) -> None:
        payload = copy.deepcopy(load_fixture("direct_text.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgType"] = 13
        raw_event["newMsgType"] = "LINK"
        raw_event["msgUniqueIdentifier"] = "article-link-001"
        raw_event["msgData"] = {
            "title": "90%的人在AI上浪费时间，因为问错了第一个问题",
            "desc": "我最近越来越觉得，大部分自媒体在用\"AI\"两个字母谋杀大众的时间。",
            "linkUrl": "https://example.com/article",
            "iconUrl": "https://example.com/icon.png",
        }
        payload["commonMsgType"] = "LINK"
        payload["content"] = ""
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(
            payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            direct_allow_all=True,
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "non_text_link")
        self.assertEqual(parsed.message_kind, "link")
        self.assertEqual(parsed.attachments[0]["title"], "90%的人在AI上浪费时间，因为问错了第一个问题")
        self.assertEqual(parsed.attachments[0]["url"], "https://example.com/article")

    def test_outer_from_group_mismatch_is_diagnostic_only(self) -> None:
        payload = load_fixture("group_mention.json")
        payload["fromGroup"] = "outer-diagnostic-group"

        parsed = parse_qiwe_payload(
            payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            active_attachment_preprocess_enabled=True,
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, True)
        self.assertEqual(parsed.group_id, "10733506388826175")
        self.assertEqual(parsed.outer_group_id, "outer-diagnostic-group")
        self.assertIs(parsed.group_id_mismatch, True)

    def test_missing_inner_from_room_id_is_not_group_message(self) -> None:
        payload = copy.deepcopy(load_fixture("group_mention.json"))
        raw_event = json.loads(payload["data"])
        raw_event.pop("fromRoomId")
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        self.assertIs(parsed.accepted, False)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "not_group_message")

    def test_nats_capture_event_payload_includes_trigger_and_mentions(self) -> None:
        payload = load_fixture("group_mention.json")
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        raw_event, message_event, message_id = build_capture_events(
            parsed,
            json.dumps(payload, ensure_ascii=False).encode("utf-8"),
        )

        self.assertEqual(message_id, "7044924045046088437")
        self.assertEqual(raw_event["event_id"], "7044924045046088437")
        self.assertEqual(raw_event["source"], "qiwe")
        self.assertEqual(message_event["message_id"], "7044924045046088437")
        self.assertEqual(message_event["platform"], "qiwe")
        self.assertEqual(message_event["chat_id"], "10733506388826175")
        self.assertEqual(message_event["chat_type"], "group")
        self.assertEqual(message_event["message_kind"], "text")
        self.assertIs(message_event["is_mention_bot"], True)
        self.assertIs(message_event["should_trigger"], True)
        self.assertEqual(message_event["trigger_reason"], "mentioned")
        self.assertGreaterEqual(len(message_event["mentions"]), 1)

    def test_nats_capture_event_uses_resolved_identity_name(self) -> None:
        payload = load_fixture("group_mention.json")
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        identity = adapter_module.QiWeIdentity(
            user_id="7881303308049798",
            display_name="弦默",
            source="room_member",
        )

        _, message_event, _ = build_capture_events(
            parsed,
            json.dumps(payload, ensure_ascii=False).encode("utf-8"),
            identity=identity,
        )

        self.assertEqual(message_event["sender_name"], "弦默")
        self.assertEqual(message_event["sender_identity"]["chat_id"], "10733506388826175")
        self.assertEqual(message_event["sender_identity"]["channel_user_id"], "7881303308049798")
        self.assertEqual(message_event["sender_identity"]["display_name"], "弦默")
        self.assertEqual(message_event["sender_identity"]["identity_source"], "room_member")

    def test_nats_capture_event_marks_unresolved_identity(self) -> None:
        payload = load_fixture("group_mention.json")
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        _, message_event, _ = build_capture_events(
            parsed,
            json.dumps(payload, ensure_ascii=False).encode("utf-8"),
        )

        self.assertEqual(message_event["sender_identity"]["display_name"], "")
        self.assertEqual(message_event["sender_identity"]["error"], "display_name_unresolved")

    def test_nats_capture_event_does_not_use_sender_id_as_display_name(self) -> None:
        payload = load_fixture("group_mention.json")
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        identity = adapter_module.QiWeIdentity(
            user_id="7881303308049798",
            display_name="7881303308049798",
            source="fallback",
        )

        _, message_event, _ = build_capture_events(
            parsed,
            json.dumps(payload, ensure_ascii=False).encode("utf-8"),
            identity=identity,
        )

        self.assertEqual(message_event["sender_name"], "")
        self.assertEqual(message_event["sender_identity"]["display_name"], "")
        self.assertEqual(message_event["sender_identity"]["error"], "display_name_unresolved")

    def test_send_body_mentions_sender_from_thread_metadata(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10733506388826175",
            "hello",
            sender_id="7881303308049798",
            guid="91355723-5CA4-46C3-A2E9-1E3434C51DF3",
            is_group=True,
        )

        self.assertEqual(body["method"], "/msg/sendHyperText")
        self.assertEqual(body["params"]["toId"], "10733506388826175")
        self.assertEqual(body["params"]["content"][0], {"subtype": 1, "text": "7881303308049798"})
        self.assertEqual(body["params"]["content"][1], {"subtype": 0, "text": " hello"})

    def test_group_send_body_can_quote_source_message(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10733506388826175",
            "活动快开始啦",
            is_group=True,
            reply_ref={
                "msgServerId": "1017194",
                "msgType": 213,
                "msgUniqueIdentifier": "8971589583608054865",
                "userId": "7881303308049798",
                "timeStamp": 1781176277,
                "msgData": {"title": "#Group Note\n接龙数据格式测试"},
            },
        )

        self.assertEqual(body["method"], "/msg/sendHyperText")
        self.assertEqual(
            body["params"]["reply"],
            {
                "type": 0,
                "userId": "7881303308049798",
                "timeStamp": 1781176277,
                "msgUniqueIdentifier": "8971589583608054865",
                "msgData": {"content": "#Group Note\n接龙数据格式测试"},
            },
        )

    def test_group_send_body_can_mention_human_support_and_quote_source_message(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10859791146538059",
            "这个我现在不能确认具体情况哦，麻烦帮忙看下这个问题～",
            mention_user_ids=["1688854741472532"],
            is_group=True,
            reply_ref={
                "userId": "7881303308049798",
                "timeStamp": 1782439549,
                "msgUniqueIdentifier": "question-001",
                "msgData": {"content": "还有空房吗"},
            },
        )

        self.assertEqual(body["method"], "/msg/sendHyperText")
        self.assertEqual(body["params"]["toId"], "10859791146538059")
        self.assertEqual(body["params"]["content"][0], {"subtype": 1, "text": "1688854741472532"})
        self.assertEqual(
            body["params"]["content"][1],
            {"subtype": 0, "text": " 这个我现在不能确认具体情况哦，麻烦帮忙看下这个问题～"},
        )
        self.assertEqual(
            body["params"]["reply"],
            {
                "type": 0,
                "userId": "7881303308049798",
                "timeStamp": 1782439549,
                "msgUniqueIdentifier": "question-001",
                "msgData": {"content": "还有空房吗"},
            },
        )

    def test_group_send_body_strips_duplicate_sender_display_name(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10733506388826175",
            "王羽 我先只放轻梗，不放伤人的哈",
            sender_id="7881303308049798",
            sender_display_names=["王羽"],
            is_group=True,
        )

        self.assertEqual(body["params"]["content"][0], {"subtype": 1, "text": "7881303308049798"})
        self.assertEqual(body["params"]["content"][1], {"subtype": 0, "text": " 我先只放轻梗，不放伤人的哈"})

    def test_group_send_body_strips_duplicate_at_sender_display_name(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10733506388826175",
            "@huang 我找了下，能确认两块",
            sender_id="7881303308049798",
            sender_display_names=["huang"],
            is_group=True,
        )

        self.assertEqual(body["params"]["content"][0], {"subtype": 1, "text": "7881303308049798"})
        self.assertEqual(body["params"]["content"][1], {"subtype": 0, "text": " 我找了下，能确认两块"})

    def test_group_send_body_does_not_strip_partial_sender_name_prefix(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10733506388826175",
            "王羽毛球活动在哪",
            sender_id="7881303308049798",
            sender_display_names=["王羽"],
            is_group=True,
        )

        self.assertEqual(body["params"]["content"][1], {"subtype": 0, "text": " 王羽毛球活动在哪"})

    def test_group_send_uses_sender_display_name_metadata_for_prefix_cleanup(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"send_enabled": False}})())
                self.bodies = []

            async def _post_qiwe_body(self, body):
                self.bodies.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        adapter = RecordingAdapter()
        result = asyncio.run(
            adapter.send(
                "10733506388826175",
                "王羽 我先只放轻梗",
                metadata={"sender_id": "7881303308049798", "sender_display_name": "王羽"},
            )
        )

        self.assertIs(result.success, True)
        self.assertEqual(adapter.bodies[0]["params"]["content"][1], {"subtype": 0, "text": " 我先只放轻梗"})

    def test_home_group_send_is_treated_as_group_without_sender_id(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"send_enabled": False}})())
                self.bodies = []

            async def _post_qiwe_body(self, body):
                self.bodies.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        old_env = dict(os.environ)
        try:
            os.environ["QIWE_HOME_GROUP"] = "10733506388826175"
            adapter = RecordingAdapter()
            result = asyncio.run(adapter.send("10733506388826175", "大家早"))
        finally:
            os.environ.clear()
            os.environ.update(old_env)

        self.assertIs(result.success, True)
        self.assertEqual(adapter.bodies[0]["method"], "/msg/sendHyperText")
        self.assertEqual(adapter.bodies[0]["params"]["toId"], "10733506388826175")
        self.assertEqual(adapter.bodies[0]["params"]["content"], [{"subtype": 0, "text": "大家早"}])

    def test_standalone_send_strips_cron_response_wrapper(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                calls.append((chat_id, content, metadata))
                return SendResult(success=True, raw_response={"dryRun": True})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_HOME_GROUP"] = "10733506388826175"
            adapter_module.QiWeAdapter = RecordingAdapter
            message = (
                "Cronjob Response: 秦托邦小伙伴（新）每晚安全提醒\n"
                "(job_id: 22443ab4be67)\n"
                "-------------\n\n"
                "睡前一分钟安全巡检：关水、关灯、关空调、锁门窗。小动作省心又省电，祝大家今晚睡个好觉～\n\n"
                "To stop or manage this job, send me a new message (e.g. \"stop reminder 秦托邦小伙伴（新）每晚安全提醒\")."
            )

            result = asyncio.run(_standalone_send(type("Config", (), {"extra": {}})(), "10733506388826175", message))
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)

        self.assertIs(result["success"], True)
        self.assertEqual(calls[0][0], "10733506388826175")
        self.assertEqual(calls[0][1], "睡前一分钟安全巡检：关水、关灯、关空调、锁门窗。小动作省心又省电，祝大家今晚睡个好觉～")
        self.assertEqual(calls[0][2], {"conversation_type": "group"})

    def test_standalone_send_strips_saved_cron_output_header(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                calls.append(content)
                return SendResult(success=True, raw_response={"dryRun": True})

        original = adapter_module.QiWeAdapter
        try:
            adapter_module.QiWeAdapter = RecordingAdapter
            message = (
                "# Cron Job: 秦托邦小伙伴（新）每晚安全提醒\n"
                "**Job ID:** 22443ab4be67\n"
                "**Run Time:** 2026-06-07 23:00:03\n"
                "**Mode:** no_agent (script)\n"
                "---\n"
                "睡前一分钟安全巡检：关水、关灯、关空调、锁门窗。"
            )

            result = asyncio.run(_standalone_send(type("Config", (), {"extra": {}})(), "10733506388826175", message))
        finally:
            adapter_module.QiWeAdapter = original

        self.assertIs(result["success"], True)
        self.assertEqual(calls, ["睡前一分钟安全巡检：关水、关灯、关空调、锁门窗。"])

    def test_standalone_send_skips_silent_sentinel_after_cron_wrapper(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                raise AssertionError("SILENT cron output should not be sent")

        original = adapter_module.QiWeAdapter
        try:
            adapter_module.QiWeAdapter = RecordingAdapter
            message = "Cronjob Response: weather\n(job_id: 1)\n-------------\n\n[SILENT]"

            result = asyncio.run(_standalone_send(type("Config", (), {"extra": {}})(), "10733506388826175", message))
        finally:
            adapter_module.QiWeAdapter = original

        self.assertIs(result["success"], True)
        self.assertEqual(result["raw_response"], {"skipped": "[SILENT]"})

    def test_direct_send_body_uses_send_text_without_mention(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "7881303308049798",
            "王羽 我发你一下",
            sender_id="",
            sender_display_names=["王羽"],
            guid="91355723-5CA4-46C3-A2E9-1E3434C51DF3",
            is_group=False,
        )

        self.assertEqual(body["method"], "/msg/sendText")
        self.assertEqual(body["params"]["toId"], "7881303308049798")
        self.assertEqual(body["params"]["content"], "王羽 我发你一下")

    def test_group_location_fallback_strips_duplicate_sender_display_name(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"send_enabled": False}})())
                self.bodies = []

            async def _post_qiwe_body(self, body):
                self.bodies.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        adapter = RecordingAdapter()
        result = asyncio.run(
            adapter._send_location_bundle(
                "10733506388826175",
                "王羽 我找到了这个位置",
                {"title": "秦托邦 B 栋"},
                sender_id="7881303308049798",
                sender_display_names=["王羽"],
                is_group=True,
            )
        )

        self.assertIs(result.success, True)
        self.assertEqual(adapter.bodies[0]["method"], "/msg/sendHyperText")
        self.assertEqual(adapter.bodies[0]["params"]["content"][1], {"subtype": 0, "text": " 我找到了这个位置"})

    def test_group_send_body_strips_explicit_at_sender_id_only(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10733506388826175",
            "@7881303308049798 我找了下",
            sender_id="7881303308049798",
            is_group=True,
        )

        self.assertEqual(body["params"]["content"][1], {"subtype": 0, "text": " 我找了下"})

    def test_direct_send_checks_normal_friend_before_send(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"token": "test-token", "guid": "guid-1"}})())
                self.calls = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                self.calls.append((method, params, require_send_enabled))
                if method == "/contact/getWxContactList":
                    return SendResult(
                        success=True,
                        raw_response={
                            "code": 0,
                            "data": {
                                "currentSeq": 1,
                                "hasMore": False,
                                "contactList": [{"userId": "7881303308049798", "contactType": 2057}],
                            },
                        },
                    )
                if method == "/msg/sendText":
                    return SendResult(success=True, raw_response={"code": 0, "data": {"isSendSuccess": 1}})
                raise AssertionError(f"unexpected method: {method}")

        adapter = RecordingAdapter()
        result = asyncio.run(adapter.send("7881303308049798", "hello", metadata={"conversation_type": "direct"}))

        self.assertIs(result.success, True)
        self.assertEqual(adapter.calls[0][0], "/contact/getWxContactList")
        self.assertEqual(adapter.calls[0][1]["bizType"], 1)
        self.assertEqual(adapter.calls[0][2], False)
        self.assertEqual(adapter.calls[1][0], "/msg/sendText")
        self.assertEqual(adapter.calls[1][1]["toId"], "7881303308049798")

    def test_direct_send_blocks_deleted_contact_before_send(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"token": "test-token", "guid": "guid-1"}})())
                self.calls = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                self.calls.append(method)
                if method == "/contact/getWxContactList":
                    return SendResult(
                        success=True,
                        raw_response={
                            "code": 0,
                            "data": {
                                "currentSeq": 1,
                                "hasMore": False,
                                "contactList": [
                                    {
                                        "userId": "7881303308049798",
                                        "contactType": 2049,
                                        "mobile": "17600000000",
                                        "remark": "不应泄露",
                                    }
                                ],
                            },
                        },
                    )
                raise AssertionError(f"send should be blocked before {method}")

        adapter = RecordingAdapter()
        result = asyncio.run(adapter.send("7881303308049798", "hello", metadata={"conversation_type": "direct"}))

        self.assertIs(result.success, False)
        self.assertIn("contactType=2049", result.error)
        encoded = json.dumps(result.raw_response, ensure_ascii=False)
        self.assertNotIn("17600000000", encoded)
        self.assertNotIn("不应泄露", encoded)
        self.assertEqual(adapter.calls, ["/contact/getWxContactList"])

    def test_direct_send_blocks_when_contact_guard_fails(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"token": "test-token", "guid": "guid-1"}})())
                self.calls = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                self.calls.append(method)
                if method == "/contact/getWxContactList":
                    return SendResult(success=False, error="QiWe HTTP 502", retryable=True)
                raise AssertionError(f"send should be blocked before {method}")

        adapter = RecordingAdapter()
        result = asyncio.run(adapter.send("7881303308049798", "hello", metadata={"conversation_type": "direct"}))

        self.assertIs(result.success, False)
        self.assertIs(result.retryable, True)
        self.assertIn("QiWe contact guard failed", result.error)
        self.assertEqual(adapter.calls, ["/contact/getWxContactList"])

    def test_group_send_does_not_check_contact_guard(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"token": "test-token", "guid": "guid-1"}})())
                self.calls = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                self.calls.append(method)
                if method == "/msg/sendHyperText":
                    return SendResult(success=True, raw_response={"code": 0, "data": {"isSendSuccess": 1}})
                raise AssertionError(f"unexpected method: {method}")

        adapter = RecordingAdapter()
        result = asyncio.run(
            adapter.send(
                "10733506388826175",
                "hello",
                metadata={"sender_id": "7881303308049798", "conversation_type": "group"},
            )
        )

        self.assertIs(result.success, True)
        self.assertEqual(adapter.calls, ["/msg/sendHyperText"])

    def test_location_message_is_metadata_only_and_does_not_trigger(self) -> None:
        payload = copy.deepcopy(load_fixture("group_mention.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgType"] = 6
        raw_event["newMsgType"] = "LOCATION"
        raw_event["msgUniqueIdentifier"] = "location-msg-001"
        raw_event["msgData"] = {
            "title": "56em5omY6YKmIEIg5qCL",
            "address": "56em5omY6YKm56S+5Yy6",
            "latitude": 34.022625,
            "longitude": 108.572545,
            "zoom": 16,
        }
        payload["commonMsgType"] = "LOCATION"
        payload["content"] = ""
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "non_text_location")
        self.assertEqual(parsed.message_kind, "location")
        self.assertEqual(parsed.attachments[0]["title"], "秦托邦 B 栋")
        self.assertEqual(parsed.attachments[0]["address"], "秦托邦社区")
        self.assertEqual(parsed.attachments[0]["latitude"], 34.022625)
        self.assertEqual(parsed.attachments[0]["longitude"], 108.572545)

    def test_voice_message_is_metadata_only_without_secret_file_key(self) -> None:
        payload = copy.deepcopy(load_fixture("group_mention.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgType"] = 16
        raw_event["newMsgType"] = "VOICE"
        raw_event["msgServerId"] = 1003001
        raw_event["msgUniqueIdentifier"] = "voice-msg-001"
        raw_event["msgData"] = {
            "fileId": "voice-file-1",
            "fileAesKey": "must-not-leak",
            "fileMd5": "voice-md5",
            "fileSize": 2400,
            "voiceTime": 3,
        }
        payload["commonMsgType"] = "VOICE"
        payload["content"] = ""
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        encoded = json.dumps(parsed.attachments, ensure_ascii=False)

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "non_text_voice")
        self.assertEqual(parsed.message_kind, "voice")
        self.assertEqual(parsed.attachments[0]["msg_server_id"], "1003001")
        self.assertEqual(parsed.attachments[0]["voice_time"], 3)
        self.assertNotIn("file_aes_key", parsed.attachments[0])
        self.assertNotIn("must-not-leak", encoded)

    def test_solitaire_message_uses_msg_data_title_and_does_not_trigger_when_not_mentioned(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        self.assertIs(parsed.accepted, True)
        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "non_text_solitaire")
        self.assertEqual(parsed.message_kind, "solitaire")
        self.assertIn("接龙数据格式测试", parsed.text)
        self.assertEqual(parsed.content, parsed.text)
        self.assertEqual(parsed.attachments[0]["kind"], "solitaire")
        self.assertEqual(parsed.attachments[0]["author_id"], "7881303308049798")

    def test_solitaire_activity_parser_handles_group_note_header(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        activity = asyncio.run(parse_activity_record(normalized_event_from_parsed(parsed), FakeSolitaireContentParser()))

        self.assertIsNotNone(activity)
        assert activity is not None
        self.assertEqual(activity.source_group_id, "10789255155259073")
        self.assertEqual(activity.source_message_id, "8971589583608054865")
        self.assertEqual(activity.source_sender_id, "弦默")
        self.assertEqual(activity.activity_subject, "接龙数据格式测试")
        self.assertEqual(activity.activity_detail, "剪鸭村·秦托邦数字游民社区(鄠邑区石井街道太土路457号)")
        self.assertEqual(activity.start_time, "2026-06-11")
        self.assertEqual(activity.solitaire_created_at, "2026-06-11T11:10:32+00:00")
        self.assertEqual(activity.participant_names, ["弦默"])
        self.assertEqual(activity.participant_count, 1)
        self.assertIn("接龙数据格式测试", activity.promo_text)

    def test_solitaire_created_at_prefers_earliest_solitaire_item_timestamp(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        event = normalized_event_from_parsed(parsed)

        self.assertEqual(solitaire_created_at_from_event(event), "2026-06-11T11:10:32+00:00")

    def test_solitaire_activity_sender_uses_name_then_first_participant(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        event = normalized_event_from_parsed(parsed)
        event.sender_name = "发起人昵称"
        activity = asyncio.run(parse_activity_record(event, FakeSolitaireContentParser(participants=["弦默", "阿凯"])))

        self.assertIsNotNone(activity)
        assert activity is not None
        self.assertEqual(activity.source_sender_id, "发起人昵称")

        event.sender_name = ""
        fallback = asyncio.run(parse_activity_record(event, FakeSolitaireContentParser(participants=["弦默", "阿凯"])))

        self.assertIsNotNone(fallback)
        assert fallback is not None
        self.assertEqual(fallback.source_sender_id, "弦默")

    def test_chinese_solitaire_header_is_ignored(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["title"] = raw_event["msgData"]["title"].replace("#Group Note", "#接龙")
        raw_event["msgUniqueIdentifier"] = "solitaire-cn-001"
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        activity = asyncio.run(parse_activity_record(normalized_event_from_parsed(parsed), FakeSolitaireContentParser()))

        self.assertIsNotNone(activity)
        assert activity is not None
        self.assertEqual(activity.activity_subject, "接龙数据格式测试")

    def test_solitaire_activity_parser_uses_llm_structured_result(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["title"] = (
            "#Group Note\n"
            "下午 3点开始测试二花从群聊中提取接龙信息，包含标题、时间等，输出内容到日志，包含活动参与人\n\n"
            "2026/06/12;\n\n"
            "1. 弦默\n"
            "2. 大羽带两个人\n"
            "3. 阿凯"
        )
        raw_event["msgUniqueIdentifier"] = "solitaire-cn-time-proxy-001"
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        activity = asyncio.run(
            parse_activity_record(
                normalized_event_from_parsed(parsed),
                FakeSolitaireContentParser(
                    subject="测试二花从群聊中提取接龙信息，包含标题、时间等，输出内容到日志，包含活动参与人",
                    start_time="2026-06-12 15:00",
                    participants=["弦默", "大羽", "大羽代报名1", "大羽代报名2", "阿凯"],
                ),
            )
        )

        self.assertIsNotNone(activity)
        assert activity is not None
        self.assertEqual(activity.activity_subject, "测试二花从群聊中提取接龙信息，包含标题、时间等，输出内容到日志，包含活动参与人")
        self.assertEqual(activity.start_time, "2026-06-12 15:00")
        self.assertEqual(activity.participant_names, ["弦默", "大羽", "大羽代报名1", "大羽代报名2", "阿凯"])
        self.assertEqual(activity.participant_count, 5)

    def test_solitaire_activity_parser_without_content_parser_returns_none(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        activity = asyncio.run(parse_activity_record(normalized_event_from_parsed(parsed)))

        self.assertIsNone(activity)

    def test_llm_solitaire_parser_handles_series_activity_json(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["title"] = (
            "#接龙\n"
            "🎋 【社区端午刺绣系列活动·预报名接龙】 🪡\n\n"
            "📅 活动主题 & 时间\n"
            "1️⃣ 布艺刺绣手袋 —— 6月15日\n"
            "2️⃣ 刺绣布艺 —— 6月17日\n"
            "3️⃣ 端午香囊 —— 6月19日\n\n"
            "📍 地点：社区活动室（具体房号稍后通知）\n"
            "💰 费用： 少量材料费\n\n"
            "1. 秦托邦小客服\n"
            "2. 阿城 2"
        )
        raw_event["msgUniqueIdentifier"] = "solitaire-llm-series-001"
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        content_parser = HermesSolitaireContentParser(
            FakeHermesLlm(
                {
                    "is_activity": True,
                    "activity_subject": "社区端午刺绣系列活动·预报名接龙",
                    "activity_type": "手作体验",
                    "activity_detail": "三场刺绣手作活动：布艺刺绣手袋 6月15日；刺绣布艺 6月17日；端午香囊 6月19日。地点：社区活动室（具体房号稍后通知）。费用：少量材料费。",
                    "start_time": "",
                    "participant_names": ["秦托邦小客服", "阿城", "阿城代报名1"],
                    "promo_text": "端午前一起来社区活动室体验刺绣手作，感受手作的温度。",
                }
            )
        )

        activity = asyncio.run(parse_activity_record(normalized_event_from_parsed(parsed), content_parser))

        self.assertIsNotNone(activity)
        assert activity is not None
        self.assertEqual(activity.activity_subject, "社区端午刺绣系列活动·预报名接龙")
        self.assertEqual(activity.activity_type, "手作体验")
        self.assertEqual(activity.start_time, "")
        self.assertEqual(activity.participant_names, ["秦托邦小客服", "阿城", "阿城代报名1"])
        self.assertEqual(activity.participant_count, 3)
        self.assertIn("三场刺绣手作活动", activity.activity_detail)
        self.assertIn("messages", content_parser.llm.calls[0])
        self.assertNotIn("json_schema", content_parser.llm.calls[0])
        self.assertNotIn("json_mode", content_parser.llm.calls[0])

    def test_llm_solitaire_parser_includes_message_time_context(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw_event = json.loads(payload["data"])
        raw_event["timestamp"] = 1781312400
        raw_event["msgData"]["title"] = "#接龙\n下午3点半去开卡丁车\n\n1. 弦默"
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        content_parser = HermesSolitaireContentParser(
            FakeHermesLlm(
                {
                    "is_activity": True,
                    "activity_subject": "开卡丁车",
                    "activity_type": "运动娱乐",
                    "activity_detail": "下午3点半去开卡丁车",
                    "start_time": "2026-06-13 15:30",
                    "participant_names": ["弦默"],
                    "promo_text": "下午一起去开卡丁车，体验速度与激情。",
                }
            )
        )
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_ACTIVITY_TIMEZONE"] = "Asia/Shanghai"
            activity = asyncio.run(parse_activity_record(normalized_event_from_parsed(parsed), content_parser))
        finally:
            os.environ.clear()
            os.environ.update(old_env)

        self.assertIsNotNone(activity)
        assert activity is not None
        self.assertEqual(activity.activity_type, "运动娱乐")
        self.assertEqual(activity.start_time, "2026-06-13 15:30")
        user_message = content_parser.llm.calls[0]["messages"][1]["content"]
        self.assertIn("消息发送时间：2026-06-13 09:00:00", user_message)
        self.assertIn("时区：Asia/Shanghai", user_message)
        self.assertIn("下午3点半去开卡丁车", user_message)

    def test_llm_solitaire_parser_rejects_invalid_or_non_activity_json(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        non_activity_parser = HermesSolitaireContentParser(FakeHermesLlm({"is_activity": False}))
        missing_participants_parser = HermesSolitaireContentParser(
            FakeHermesLlm(
                {
                    "is_activity": True,
                    "activity_subject": "缺参与人",
                    "activity_type": "其他",
                    "activity_detail": "",
                    "start_time": "",
                    "participant_names": [],
                    "promo_text": "",
                }
            )
        )

        non_activity = asyncio.run(
            parse_activity_record(
                normalized_event_from_parsed(parsed),
                non_activity_parser,
            )
        )
        invalid = asyncio.run(
            parse_activity_record(
                normalized_event_from_parsed(parsed),
                missing_participants_parser,
            )
        )

        self.assertIsNone(non_activity)
        self.assertIsNone(invalid)
        self.assertEqual(non_activity_parser.last_diagnostic["reason"], "llm_non_activity")
        self.assertEqual(missing_participants_parser.last_diagnostic["reason"], "missing_participants")

    def test_llm_solitaire_parser_rejects_non_json_text(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        content_parser = HermesSolitaireContentParser(FakeHermesLlm(text="这是活动，但我没有输出 JSON"))

        activity = asyncio.run(
            parse_activity_record(
                normalized_event_from_parsed(parsed),
                content_parser,
            )
        )

        self.assertIsNone(activity)
        self.assertEqual(content_parser.last_diagnostic["reason"], "invalid_json")
        self.assertIn("这是活动", content_parser.last_diagnostic["response_preview"])

    def test_mentioned_solitaire_does_not_trigger_active_agent(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["title"] = "@二花 " + raw_event["msgData"]["title"]
        raw_event["msgData"]["atList"] = [{"nickname": "二花", "userId": "1688857683805864"}]
        raw_event["msgUniqueIdentifier"] = "solitaire-mentioned-001"
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(
            payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            active_attachment_preprocess_enabled=True,
        )

        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "non_text_solitaire")
        self.assertEqual(parsed.message_kind, "solitaire")

    def test_mentioned_voice_gets_controlled_fallback_when_preprocess_enabled(self) -> None:
        payload = copy.deepcopy(load_fixture("group_mention.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgType"] = 16
        raw_event["newMsgType"] = "VOICE"
        raw_event["msgServerId"] = 1003002
        raw_event["msgUniqueIdentifier"] = "voice-mentioned-001"
        raw_event["msgData"] = {
            "atList": [{"nickname": "二花", "userId": "1688857683805864"}],
            "fileId": "voice-file-2",
            "fileMd5": "voice-md5",
            "voiceTime": 5,
        }
        payload["commonMsgType"] = "VOICE"
        payload["content"] = ""
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)
        adapter = QiWeAdapter(type("Config", (), {"extra": {"active_attachment_preprocess_enabled": True}})())

        parsed = parse_qiwe_payload(
            payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            active_attachment_preprocess_enabled=True,
        )
        dispatch_text = adapter._active_dispatch_text(parsed)

        self.assertIs(parsed.should_trigger, True)
        self.assertEqual(parsed.message_kind, "voice")
        self.assertIn("voice消息", dispatch_text)
        self.assertIn("尚未启用", dispatch_text)

    def test_mentioned_attachment_does_not_trigger_when_preprocess_disabled(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw_event = json.loads(payload["data"])
        raw_event["msgData"]["title"] = "@二花 " + raw_event["msgData"]["title"]
        raw_event["msgData"]["atList"] = [{"nickname": "二花", "userId": "1688857683805864"}]
        raw_event["msgUniqueIdentifier"] = "solitaire-mentioned-disabled-001"
        payload["data"] = json.dumps(raw_event, ensure_ascii=False)

        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        self.assertIs(parsed.should_trigger, False)
        self.assertEqual(parsed.reason, "non_text_solitaire")

    def test_send_body_preserves_markdown(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_send_body(
            "10733506388826175",
            "**重点**\n# 标题",
            sender_id="7881303308049798",
            is_group=True,
        )

        self.assertEqual(body["params"]["content"][1], {"subtype": 0, "text": " **重点**\n# 标题"})

    def test_send_skips_no_reply_sentinel(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"send_enabled": False}})())
                self.bodies = []

            async def _post_qiwe_body(self, body):
                self.bodies.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        adapter = RecordingAdapter()
        result = asyncio.run(
            adapter.send(
                "10733506388826175",
                "  NO_REPLY\n",
                metadata={
                    "sender_id": "7881303308049798",
                    "location_card": {
                        "title": "秦托邦 B 栋",
                        "latitude": 34.022625,
                        "longitude": 108.572545,
                    },
                },
            )
        )

        self.assertIs(result.success, True)
        self.assertEqual(result.raw_response, {"skipped": "NO_REPLY"})
        self.assertEqual(adapter.bodies, [])

    def test_send_skips_internal_process_messages(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"send_enabled": False}})())
                self.bodies = []

            async def _post_qiwe_body(self, body):
                self.bodies.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        samples = [
            "⚠️ **Dangerous command requires approval:**\n```\nexecute_code <<'PY'\nprint(1)\nPY\n```",
            "Reply `/approve` to execute, `/approve session` to approve this pattern for the session.",
            "⏳ Working — 3 min — iteration 2/90, execute_code",
            'Traceback (most recent call last):\n  File "/home/ubuntu/.hermes/hermes-agent/foo.py", line 1',
            "record_id=recABCDEFG123456789 obj_token=secretish",
        ]

        for sample in samples:
            adapter = RecordingAdapter()
            result = asyncio.run(
                adapter.send(
                    "10733506388826175",
                    sample,
                    metadata={"sender_id": "7881303308049798"},
                )
            )

            self.assertIs(result.success, True)
            self.assertEqual(result.raw_response, {"skipped": "internal_process_message"})
            self.assertEqual(adapter.bodies, [])

    def test_location_body_uses_qiwe_send_location_shape(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_location_body(
            "10733506388826175",
            {
                "title": "秦托邦 B 栋",
                "address": "秦托邦社区",
                "latitude": 34.022625,
                "longitude": 108.572545,
            },
            guid="91355723-5CA4-46C3-A2E9-1E3434C51DF3",
        )

        self.assertEqual(body["method"], "/msg/sendLocation")
        self.assertEqual(body["params"]["toId"], "10733506388826175")
        self.assertEqual(body["params"]["title"], "秦托邦 B 栋")
        self.assertEqual(body["params"]["latitude"], 34.022625)
        self.assertEqual(body["params"]["longitude"], 108.572545)

    def test_rich_message_body_uses_qiwe_documented_shapes(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False, "guid": "guid-1"}})())
        cases = [
            (
                "image",
                {
                    "file_aes_key": "aes-image",
                    "file_id": "file-image",
                    "file_md5": "md5-image",
                    "file_size": "48300",
                    "filename": "mystone.jpg",
                },
                "/msg/sendImage",
                {"fileAesKey": "aes-image", "fileId": "file-image", "fileMd5": "md5-image", "fileSize": 48300, "filename": "mystone.jpg"},
            ),
            ("gif", {"wx_file_url": "http://p.qpic.cn/gif/0"}, "/msg/sendGif", {"wxFileUrl": "http://p.qpic.cn/gif/0"}),
            (
                "file",
                {"file_aes_key": "aes-file", "file_id": "file-id", "file_size": 827392, "filename": "report.xls"},
                "/msg/sendFile",
                {"fileAesKey": "aes-file", "fileId": "file-id", "fileSize": 827392, "filename": "report.xls"},
            ),
            (
                "voice",
                {"file_aes_key": "aes-voice", "file_id": "voice-id", "file_size": 117935, "voice_time": 2},
                "/msg/sendVoice",
                {"fileAesKey": "aes-voice", "fileId": "voice-id", "fileSize": 117935, "voiceTime": 2},
            ),
            (
                "link",
                {"title": "我是title", "icon_url": "https://example.com/icon.png", "link_url": "https://example.com", "desc": "我是desc"},
                "/msg/sendLink",
                {"title": "我是title", "iconUrl": "https://example.com/icon.png", "linkUrl": "https://example.com", "desc": "我是desc"},
            ),
            (
                "weapp",
                {
                    "app_id": "wx-app",
                    "cover_file_aes_key": "cover-aes",
                    "cover_file_id": "cover-id",
                    "cover_file_size": 2899681,
                    "desc": "test-desc",
                    "page_path": "pages/index",
                    "thumb_url": "https://example.com/thumb.png",
                    "title": "test-title",
                    "username": "gh_x@app",
                },
                "/msg/sendWeapp",
                {
                    "appId": "wx-app",
                    "coverFileAesKey": "cover-aes",
                    "coverFileId": "cover-id",
                    "coverFileSize": 2899681,
                    "desc": "test-desc",
                    "pagePath": "pages/index",
                    "thumbUrl": "https://example.com/thumb.png",
                    "title": "test-title",
                    "username": "gh_x@app",
                },
            ),
            ("personal_card", {"shared_id": "7881302799122145"}, "/msg/sendPersonalCard", {"sharedId": "7881302799122145"}),
        ]

        for message_type, payload, method, expected in cases:
            with self.subTest(message_type=message_type):
                body = adapter._build_rich_message_body(message_type, "10814496149970753", payload)
                self.assertEqual(body["method"], method)
                self.assertEqual(body["params"]["guid"], "guid-1")
                self.assertEqual(body["params"]["toId"], "10814496149970753")
                for key, value in expected.items():
                    self.assertEqual(body["params"][key], value)

    def test_revoke_message_body_uses_qiwe_documented_shape(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())
        body = adapter._build_revoke_message_body("10814496149970753", "1121922", guid="guid-1")

        self.assertEqual(body["method"], "/msg/revokeMsg")
        self.assertEqual(body["params"], {"guid": "guid-1", "chatId": "10814496149970753", "msgServerId": 1121922})

    def test_location_bundle_group_fallback_can_skip_sender_mention(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"send_enabled": False}})())
                self.bodies = []

            async def _post_qiwe_body(self, body):
                self.bodies.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        adapter = RecordingAdapter()
        result = asyncio.run(
            adapter._send_location_bundle(
                "10733506388826175",
                "我找到了这个位置",
                {"title": "秦托邦 B 栋"},
                sender_id="",
                is_group=True,
            )
        )

        self.assertIs(result.success, True)
        self.assertEqual(adapter.bodies[0]["method"], "/msg/sendHyperText")
        self.assertEqual(adapter.bodies[0]["params"]["content"][0]["subtype"], 0)

    def test_dedupe_detects_replayed_message_id(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())

        self.assertIs(adapter._is_duplicate("msg-1"), False)
        self.assertIs(adapter._is_duplicate("msg-1"), True)

    def test_dispatch_does_not_call_qiwe_identity_lookup_on_cache_miss(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(
                    type(
                        "Config",
                        (),
                        {"extra": {"token": "test-token", "send_enabled": False, "answer_context_prepare_enabled": False}},
                    )()
                )
                self.events = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                raise AssertionError(f"identity lookup should not call QiWe in realtime path: {method}")

            async def handle_message(self, event):
                self.events.append(event)

        adapter = RecordingAdapter()
        parsed = parse_qiwe_payload(
            load_fixture("group_mention.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        asyncio.run(adapter._dispatch_message(parsed))

        self.assertEqual(adapter.events[0].source["user_id"], "7881303308049798")
        self.assertIsNone(adapter.events[0].source["user_name"])
        self.assertEqual(adapter.events[0].raw_message["message_kind"], "text")
        self.assertEqual(adapter.events[0].raw_message["attachments"], [])
        self.assertIn("answer_context_unavailable", adapter.events[0].channel_prompt)
        self.assertIn('"chat_id": "10733506388826175"', adapter.events[0].channel_prompt)
        self.assertIn('"channel_user_id": "7881303308049798"', adapter.events[0].channel_prompt)
        self.assertNotIn("mobile", adapter.events[0].raw_message)

    def test_dispatch_includes_quoted_link_context_from_recent_message(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(
                    type(
                        "Config",
                        (),
                        {"extra": {"token": "test-token", "send_enabled": False, "answer_context_prepare_enabled": False}},
                    )()
                )
                self.events = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                raise AssertionError(f"identity lookup should not call QiWe in realtime path: {method}")

            async def handle_message(self, event):
                self.events.append(event)

        _RECENT_QIWE_MESSAGE_CONTEXTS.clear()
        link_payload = copy.deepcopy(load_fixture("direct_text.json"))
        link_raw = json.loads(link_payload["data"])
        link_raw["msgType"] = 13
        link_raw["newMsgType"] = "LINK"
        link_raw["msgUniqueIdentifier"] = "article-link-001"
        link_raw["msgData"] = {
            "title": "90%的人在AI上浪费时间，因为问错了第一个问题",
            "desc": "我最近越来越觉得，大部分自媒体在用\"AI\"两个字母谋杀大众的时间。",
            "linkUrl": "https://example.com/article",
        }
        link_payload["commonMsgType"] = "LINK"
        link_payload["content"] = ""
        link_payload["data"] = json.dumps(link_raw, ensure_ascii=False)
        link_parsed = parse_qiwe_payload(
            link_payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            direct_allow_all=True,
        )
        _store_recent_message_context(link_parsed)

        quote_payload = copy.deepcopy(load_fixture("direct_text.json"))
        quote_raw = json.loads(quote_payload["data"])
        quote_raw["msgUniqueIdentifier"] = "quote-link-001"
        quote_raw["msgData"] = {
            "atList": [],
            "content": "把这篇文章可以转发到秦托邦的小伙伴群",
            "reply": {"msgId": "article-link-001", "msgType": 0},
        }
        quote_payload["content"] = quote_raw["msgData"]["content"]
        quote_payload["data"] = json.dumps(quote_raw, ensure_ascii=False)
        quote_parsed = parse_qiwe_payload(
            quote_payload,
            bot_names=["二花"],
            bot_user_id="1688857683805864",
            direct_allow_all=True,
        )

        adapter = RecordingAdapter()
        asyncio.run(adapter._dispatch_message(quote_parsed))

        event = adapter.events[0]
        self.assertIn("引用消息上下文", event.text)
        self.assertIn("90%的人在AI上浪费时间", event.text)
        self.assertIn("https://example.com/article", event.text)
        self.assertIn("当前消息：把这篇文章可以转发到秦托邦的小伙伴群", event.text)
        self.assertEqual(event.raw_message["referenced_message"]["link"]["url"], "https://example.com/article")
        _RECENT_QIWE_MESSAGE_CONTEXTS.clear()

    def test_dispatch_uses_persisted_identity_cache_without_qiwe_lookup(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(
                    type(
                        "Config",
                        (),
                        {"extra": {"token": "test-token", "send_enabled": False, "answer_context_prepare_enabled": False}},
                    )()
                )
                self._identity_resolver._cache[("10733506388826175", "7881303308049798")] = (
                    time.time(),
                    adapter_module.QiWeIdentity(user_id="7881303308049798", display_name="弦默", source="room_member"),
                )
                self.events = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                raise AssertionError(f"cached identity should not call QiWe: {method}")

            async def handle_message(self, event):
                self.events.append(event)

        adapter = RecordingAdapter()
        parsed = parse_qiwe_payload(
            load_fixture("group_mention.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        asyncio.run(adapter._dispatch_message(parsed))

        self.assertEqual(adapter.events[0].source["user_name"], "弦默")
        self.assertIn('"display_name": "弦默"', adapter.events[0].channel_prompt)
        self.assertNotIn("mobile", json.dumps(adapter.events[0].raw_message, ensure_ascii=False))

    def test_dispatch_injects_prepared_answer_context(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(type("Config", (), {"extra": {"token": "test-token", "send_enabled": False}})())
                self.events = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                raise AssertionError(f"identity lookup should not call QiWe in realtime path: {method}")

            async def _prepare_answer_context(self, parsed):
                return {
                    "success": True,
                    "speaker": {"resolved": True, "display_name": "弦默"},
                    "mentioned_members": [
                        {
                            "mention_text": "阿城",
                            "resolved": True,
                            "display_name": "阿城",
                            "safe_summary": "阿城 最近的安全上下文主要与 活动、服务需求 有关。",
                            "safe_reply_hints": {"topics": ["活动", "服务需求"]},
                        }
                    ],
                    "answer_rules": {"do_not_guess_member_state": True},
                }

            async def handle_message(self, event):
                self.events.append(event)

        adapter = RecordingAdapter()
        raw = load_fixture("group_mention.json")
        raw["data"] = json.dumps(
            {
                **json.loads(raw["data"]),
                "msgData": {"content": "@二花 阿城最近参与了什么活动", "atList": []},
            },
            ensure_ascii=False,
        )
        parsed = parse_qiwe_payload(raw, bot_names=["二花"], bot_user_id="1688857683805864")
        asyncio.run(adapter._dispatch_message(parsed))

        prompt = adapter.events[0].channel_prompt
        self.assertIn("answer_context", prompt)
        self.assertIn("用户问题中提到的成员已解析", prompt)
        self.assertIn("qintopia_erhua_training_note_submit", prompt)
        self.assertIn("trainer_user_id 必须使用上方 channel_user_id", prompt)
        self.assertIn("不要回答“我不知道”", prompt)
        self.assertIn("如果用户询问该成员的状态、原因、偏好或参与情况", prompt)
        self.assertIn("阿城 => 阿城", prompt)
        self.assertIn('"mention_text": "阿城"', prompt)
        self.assertIn("服务需求", prompt)

    def test_answer_context_prompt_injects_training_guidance(self) -> None:
        raw = load_fixture("group_mention.json")
        parsed = parse_qiwe_payload(raw, bot_names=["二花"], bot_user_id="1688857683805864")
        prompt = _member_context_channel_prompt(
            parsed,
            None,
            {
                "success": True,
                "speaker": {"resolved": True},
                "mentioned_members": [],
                "training_guidance": {
                    "persona_overlays": ["回复更短一点，保留熟人感。"],
                    "member_guidance": [{"summary": "这个成员喜欢直接一点，不要客服腔。"}],
                    "reply_examples": ["这类问题先给结论，再补一句边界。"],
                },
            },
        )

        self.assertIn("回复风格增量", prompt)
        self.assertIn("这个成员喜欢直接一点", prompt)
        self.assertIn("回复示例", prompt)
        self.assertIn("不要逐字照搬", prompt)

    def test_training_note_mcp_request_uses_real_sender_as_trainer(self) -> None:
        request = _training_note_mcp_request(
            chat_id="10859791146538059",
            trainer_user_id="7881303308049798",
            training_type="member_preference",
            training_text="Cici 喜欢直接一点，不要客服腔",
            source_conversation_type="direct",
            target_channel_user_id="7881300531962448",
            source_platform_message_id="3387223631956930895",
        )
        lines = [json.loads(line) for line in request.splitlines() if line.strip()]
        call = next(item for item in lines if item.get("id") == 2)
        args = call["params"]["arguments"]

        self.assertEqual(call["params"]["name"], "qintopia_erhua_training_note_submit")
        self.assertEqual(args["caller_profile"], "erhua")
        self.assertEqual(args["trainer_user_id"], "7881303308049798")
        self.assertEqual(args["source_conversation_type"], "direct")
        self.assertEqual(args["target_channel_user_id"], "7881300531962448")
        self.assertEqual(args["source_platform_message_id"], "3387223631956930895")

    def test_answer_context_prompt_directs_unavailable_context_to_avoid_guessing(self) -> None:
        raw = load_fixture("group_mention.json")
        parsed = parse_qiwe_payload(raw, bot_names=["二花"], bot_user_id="1688857683805864")

        prompt = _member_context_channel_prompt(parsed, None, None)

        self.assertIn("Agent OS 本轮回答上下文不可用", prompt)
        self.assertIn("不要猜测", prompt)
        self.assertIn("answer_context_unavailable", prompt)

    def test_answer_context_from_mcp_stdout_parses_context(self) -> None:
        stdout = "\n".join(
            [
                json.dumps({"id": 1, "result": {}}),
                json.dumps(
                    {
                        "id": 2,
                        "result": {
                            "content": [
                                {
                                    "text": json.dumps(
                                        {
                                            "success": True,
                                            "mentioned_members": [
                                                {
                                                    "mention_text": "Cici",
                                                    "display_name": "Cici（27-29止语）",
                                                    "safe_reply_hints": {"topics": ["短期沟通状态"]},
                                                }
                                            ],
                                        },
                                        ensure_ascii=False,
                                    )
                                }
                            ]
                        },
                    },
                    ensure_ascii=False,
                ),
            ]
        )

        context = _answer_context_from_mcp_stdout(stdout)

        self.assertEqual(context["mentioned_members"][0]["display_name"], "Cici（27-29止语）")
        self.assertIn("短期沟通状态", context["mentioned_members"][0]["safe_reply_hints"]["topics"])

    def test_answer_context_request_passes_non_bot_at_list_names(self) -> None:
        request = _answer_context_mcp_request(
            chat_id="room_example",
            sender_id="user_example",
            message_text="@二花 @小乔 小乔是谁",
            mentioned_member_names=_mentioned_member_names_from_at_list(
                [
                    {"nickname": "二花", "userId": "bot-user"},
                    {"nickname": "小乔", "userId": "member-user"},
                    {"nickname": "小乔", "userId": "member-user"},
                ],
                bot_user_id="bot-user",
                bot_names=["二花"],
            ),
        )
        lines = [json.loads(line) for line in request.splitlines() if line.strip()]
        call = next(item for item in lines if item.get("id") == 2)
        args = call["params"]["arguments"]

        self.assertEqual(args["mentioned_member_names"], ["小乔"])

    def test_identity_cache_persists_and_loads_from_state_dir(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self, state_dir: str, *, fail_lookup: bool = False) -> None:
                super().__init__(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "token": "test-token",
                                "send_enabled": False,
                                "state_dir": state_dir,
                                "answer_context_prepare_enabled": False,
                            }
                        },
                    )()
                )
                self.fail_lookup = fail_lookup
                self.methods = []
                self.events = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                self.methods.append(method)
                raise AssertionError(f"identity cache test should not call QiWe: {method}")

            async def handle_message(self, event):
                self.events.append(event)

        parsed = parse_qiwe_payload(
            load_fixture("group_mention.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            first = RecordingAdapter(tmp)
            first._identity_resolver._cache[("10733506388826175", "7881303308049798")] = (
                time.time(),
                adapter_module.QiWeIdentity(user_id="7881303308049798", display_name="弦默", source="room_member"),
            )
            first._identity_resolver._save_cache_file()
            asyncio.run(first._dispatch_message(parsed))
            cache_text = (Path(tmp) / "cache" / "identity.json").read_text(encoding="utf-8")

            second = RecordingAdapter(tmp, fail_lookup=True)
            asyncio.run(second._dispatch_message(parsed))

        self.assertEqual(first.methods, [])
        self.assertEqual(first.events[0].source["user_name"], "弦默")
        self.assertEqual(second.methods, [])
        self.assertEqual(second.events[0].source["user_name"], "弦默")
        self.assertIn("弦默", cache_text)
        self.assertIn("7881303308049798", cache_text)
        self.assertNotIn("17600000000", cache_text)

    def test_audit_records_hash_without_raw_sender_id_or_token(self) -> None:
        payload = copy.deepcopy(load_fixture("group_mention.json"))
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            adapter = QiWeAdapter(type("Config", (), {"extra": {"state_dir": tmp, "audit_enabled": True}})())
            adapter._auditor.record(parsed, decision="dispatch")
            audit_line = (Path(tmp) / "audit" / "qiwe.jsonl").read_text(encoding="utf-8")

        audit = json.loads(audit_line)
        self.assertEqual(audit["sender_id_hash"][:7], "sha256:")
        self.assertNotIn("7881303308049798", audit_line)
        self.assertNotIn("QIWE_TOKEN", audit_line)

    def test_webhook_schedules_dispatch_without_waiting_for_agent(self) -> None:
        class FakeWeb:
            @staticmethod
            def json_response(data, status=200):
                return type("Response", (), {"status": status, "text": json.dumps(data)})()

        class FakeRequest:
            async def read(self):
                return json.dumps(load_fixture("group_mention.json"), ensure_ascii=False).encode("utf-8")

        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(
                    type(
                        "Config",
                        (),
                        {"extra": {"token": "test-token", "send_enabled": False, "identity_lookup_enabled": False}},
                    )()
                )
                self.dispatch_started = asyncio.Event()
                self.allow_finish = asyncio.Event()

            async def handle_message(self, event):
                self.dispatch_started.set()
                await self.allow_finish.wait()

        async def run_case():
            old_web = adapter_module.web
            adapter_module.web = FakeWeb
            adapter = RecordingAdapter()
            try:
                response = await adapter._handle_webhook(FakeRequest())
                self.assertEqual(response.status, 200)
                self.assertIn('"triggered": true', response.text)
                self.assertFalse(adapter.dispatch_started.is_set())
                await asyncio.wait_for(adapter.dispatch_started.wait(), timeout=1)
                adapter.allow_finish.set()
            finally:
                adapter_module.web = old_web

        asyncio.run(run_case())

    def test_webhook_nats_capture_defaults_off(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False}})())

        self.assertIsNone(adapter._nats_capture)

    def test_webhook_nats_capture_failure_still_acks(self) -> None:
        class FakeWeb:
            @staticmethod
            def json_response(data, status=200):
                return type("Response", (), {"status": status, "text": json.dumps(data)})()

        class FakeRequest:
            async def read(self):
                return json.dumps(load_fixture("group_normal.json"), ensure_ascii=False).encode("utf-8")

        class FailingPublisher:
            async def publish_capture(self, raw_event, message_event, *, message_id):
                raise RuntimeError("nats down")

        async def run_case():
            old_web = adapter_module.web
            old_disabled = adapter_module.logger.disabled
            adapter_module.web = FakeWeb
            adapter_module.logger.disabled = True
            adapter = QiWeAdapter(type("Config", (), {"extra": {"send_enabled": False, "nats_capture_enabled": True}})())
            adapter._nats_capture = FailingPublisher()
            try:
                response = await adapter._handle_webhook(FakeRequest())
                self.assertEqual(response.status, 200)
                self.assertIn('"triggered": false', response.text)
                await asyncio.sleep(0)
            finally:
                adapter_module.web = old_web
                adapter_module.logger.disabled = old_disabled

        asyncio.run(run_case())

    def test_location_tool_dry_run_sends_location_bundle(self) -> None:
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_TOKEN"] = "test-token"
            _LOCATION_TOOL_SEEN.clear()

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_send_location_card(
                        {
                            "chat_id": "10733506388826175",
                            "title": "秦托邦 B 栋",
                            "address": "秦托邦社区",
                            "latitude": 34.022625,
                            "longitude": 108.572545,
                            "message": "**已找到** 位置卡片",
                            "sender_id": "7881303308049798",
                            "idempotency_key": "loc-001",
                        }
                    )
                )
            )
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _LOCATION_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertIs(payload["duplicate"], False)
        self.assertEqual(payload["idempotency_key"], "loc-001")
        self.assertEqual(payload["raw_response"]["location"], {"dryRun": True})
        self.assertEqual(payload["raw_response"]["text"], {"dryRun": True})

    def test_direct_message_tool_dry_run_uses_send_text(self) -> None:
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_TOKEN"] = "test-token"
            _DIRECT_TOOL_SEEN.clear()

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_send_direct_message(
                        {
                            "recipient_user_id": "7881303308049798",
                            "message": "我来私聊补充投诉细节。",
                            "idempotency_key": "complaint-direct-1",
                            "purpose": "complaint_followup",
                        }
                    )
                )
            )

            self.assertTrue(payload["success"])
            self.assertEqual(payload["recipient_user_id"], "7881303308049798")
            self.assertEqual(payload["conversation_type"], "direct")
            self.assertEqual(payload["method"], "/msg/sendText")
            self.assertEqual(payload["purpose"], "complaint_followup")
            self.assertIs(payload["duplicate"], False)
            self.assertEqual(payload["raw_response"], {"dryRun": True})
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _DIRECT_TOOL_SEEN.clear()

    def test_direct_message_tool_requires_approval_metadata(self) -> None:
        missing_key = json.loads(
            asyncio.run(
                _handle_qiwe_send_direct_message(
                    {
                        "recipient_user_id": "7881303308049798",
                        "message": "我来私聊补充投诉细节。",
                        "purpose": "complaint_followup",
                    }
                )
            )
        )
        missing_purpose = json.loads(
            asyncio.run(
                _handle_qiwe_send_direct_message(
                    {
                        "recipient_user_id": "7881303308049798",
                        "message": "我来私聊补充投诉细节。",
                        "idempotency_key": "complaint-direct-2",
                    }
                )
            )
        )

        self.assertIs(missing_key["success"], False)
        self.assertEqual(missing_key["error"], "idempotency_key is required")
        self.assertIs(missing_purpose["success"], False)
        self.assertEqual(missing_purpose["error"], "purpose is required")

    def test_human_handoff_tool_quotes_question_and_mentions_support(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _post_qiwe_body(self, body):
                calls.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_HUMAN_HANDOFF_ENABLED"] = "true"
            os.environ["QIWE_HUMAN_HANDOFF_GROUPS_JSON"] = json.dumps(
                {"10859791146538059": {"user_id": "1688854741472532", "display_name": "秦托邦小客服"}},
                ensure_ascii=False,
            )
            _HUMAN_HANDOFF_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_handoff_to_human(
                        {
                            "chat_id": "10859791146538059",
                            "message": "这个我现在不能确认具体情况哦，麻烦帮忙看下这个问题～",
                            "original_sender_id": "7881303308049798",
                            "original_content": "还有空房吗",
                            "original_timestamp": 1782439549,
                            "original_msg_unique_identifier": "question-001",
                            "idempotency_key": "handoff-001",
                            "purpose": "authoritative_source_required",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _HUMAN_HANDOFF_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(payload["support_user_id"], "1688854741472532")
        self.assertEqual(payload["support_display_name"], "秦托邦小客服")
        self.assertEqual(payload["target_source"], "group_map")
        self.assertEqual(payload["purpose"], "authoritative_source_required")
        self.assertEqual(len(calls), 1)
        body = calls[0]
        self.assertEqual(body["method"], "/msg/sendHyperText")
        self.assertEqual(body["params"]["toId"], "10859791146538059")
        self.assertEqual(body["params"]["content"][0], {"subtype": 1, "text": "1688854741472532"})
        self.assertEqual(
            body["params"]["reply"],
            {
                "type": 0,
                "userId": "7881303308049798",
                "timeStamp": 1782439549,
                "msgUniqueIdentifier": "question-001",
                "msgData": {"content": "还有空房吗"},
            },
        )

    def test_human_handoff_tool_uses_recent_group_question_ref(self) -> None:
        calls = []

        class DispatchRecordingAdapter(QiWeAdapter):
            async def handle_message(self, event):
                return None

        class SendRecordingAdapter(QiWeAdapter):
            async def _post_qiwe_body(self, body):
                calls.append(body)
                return SendResult(success=True, raw_response={"dryRun": True})

        parsed = parse_qiwe_payload(
            load_fixture("group_mention.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            _RECENT_QIWE_MESSAGE_REFS.clear()
            dispatch_adapter = DispatchRecordingAdapter(type("Config", (), {"extra": {"send_enabled": False, "identity_lookup_enabled": False}})())
            asyncio.run(dispatch_adapter._dispatch_message(parsed))

            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_HUMAN_HANDOFF_ENABLED"] = "true"
            os.environ["QIWE_HUMAN_HANDOFF_GROUPS_JSON"] = json.dumps(
                {parsed.chat_id: {"user_id": "1688854741472532", "display_name": "秦托邦小客服"}},
                ensure_ascii=False,
            )
            os.environ["HERMES_SESSION_CHAT_ID"] = parsed.chat_id
            os.environ["HERMES_SESSION_USER_ID"] = parsed.sender_id
            _HUMAN_HANDOFF_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = SendRecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_handoff_to_human(
                        {
                            "message": "这个我现在不能确认具体情况哦，麻烦小客服帮忙看下～",
                            "purpose": "authoritative_source_required",
                            "idempotency_key": "recent-ref-handoff",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _HUMAN_HANDOFF_TOOL_SEEN.clear()
            _RECENT_QIWE_MESSAGE_REFS.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0]["params"]["reply"]["userId"], parsed.sender_id)
        self.assertEqual(calls[0]["params"]["reply"]["msgUniqueIdentifier"], parsed.message_id)
        self.assertEqual(calls[0]["params"]["reply"]["msgData"]["content"], parsed.content)

    def test_human_handoff_tool_rejects_unmapped_group(self) -> None:
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_HUMAN_HANDOFF_ENABLED"] = "true"
            os.environ["QIWE_HUMAN_HANDOFF_GROUPS_JSON"] = json.dumps(
                {"10859791146538059": {"user_id": "1688854741472532", "display_name": "秦托邦小客服"}},
                ensure_ascii=False,
            )
            _HUMAN_HANDOFF_TOOL_SEEN.clear()

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_handoff_to_human(
                        {
                            "chat_id": "10733506388826175",
                            "message": "这个我现在不能确认具体情况哦，麻烦小客服帮忙看下～",
                            "purpose": "authoritative_source_required",
                        }
                    )
                )
            )
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _HUMAN_HANDOFF_TOOL_SEEN.clear()

        self.assertIs(payload["success"], False)
        self.assertEqual(payload["error"], "no human handoff target configured for this group")
        self.assertEqual(payload["chat_id"], "10733506388826175")

    def test_human_handoff_tool_requires_enabled_config(self) -> None:
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ.pop("QIWE_HUMAN_HANDOFF_ENABLED", None)
            os.environ["QIWE_HUMAN_HANDOFF_USER_ID"] = "1688854741472532"
            _HUMAN_HANDOFF_TOOL_SEEN.clear()

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_handoff_to_human(
                        {
                            "chat_id": "10859791146538059",
                            "message": "这个我现在不能确认具体情况哦，麻烦帮忙看下这个问题～",
                            "purpose": "authoritative_source_required",
                        }
                    )
                )
            )
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _HUMAN_HANDOFF_TOOL_SEEN.clear()

        self.assertIs(payload["success"], False)
        self.assertEqual(payload["error"], "QiWe human handoff is disabled")

    def test_direct_message_guard_suggests_contact_request_in_group_context(self) -> None:
        class GuardedAdapter(QiWeAdapter):
            async def _ensure_direct_recipient_sendable(self, user_id, *, guid=""):
                return SendResult(
                    success=False,
                    error="QiWe direct recipient was not found in external contacts",
                    raw_response={
                        "contactGuard": {
                            "userId": user_id,
                            "allowed": False,
                            "contactType": None,
                            "reason": "not_found",
                            "cached": False,
                        }
                    },
                )

            def _build_send_body(self, *args, **kwargs):
                raise AssertionError("send body should not be built when guard blocks")

        original = adapter_module.QiWeAdapter
        original_aiohttp_available = adapter_module.AIOHTTP_AVAILABLE
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "true"
            os.environ["HERMES_SESSION_CHAT_ID"] = "10789255155259073"
            os.environ["HERMES_SESSION_USER_ID"] = "7881302006036777"
            _DIRECT_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = GuardedAdapter
            adapter_module.AIOHTTP_AVAILABLE = True

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_send_direct_message(
                        {
                            "recipient_user_id": "7881302006036777",
                            "message": "我来私聊补充投诉细节。",
                            "idempotency_key": "complaint-direct-guard-hint",
                            "purpose": "complaint_followup",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            adapter_module.AIOHTTP_AVAILABLE = original_aiohttp_available
            os.environ.clear()
            os.environ.update(old_env)
            _DIRECT_TOOL_SEEN.clear()

        self.assertIs(payload["success"], False)
        self.assertEqual(payload["error"], "QiWe direct recipient was not found in external contacts")
        suggestion = payload["raw_response"]["suggestedNextTool"]
        self.assertEqual(suggestion["name"], "qiwe_request_direct_contact")
        self.assertEqual(suggestion["mode"], "room_member")
        self.assertEqual(suggestion["user_id"], "7881302006036777")
        self.assertEqual(suggestion["room_id"], "10789255155259073")
        self.assertEqual(suggestion["purpose"], "complaint_followup")
        self.assertIs(suggestion["requiresApproval"], True)

    def test_direct_message_tool_is_idempotent(self) -> None:
        old_env = dict(os.environ)
        args = {
            "recipient_user_id": "7881303308049798",
            "message": "投诉处理结果已确认。",
            "idempotency_key": "complaint-direct-duplicate",
            "purpose": "complaint_resolution",
        }
        try:
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_TOKEN"] = "test-token"
            _DIRECT_TOOL_SEEN.clear()

            first = json.loads(asyncio.run(_handle_qiwe_send_direct_message(args)))
            second = json.loads(asyncio.run(_handle_qiwe_send_direct_message(args)))
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _DIRECT_TOOL_SEEN.clear()

        self.assertIs(first["success"], True)
        self.assertIs(first["duplicate"], False)
        self.assertIs(second["success"], True)
        self.assertIs(second["duplicate"], True)

    def test_contact_request_tool_room_member_calls_add_room_contact(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                calls.append((method, params, require_send_enabled))
                return SendResult(
                    success=True,
                    raw_response={
                        "code": 0,
                        "msg": "ok",
                        "data": [{"mobile": "17600000000", "remark": "不应泄露"}],
                    },
                )

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_GUID"] = "guid-1"
            _CONTACT_REQUEST_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_request_direct_contact(
                        {
                            "mode": "room_member",
                            "user_id": "7881303308049798",
                            "room_id": "10733506388826175",
                            "verify_text": "我是二花，来跟进你的投诉。",
                            "purpose": "complaint_followup",
                            "idempotency_key": "contact-request-room-1",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _CONTACT_REQUEST_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertIs(payload["duplicate"], False)
        self.assertEqual(payload["method"], "/contact/addRoomContact")
        self.assertEqual(payload["mode"], "room_member")
        self.assertEqual(payload["user_id"], "7881303308049798")
        self.assertEqual(payload["room_id"], "10733506388826175")
        self.assertEqual(payload["purpose"], "complaint_followup")
        self.assertEqual(payload["idempotency_key"], "contact-request-room-1")
        self.assertEqual(payload["qiwe_code"], 0)
        self.assertEqual(payload["qiwe_msg"], "ok")
        encoded = json.dumps(payload, ensure_ascii=False)
        self.assertNotIn("17600000000", encoded)
        self.assertNotIn("不应泄露", encoded)
        self.assertEqual(calls[0][0], "/contact/addRoomContact")
        self.assertEqual(
            calls[0][1],
            {
                "guid": "guid-1",
                "userId": "7881303308049798",
                "verifyText": "我是二花，来跟进你的投诉。",
                "roomId": "10733506388826175",
            },
        )

    def test_contact_request_tool_defaults_to_current_group_context(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                calls.append((method, params, require_send_enabled))
                return SendResult(success=True, raw_response={"code": 0, "msg": "ok", "data": [{}]})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_GUID"] = "guid-1"
            os.environ["HERMES_SESSION_CHAT_ID"] = "10789255155259073"
            os.environ["HERMES_SESSION_USER_ID"] = "7881302006036777"
            _CONTACT_REQUEST_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_request_direct_contact(
                        {
                            "verify_text": "我是二花，来跟进你刚才在群里的投诉。",
                            "purpose": "complaint_followup",
                            "idempotency_key": "contact-request-context-default",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _CONTACT_REQUEST_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(payload["method"], "/contact/addRoomContact")
        self.assertEqual(payload["mode"], "room_member")
        self.assertEqual(payload["user_id"], "7881302006036777")
        self.assertEqual(payload["room_id"], "10789255155259073")
        self.assertEqual(
            calls[0][1],
            {
                "guid": "guid-1",
                "userId": "7881302006036777",
                "verifyText": "我是二花，来跟进你刚才在群里的投诉。",
                "roomId": "10789255155259073",
            },
        )

    def test_contact_request_tool_rejects_room_member_without_room_id(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                calls.append(method)
                raise AssertionError("contact request should not call QiWe")

        original = adapter_module.QiWeAdapter
        try:
            adapter_module.QiWeAdapter = RecordingAdapter
            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_request_direct_contact(
                        {
                            "mode": "room_member",
                            "user_id": "7881303308049798",
                            "verify_text": "我是二花，来跟进你的投诉。",
                            "purpose": "complaint_followup",
                            "idempotency_key": "contact-request-missing-room",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original

        self.assertIs(payload["success"], False)
        self.assertEqual(payload["error"], "room_id is required for room_member mode")
        self.assertEqual(calls, [])

    def test_contact_request_tool_deleted_contact_calls_add_deleted_contact(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                calls.append((method, params, require_send_enabled))
                return SendResult(success=True, raw_response={"code": 0, "msg": "ok", "data": [{}]})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_GUID"] = "guid-1"
            _CONTACT_REQUEST_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_request_direct_contact(
                        {
                            "mode": "deleted_contact",
                            "user_id": "7881303308049798",
                            "verify_text": "我是二花，来继续跟进你的投诉。",
                            "purpose": "complaint_followup",
                            "idempotency_key": "contact-request-deleted-1",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _CONTACT_REQUEST_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(payload["method"], "/contact/addDeletedContact")
        self.assertEqual(payload["mode"], "deleted_contact")
        self.assertEqual(payload["room_id"], "")
        self.assertEqual(
            calls[0][1],
            {
                "guid": "guid-1",
                "userId": "7881303308049798",
                "verifyText": "我是二花，来继续跟进你的投诉。",
            },
        )

    def test_contact_request_tool_requires_approval_metadata(self) -> None:
        base = {
            "mode": "deleted_contact",
            "user_id": "7881303308049798",
            "verify_text": "我是二花，来继续跟进你的投诉。",
            "purpose": "complaint_followup",
            "idempotency_key": "contact-request-required",
        }

        cases = [
            ("user_id", "user_id is required"),
            ("verify_text", "verify_text is required"),
            ("purpose", "purpose is required"),
            ("idempotency_key", "idempotency_key is required"),
        ]
        for field, error in cases:
            args = dict(base)
            args.pop(field)
            payload = json.loads(asyncio.run(_handle_qiwe_request_direct_contact(args)))
            self.assertIs(payload["success"], False)
            self.assertEqual(payload["error"], error)

    def test_contact_request_tool_is_idempotent(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                calls.append(method)
                return SendResult(success=True, raw_response={"code": 0, "msg": "ok", "data": [{}]})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        args = {
            "mode": "deleted_contact",
            "user_id": "7881303308049798",
            "verify_text": "我是二花，来继续跟进你的投诉。",
            "purpose": "complaint_followup",
            "idempotency_key": "contact-request-duplicate",
        }
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            _CONTACT_REQUEST_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            first = json.loads(asyncio.run(_handle_qiwe_request_direct_contact(args)))
            second = json.loads(asyncio.run(_handle_qiwe_request_direct_contact(args)))
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _CONTACT_REQUEST_TOOL_SEEN.clear()

        self.assertIs(first["success"], True)
        self.assertIs(first["duplicate"], False)
        self.assertIs(second["success"], True)
        self.assertIs(second["duplicate"], True)
        self.assertEqual(calls, ["/contact/addDeletedContact"])

    def test_voice_to_text_is_disabled_by_default(self) -> None:
        adapter = QiWeAdapter(type("Config", (), {"extra": {"token": "test-token", "send_enabled": False}})())
        result = asyncio.run(adapter._voice_to_text("1003001"))

        self.assertIs(result.success, False)
        self.assertEqual(result.error, "QiWe voice transcription is disabled")

    def test_voice_to_text_helper_calls_qiwe_when_enabled(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self) -> None:
                super().__init__(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "token": "test-token",
                                "send_enabled": False,
                                "voice_to_text_enabled": True,
                                "voice_to_text_poll_interval_seconds": 0,
                            }
                        },
                    )()
                )
                self.calls = []

            async def _call_qiwe_api(self, method, params, *, require_send_enabled=True):
                self.calls.append((method, params, require_send_enabled))
                if method == "/msg/voiceToTextApply":
                    return SendResult(success=True, raw_response={"code": 0, "data": {"voiceId": "voice-id-1"}})
                if method == "/msg/voiceToTextQuery":
                    return SendResult(success=True, raw_response={"code": 0, "data": {"isEnd": True, "text": "语音内容"}})
                raise AssertionError(f"unexpected method: {method}")

        adapter = RecordingAdapter()
        result = asyncio.run(adapter._voice_to_text("1003001", guid="guid-1"))

        self.assertIs(result.success, True)
        self.assertEqual(result.raw_response["voiceId"], "voice-id-1")
        self.assertEqual(result.raw_response["text"], "语音内容")
        self.assertEqual(adapter.calls[0], ("/msg/voiceToTextApply", {"guid": "guid-1", "msgServerId": 1003001}, False))
        self.assertEqual(adapter.calls[1][0], "/msg/voiceToTextQuery")

    def test_rich_message_tool_sends_link_and_returns_safe_status(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _post_qiwe_body(self, body):
                calls.append(body)
                return SendResult(
                    success=True,
                    raw_response={
                        "code": 0,
                        "msg": "成功",
                        "data": {
                            "isSendSuccess": 1,
                            "msgServerId": 1024,
                            "msgType": 13,
                            "msgUniqueIdentifier": "unique-link",
                            "seq": 55,
                            "timestamp": 1782737285,
                            "fileAesKey": "must-not-leak",
                        },
                    },
                )

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            _RICH_MESSAGE_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_send_rich_message(
                        {
                            "chat_id": "10859791146538059",
                            "message_type": "link",
                            "payload": {
                                "title": "文章标题",
                                "link_url": "https://example.com/article",
                                "desc": "文章摘要",
                                "icon_url": "https://example.com/icon.png",
                            },
                            "conversation_type": "group",
                            "purpose": "forward_approved_article",
                            "idempotency_key": "rich-link-1",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _RICH_MESSAGE_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(payload["method"], "/msg/sendLink")
        self.assertEqual(payload["message_type"], "link")
        self.assertEqual(payload["qiwe_code"], 0)
        self.assertEqual(payload["message"]["msgServerId"], 1024)
        encoded = json.dumps(payload, ensure_ascii=False)
        self.assertNotIn("must-not-leak", encoded)
        self.assertEqual(calls[0]["params"]["linkUrl"], "https://example.com/article")

    def test_rich_message_tool_rejects_missing_required_payload_field_without_dedupe(self) -> None:
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            _RICH_MESSAGE_TOOL_SEEN.clear()
            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_send_rich_message(
                        {
                            "chat_id": "10859791146538059",
                            "message_type": "image",
                            "payload": {"file_id": "file-only"},
                            "purpose": "send_image",
                            "idempotency_key": "rich-invalid",
                        }
                    )
                )
            )
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _RICH_MESSAGE_TOOL_SEEN.clear()

        self.assertIs(payload["success"], False)
        self.assertEqual(payload["error"], "file_aes_key is required for image")

    def test_rich_message_tool_is_idempotent(self) -> None:
        old_env = dict(os.environ)
        args = {
            "chat_id": "10859791146538059",
            "message_type": "personal_card",
            "payload": {"shared_id": "7881302799122145"},
            "purpose": "share_contact_card",
            "idempotency_key": "rich-card-duplicate",
        }
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            _RICH_MESSAGE_TOOL_SEEN.clear()
            first = json.loads(asyncio.run(_handle_qiwe_send_rich_message(args)))
            second = json.loads(asyncio.run(_handle_qiwe_send_rich_message(args)))
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _RICH_MESSAGE_TOOL_SEEN.clear()

        self.assertIs(first["success"], True)
        self.assertIs(first["duplicate"], False)
        self.assertIs(second["success"], True)
        self.assertIs(second["duplicate"], True)

    def test_rich_message_tool_group_forward_from_direct_defaults_to_home_group(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _post_qiwe_body(self, body):
                calls.append(body)
                return SendResult(success=True, raw_response={"code": 0, "msg": "成功", "data": {"msgServerId": 2048}})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_HOME_GROUP"] = "10859791146538059"
            os.environ["HERMES_SESSION_CHAT_ID"] = "7881303308049798"
            os.environ["HERMES_SESSION_USER_ID"] = "7881303308049798"
            _RICH_MESSAGE_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_send_rich_message(
                        {
                            "conversation_type": "group",
                            "message_type": "link",
                            "payload": {
                                "title": "文章标题",
                                "link_url": "https://example.com/article",
                            },
                            "purpose": "forward_approved_article",
                            "idempotency_key": "rich-direct-to-home-group",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _RICH_MESSAGE_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(payload["chat_id"], "10859791146538059")
        self.assertEqual(calls[0]["params"]["toId"], "10859791146538059")

    def test_revoke_message_tool_calls_qiwe_revoke(self) -> None:
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def _post_qiwe_body(self, body):
                calls.append(body)
                return SendResult(success=True, raw_response={"code": 0, "msg": "成功", "data": [{}]})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_GUID"] = "guid-1"
            _REVOKE_MESSAGE_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_revoke_message(
                        {
                            "chat_id": "10859791146538059",
                            "msg_server_id": 1121922,
                            "purpose": "correct_wrong_link",
                            "idempotency_key": "revoke-1",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _REVOKE_MESSAGE_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(payload["method"], "/msg/revokeMsg")
        self.assertEqual(calls[0]["params"], {"guid": "guid-1", "chatId": "10859791146538059", "msgServerId": 1121922})

    def test_voice_to_text_tool_requires_enabled_helper_and_returns_text(self) -> None:
        class RecordingAdapter(QiWeAdapter):
            def __init__(self, config) -> None:
                super().__init__(type("Config", (), {"extra": {"token": "test-token", "send_enabled": False, "voice_to_text_enabled": True}})())

            async def _voice_to_text(self, msg_server_id, *, guid=""):
                return SendResult(success=True, raw_response={"voiceId": "voice-id-1", "text": "语音内容", "raw_response": {"redacted": True}})

        original = adapter_module.QiWeAdapter
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["QIWE_SEND_ENABLED"] = "false"
            _VOICE_TO_TEXT_TOOL_SEEN.clear()
            adapter_module.QiWeAdapter = RecordingAdapter

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_voice_to_text(
                        {
                            "msg_server_id": 1003001,
                            "purpose": "transcribe_user_voice",
                            "idempotency_key": "voice-to-text-1",
                        }
                    )
                )
            )
        finally:
            adapter_module.QiWeAdapter = original
            os.environ.clear()
            os.environ.update(old_env)
            _VOICE_TO_TEXT_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertEqual(payload["voice_id"], "voice-id-1")
        self.assertEqual(payload["text"], "语音内容")
        self.assertEqual(payload["msg_server_id"], 1003001)

    def test_location_tool_defaults_to_current_gateway_context(self) -> None:
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_TOKEN"] = "test-token"
            os.environ["HERMES_SESSION_CHAT_ID"] = "7881303308049798"
            os.environ["HERMES_SESSION_USER_ID"] = "7881303308049798"
            _LOCATION_TOOL_SEEN.clear()

            payload = json.loads(
                asyncio.run(
                    _handle_qiwe_send_location_card(
                        {
                            "title": "秦托邦1栋",
                            "address": "秦托邦1栋",
                            "latitude": 34.024317,
                            "longitude": 108.572849,
                            "message": "给你位置卡片啦",
                            "idempotency_key": "loc-context-001",
                        }
                    )
                )
            )
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _LOCATION_TOOL_SEEN.clear()

        self.assertIs(payload["success"], True)
        self.assertIs(payload["duplicate"], False)
        self.assertEqual(payload["raw_response"]["location"], {"dryRun": True})

    def test_location_tool_is_idempotent(self) -> None:
        old_env = dict(os.environ)
        args = {
            "chat_id": "10733506388826175",
            "title": "秦托邦 B 栋",
            "latitude": 34.022625,
            "longitude": 108.572545,
            "idempotency_key": "loc-duplicate",
        }
        try:
            os.environ["QIWE_SEND_ENABLED"] = "false"
            os.environ["QIWE_TOKEN"] = "test-token"
            _LOCATION_TOOL_SEEN.clear()

            first = json.loads(asyncio.run(_handle_qiwe_send_location_card(args)))
            second = json.loads(asyncio.run(_handle_qiwe_send_location_card(args)))
        finally:
            os.environ.clear()
            os.environ.update(old_env)
            _LOCATION_TOOL_SEEN.clear()

        self.assertIs(first["success"], True)
        self.assertIs(first["duplicate"], False)
        self.assertIs(second["success"], True)
        self.assertIs(second["duplicate"], True)

    def test_location_tool_rejects_missing_coordinates(self) -> None:
        payload = json.loads(
            asyncio.run(
                _handle_qiwe_send_location_card(
                    {
                        "chat_id": "10733506388826175",
                        "title": "秦托邦 B 栋",
                    }
                )
            )
        )

        self.assertIs(payload["success"], False)
        self.assertEqual(payload["error"], "latitude is required")

    def test_register_exposes_platform_and_location_tool(self) -> None:
        class FakeContext:
            def __init__(self) -> None:
                self.platforms = []
                self.tools = []

            def register_platform(self, **kwargs) -> None:
                self.platforms.append(kwargs)

            def register_tool(self, **kwargs) -> None:
                self.tools.append(kwargs)

        ctx = FakeContext()
        register(ctx)

        self.assertEqual(ctx.platforms[0]["name"], "qiwe")
        self.assertEqual(ctx.tools[0]["name"], "qiwe_send_location_card")
        self.assertIn("qiwe_send_direct_message", [tool["name"] for tool in ctx.tools])
        self.assertIn("qiwe_send_rich_message", [tool["name"] for tool in ctx.tools])
        self.assertIn("qiwe_revoke_message", [tool["name"] for tool in ctx.tools])
        self.assertIn("qiwe_voice_to_text", [tool["name"] for tool in ctx.tools])
        self.assertIn("qiwe_request_direct_contact", [tool["name"] for tool in ctx.tools])
        self.assertEqual(ctx.tools[0]["toolset"], "qiwe")
        self.assertIs(ctx.tools[0]["is_async"], True)

    def test_passive_pipeline_disabled_does_not_process_solitaire(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            pipeline = PassiveEventPipeline(
                PassivePipelineConfig(
                    enabled=False,
                    passive_enabled=True,
                    solitaire_enabled=True,
                    state_dir=tmp,
                )
            )
            asyncio.run(pipeline.handle(normalized_event_from_parsed(parsed)))
            self.assertFalse((Path(tmp) / "solitaire" / "activities.json").exists())

    def test_passive_pipeline_records_solitaire_when_enabled(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            pipeline = PassiveEventPipeline(
                PassivePipelineConfig(
                    enabled=True,
                    passive_enabled=True,
                    solitaire_enabled=True,
                    state_dir=tmp,
                ),
                content_parser=FakeSolitaireContentParser(),
            )
            asyncio.run(pipeline.handle(normalized_event_from_parsed(parsed)))
            activity_payload = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))
            parse_attempts = [
                json.loads(line)
                for line in (Path(tmp) / "solitaire" / "parse_attempts.jsonl").read_text(encoding="utf-8").splitlines()
            ]
            self.assertTrue((Path(tmp) / "solitaire" / "feishu_sync_jobs.jsonl").exists())

        self.assertEqual(len(activity_payload), 1)
        activity = next(iter(activity_payload.values()))
        self.assertEqual(activity["activity_subject"], "接龙数据格式测试")
        self.assertEqual(activity["participant_names"], ["弦默"])
        self.assertEqual(parse_attempts[0]["reason"], "activity_parsed")
        self.assertIs(parse_attempts[0]["handled"], True)

    def test_passive_pipeline_without_content_parser_does_not_process_solitaire(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            pipeline = PassiveEventPipeline(
                PassivePipelineConfig(
                    enabled=True,
                    passive_enabled=True,
                    solitaire_enabled=True,
                    state_dir=tmp,
                )
            )
            result = asyncio.run(pipeline.handle(normalized_event_from_parsed(parsed)))
            self.assertFalse((Path(tmp) / "solitaire" / "activities.json").exists())
            parse_attempts = [
                json.loads(line)
                for line in (Path(tmp) / "solitaire" / "parse_attempts.jsonl").read_text(encoding="utf-8").splitlines()
            ]

        self.assertIsNotNone(result)
        self.assertIs(result.handled, False)
        self.assertEqual(parse_attempts[0]["reason"], "not_activity_or_invalid_parse")
        self.assertIs(parse_attempts[0]["handled"], False)

    def test_passive_solitaire_ack_sends_only_when_enabled_for_allowed_group(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                calls.append((chat_id, content, metadata))
                return SendResult(success=True, raw_response={"dryRun": True})

        with tempfile.TemporaryDirectory() as tmp:
            adapter = RecordingAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "pipeline_enabled": True,
                            "passive_pipeline_enabled": True,
                            "solitaire_processor_enabled": True,
                            "passive_allowed_groups": ["10789255155259073"],
                            "passive_ack_enabled": True,
                            "passive_ack_allowed_groups": ["10789255155259073"],
                            "state_dir": tmp,
                        }
                    },
                )()
            )
            adapter._passive_pipeline.activity_service.content_parser = FakeSolitaireContentParser()
            asyncio.run(adapter._passive_pipeline_safe(parsed))

        self.assertEqual(calls[0][0], "10789255155259073")
        self.assertIn("二花看到有活动啦：接龙数据格式测试", calls[0][1])
        self.assertIn("时间二花也记下了：2026-06-11", calls[0][1])
        self.assertIn("活动开始前 30 分钟来群里提醒", calls[0][1])
        self.assertNotIn("参与人数", calls[0][1])
        self.assertNotIn("参与人", calls[0][1])
        self.assertEqual(calls[0][2], {"conversation_type": "group", "chat_type": "group"})

    def test_passive_solitaire_ack_mentions_current_month_time_correction(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw = json.loads(payload["data"])
        raw["fromRoomId"] = 10859791146538059
        raw["timestamp"] = 1781485555
        raw["msgData"]["title"] = "#接龙\n这个周一下午，来和 贺妈妈 一起穿针引线\n\n📅 时间：4/15（周一）下午 2:30\n\n1. 秦托邦小客服"
        payload["fromGroup"] = "10859791146538059"
        payload["data"] = json.dumps(raw, ensure_ascii=False)
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                calls.append((chat_id, content, metadata))
                return SendResult(success=True, raw_response={"dryRun": True})

        with tempfile.TemporaryDirectory() as tmp:
            adapter = RecordingAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "pipeline_enabled": True,
                            "passive_pipeline_enabled": True,
                            "solitaire_processor_enabled": True,
                            "passive_allowed_groups": ["10859791146538059"],
                            "passive_ack_enabled": True,
                            "passive_ack_allowed_groups": ["10859791146538059"],
                            "state_dir": tmp,
                        }
                    },
                )()
            )
            adapter._passive_pipeline.activity_service.content_parser = FakeSolitaireContentParser(
                subject="和贺妈妈一起穿针引线",
                activity_type="手作体验",
                start_time="2024-04-15 14:30",
                participants=["秦托邦小客服"],
            )
            asyncio.run(adapter._passive_pipeline_safe(parsed))

        self.assertEqual(calls[0][0], "10859791146538059")
        self.assertIn("时间二花也记下了：2026-06-15 14:30", calls[0][1])
        self.assertIn("当前月份", calls[0][1])

    def test_passive_solitaire_ack_lightly_reminds_when_created_within_thirty_minutes(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw = json.loads(payload["data"])
        raw["timestamp"] = 1781176277
        raw["msgUniqueIdentifier"] = "near-start-solitaire"
        payload["data"] = json.dumps(raw, ensure_ascii=False)
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                calls.append((chat_id, content, metadata))
                return SendResult(success=True, raw_response={"dryRun": True})

        with tempfile.TemporaryDirectory() as tmp:
            adapter = RecordingAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "pipeline_enabled": True,
                            "passive_pipeline_enabled": True,
                            "solitaire_processor_enabled": True,
                            "passive_allowed_groups": ["10789255155259073"],
                            "passive_ack_enabled": True,
                            "passive_ack_allowed_groups": ["10789255155259073"],
                            "state_dir": tmp,
                        }
                    },
                )()
            )
            adapter._passive_pipeline.activity_service.content_parser = FakeSolitaireContentParser(
                start_time="2026-06-11 19:30"
            )
            asyncio.run(adapter._passive_pipeline_safe(parsed))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        self.assertEqual(len(calls), 1)
        self.assertIn("不到 30 分钟", calls[0][1])
        self.assertIn("先轻轻提醒一下", calls[0][1])
        self.assertNotIn("活动开始前 30 分钟来群里提醒", calls[0][1])
        self.assertEqual(reminders, {})

    def test_passive_solitaire_ack_is_not_blocked_by_feishu_sync(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                calls.append((chat_id, content, metadata))
                return SendResult(success=True, raw_response={"dryRun": True})

        async def run_case():
            with tempfile.TemporaryDirectory() as tmp:
                adapter = RecordingAdapter(
                    type(
                        "Config",
                        (),
                        {
                            "extra": {
                                "pipeline_enabled": True,
                                "passive_pipeline_enabled": True,
                                "solitaire_processor_enabled": True,
                                "passive_allowed_groups": ["10789255155259073"],
                                "passive_ack_enabled": True,
                                "passive_ack_allowed_groups": ["10789255155259073"],
                                "state_dir": tmp,
                            }
                        },
                    )()
                )
                writer = BlockingFeishuWriter()
                writer.loop = asyncio.get_running_loop()
                adapter._passive_pipeline.activity_service.writer = writer
                adapter._passive_pipeline.activity_service.content_parser = FakeSolitaireContentParser()
                await asyncio.wait_for(adapter._passive_pipeline_safe(parsed), timeout=0.5)
                await asyncio.wait_for(writer.started.wait(), timeout=0.5)
                self.assertEqual(len(calls), 1)
                self.assertIn("二花看到有活动啦：接龙数据格式测试", calls[0][1])
                writer.release.set()
                await asyncio.sleep(0.05)

        asyncio.run(run_case())

    def test_passive_solitaire_ack_sends_only_for_first_activity_snapshot(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        calls = []

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                calls.append((chat_id, content, metadata))
                return SendResult(success=True, raw_response={"dryRun": True})

        with tempfile.TemporaryDirectory() as tmp:
            adapter = RecordingAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "pipeline_enabled": True,
                            "passive_pipeline_enabled": True,
                            "solitaire_processor_enabled": True,
                            "passive_allowed_groups": ["10789255155259073"],
                            "passive_ack_enabled": True,
                            "passive_ack_allowed_groups": ["10789255155259073"],
                            "state_dir": tmp,
                        }
                    },
                )()
            )
            adapter._passive_pipeline.activity_service.content_parser = FakeSolitaireContentParser()
            asyncio.run(adapter._passive_pipeline_safe(parsed))
            asyncio.run(adapter._passive_pipeline_safe(parsed))

        self.assertEqual(len(calls), 1)

    def test_passive_solitaire_ack_is_disabled_by_default(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        class RecordingAdapter(QiWeAdapter):
            async def send(self, chat_id, content, reply_to=None, metadata=None):
                raise AssertionError("passive ack should be disabled by default")

        with tempfile.TemporaryDirectory() as tmp:
            adapter = RecordingAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "pipeline_enabled": True,
                            "passive_pipeline_enabled": True,
                            "solitaire_processor_enabled": True,
                            "passive_allowed_groups": ["10789255155259073"],
                            "state_dir": tmp,
                        }
                    },
                )()
            )
            adapter._passive_pipeline.activity_service.content_parser = FakeSolitaireContentParser()
            asyncio.run(adapter._passive_pipeline_safe(parsed))

    def test_passive_pipeline_respects_allowed_groups(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            pipeline = PassiveEventPipeline(
                PassivePipelineConfig(
                    enabled=True,
                    passive_enabled=True,
                    solitaire_enabled=True,
                    state_dir=tmp,
                    allowed_groups=["other-group"],
                ),
                content_parser=FakeSolitaireContentParser(),
            )
            asyncio.run(pipeline.handle(normalized_event_from_parsed(parsed)))
            self.assertFalse((Path(tmp) / "solitaire" / "activities.json").exists())

    def test_passive_pipeline_reminder_jobs_are_idempotent(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            pipeline = PassiveEventPipeline(
                PassivePipelineConfig(
                    enabled=True,
                    passive_enabled=True,
                    solitaire_enabled=True,
                    state_dir=tmp,
                ),
                content_parser=FakeSolitaireContentParser(),
            )
            pipeline.activity_service.repository.reminder_policy = ReminderPolicy(
                default=["60m"],
                by_activity_type={"社区活动": ["60m", "120m"]},
            )
            event = normalized_event_from_parsed(parsed)
            asyncio.run(pipeline.handle(event))
            asyncio.run(pipeline.handle(event))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        self.assertEqual(len(reminders), 2)
        self.assertIn("before_60m", "\n".join(reminders))
        self.assertIn("before_120m", "\n".join(reminders))

    def test_reminder_jobs_update_source_message_ref_for_latest_snapshot(self) -> None:
        first = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        second_payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        second_raw = json.loads(second_payload["data"])
        second_raw["msgServerId"] = 2026002
        second_raw["msgUniqueIdentifier"] = "latest-solitaire-message"
        second_raw["timestamp"] = 1781176888
        second_payload["data"] = json.dumps(second_raw, ensure_ascii=False)
        second = parse_qiwe_payload(second_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(first)))
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(second)))
            activities = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        activity = next(iter(activities.values()))
        self.assertEqual(activity["solitaire_created_at"], "2026-06-11T11:10:32+00:00")
        self.assertEqual(activity["last_seen_at"], "2026-06-11T11:21:28+00:00")
        job = next(iter(reminders.values()))
        self.assertEqual(job["source_message_ref"]["msgServerId"], "2026002")
        self.assertEqual(job["source_message_ref"]["msgUniqueIdentifier"], "latest-solitaire-message")
        self.assertEqual(job["source_message_ref"]["timeStamp"], 1781176888)

    def test_reminder_policy_sample_covers_feishu_activity_types(self) -> None:
        policy = ReminderPolicy.load(str(Path("docs/examples/activity-reminder-policy.sample.json")))

        self.assertEqual(policy.missing_activity_types(FEISHU_ACTIVITY_TYPES), [])
        self.assertEqual([offset.label for offset in policy.offsets_for("运动娱乐")], ["before_30m"])
        self.assertEqual([offset.label for offset in policy.offsets_for("户外运动🏃‍♀️")], ["before_30m"])

    def test_reminder_policy_defaults_to_thirty_minutes_for_unconfigured_type(self) -> None:
        policy = ReminderPolicy(by_activity_type={})

        self.assertEqual([offset.label for offset in policy.offsets_for("未知类型")], ["before_30m"])

    def test_reminder_job_due_at_uses_activity_timezone(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_ACTIVITY_TIMEZONE"] = "Asia/Shanghai"
            with tempfile.TemporaryDirectory() as tmp:
                service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser(start_time="2026-06-13 15:30"))
                asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
                reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))
        finally:
            os.environ.clear()
            os.environ.update(old_env)

        job = next(iter(reminders.values()))
        self.assertEqual(job["reminder_type"], "before_30m")
        self.assertEqual(job["due_at"], "2026-06-13T07:00:00+00:00")

    def test_reminder_job_is_not_created_when_first_seen_within_thirty_minutes(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw = json.loads(payload["data"])
        raw["timestamp"] = 1781176277
        raw["msgUniqueIdentifier"] = "near-start-no-job"
        payload["data"] = json.dumps(raw, ensure_ascii=False)
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(start_time="2026-06-11 19:30"),
            )
            result = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        self.assertIs(result.handled, True)
        self.assertIs(result.immediate_reminder, True)
        self.assertEqual(reminders, {})

    def test_reminder_policy_change_removes_stale_unsent_jobs(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            repository = ActivityRepository(
                tmp,
                ReminderPolicy(default=["60m"], by_activity_type={"社区活动": ["60m", "120m"]}),
            )
            service = ActivityService(repository, FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            reminders_path = Path(tmp) / "solitaire" / "reminders.json"
            reminders = json.loads(reminders_path.read_text(encoding="utf-8"))
            old_job_id = next(job_id for job_id in reminders if job_id.endswith("before_120m"))
            reminders[old_job_id]["sent"] = False
            reminders_path.write_text(json.dumps(reminders, ensure_ascii=False), encoding="utf-8")

            repository.reminder_policy = ReminderPolicy(default=["60m"], by_activity_type={"社区活动": ["60m"]})
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            updated = json.loads(reminders_path.read_text(encoding="utf-8"))

        self.assertEqual(len(updated), 1)
        self.assertIn("before_60m", next(iter(updated)))
        self.assertNotIn(old_job_id, updated)

    def test_activity_service_exposes_due_reminders_and_feishu_queue(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            result = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            due = service.due_reminders(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc))
            sync_queue = (Path(tmp) / "solitaire" / "feishu_sync_jobs.jsonl").read_text(encoding="utf-8")

        self.assertIs(result.handled, True)
        self.assertEqual(result.participant_count, 1)
        self.assertEqual(len(due), 1)
        self.assertIn("queued", sync_queue)
        self.assertEqual(due[0].source_message_ref["msgServerId"], "1017194")
        self.assertEqual(due[0].source_message_ref["msgType"], 213)
        self.assertEqual(due[0].source_message_ref["msgUniqueIdentifier"], "8971589583608054865")
        self.assertEqual(due[0].source_message_ref["timeStamp"], 1781176277)
        self.assertIn("title", due[0].source_message_ref["msgData"])

    def test_activity_service_records_parse_failure_diagnostic(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        content_parser = HermesSolitaireContentParser(FakeHermesLlm(text="这是活动，但我没有输出 JSON"))
        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), content_parser)
            result = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            attempts = [
                json.loads(line)
                for line in (Path(tmp) / "solitaire" / "parse_attempts.jsonl").read_text(encoding="utf-8").splitlines()
            ]

        self.assertIs(result.handled, False)
        self.assertEqual(attempts[0]["reason"], "not_activity_or_invalid_parse")
        self.assertEqual(attempts[0]["diagnostic_reason"], "invalid_json")
        self.assertIn("这是活动", attempts[0]["diagnostic_preview"])

    def test_activity_service_schedules_feishu_sync_without_blocking_result(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        async def run_case():
            with tempfile.TemporaryDirectory() as tmp:
                writer = BlockingFeishuWriter()
                writer.loop = asyncio.get_running_loop()
                service = ActivityService(ActivityRepository(tmp), writer, FakeSolitaireContentParser())
                result = await asyncio.wait_for(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)), timeout=0.5)
                await asyncio.wait_for(writer.started.wait(), timeout=0.5)
                self.assertIs(result.handled, True)
                self.assertIs(result.is_new_activity, True)
                self.assertEqual(writer.calls, [result.activity_id])
                writer.release.set()
                await asyncio.sleep(0.05)

        asyncio.run(run_case())

    def test_activity_service_merges_time_only_update_into_existing_activity(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            first = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            service.content_parser = FakeSolitaireContentParser(start_time="23:00", participants=["弦默", "无名", "huang"])
            second = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            activities = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))

        self.assertEqual(first.activity_id, second.activity_id)
        self.assertIs(first.is_new_activity, True)
        self.assertIs(second.is_new_activity, False)
        self.assertEqual(len(activities), 1)
        activity = activities[first.activity_id]
        self.assertEqual(activity["start_time"], "2026-06-11")
        self.assertEqual(activity["participant_names"], ["弦默", "无名", "huang"])

    def test_stable_activity_body_ignores_participant_snapshot_changes(self) -> None:
        first = "#接龙\n7点篮球场打匹克球\n\n1. HL"
        second = "#接龙\n7点篮球场打匹克球\n\n1. HL\n2. 小岸姐\n3. 小乔"

        self.assertEqual(stable_activity_body(first), "7点篮球场打匹克球")
        self.assertEqual(stable_activity_body(first), stable_activity_body(second))

    def test_start_time_before_message_time_uses_current_month(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        event = normalized_event_from_parsed(parsed)
        event.timestamp = datetime(2026, 6, 15, 9, 5, tzinfo=timezone.utc)

        start_time, note = normalize_start_time_from_event("2024-04-15 14:30", event)

        self.assertEqual(start_time, "2026-06-15 14:30")
        self.assertIn("当前月份", note)

    def test_activity_service_uses_stable_body_when_llm_subject_changes(self) -> None:
        first_payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        first_raw = json.loads(first_payload["data"])
        first_raw["fromRoomId"] = 10859791146538059
        first_raw["msgData"]["title"] = "#接龙\n7点篮球场打匹克球\n\n1. HL"
        first_raw["msgUniqueIdentifier"] = "pickleball-first"
        first_payload["fromGroup"] = "10859791146538059"
        first_payload["data"] = json.dumps(first_raw, ensure_ascii=False)
        first_parsed = parse_qiwe_payload(first_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        second_payload = copy.deepcopy(first_payload)
        second_raw = json.loads(second_payload["data"])
        second_raw["msgData"]["title"] = "#接龙\n7点篮球场打匹克球\n\n1. HL\n2. 小岸姐\n3. 小乔"
        second_raw["msgUniqueIdentifier"] = "pickleball-update"
        second_payload["data"] = json.dumps(second_raw, ensure_ascii=False)
        second_parsed = parse_qiwe_payload(second_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(
                    subject="匹克球活动",
                    activity_type="运动娱乐",
                    detail="地点：篮球场；内容：打匹克球。",
                    start_time="2026-06-13 19:00",
                    participants=["HL"],
                ),
            )
            first = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(first_parsed)))
            service.content_parser = FakeSolitaireContentParser(
                subject="篮球场打匹克球",
                activity_type="运动娱乐",
                detail="地点：篮球场；活动内容：打匹克球。",
                start_time="2026-06-13 19:00",
                participants=["HL", "小岸姐", "小乔"],
            )
            second = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(second_parsed)))
            activities = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        self.assertEqual(first.activity_id, second.activity_id)
        self.assertIs(first.is_new_activity, True)
        self.assertIs(second.is_new_activity, False)
        self.assertEqual(len(activities), 1)
        activity = activities[first.activity_id]
        self.assertEqual(activity["activity_subject"], "篮球场打匹克球")
        self.assertEqual(activity["activity_identity"], "7点篮球场打匹克球")
        self.assertTrue(activity["stable_body_fingerprint"])
        self.assertEqual(activity["participant_names"], ["HL", "小岸姐", "小乔"])
        self.assertTrue(all(job["activity_id"] == first.activity_id for job in reminders.values()))

    def test_activity_service_does_not_merge_recurring_activity_across_plan_dates(self) -> None:
        first_payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        first_raw = json.loads(first_payload["data"])
        first_raw["fromRoomId"] = 10859791146538059
        first_raw["timestamp"] = 1782312327
        first_raw["msgUniqueIdentifier"] = "morning-run-day-one"
        first_raw["msgData"]["solitaireInfo"]["items"] = [
            {"range": "125-2", "timestamp": 1782312327, "userId": 7881302006036777}
        ]
        first_raw["msgData"]["title"] = (
            "#接龙\n"
            "明日早上七点社区门口集合出发\n"
            "环绕村落慢跑一圈，全程约5公里\n"
            "主打低强度有氧放松跑，不竞速、不赶进度\n\n"
            "1. 小乔"
        )
        first_payload["fromGroup"] = "10859791146538059"
        first_payload["data"] = json.dumps(first_raw, ensure_ascii=False)
        first_parsed = parse_qiwe_payload(first_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        second_payload = copy.deepcopy(first_payload)
        second_raw = json.loads(second_payload["data"])
        second_raw["timestamp"] = 1782397420
        second_raw["msgUniqueIdentifier"] = "morning-run-day-two"
        second_raw["msgData"]["solitaireInfo"]["items"] = [
            {"range": "125-2", "timestamp": 1782397420, "userId": 7881302006036777}
        ]
        second_raw["msgData"]["title"] += "\n2. Jason"
        second_payload["data"] = json.dumps(second_raw, ensure_ascii=False)
        second_parsed = parse_qiwe_payload(second_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(
                    subject="低强度村落环绕慢跑晨练",
                    activity_identity="明日早上七点社区门口集合出发",
                    activity_type="运动娱乐",
                    start_time="2026-06-25 07:00",
                    participants=["小乔"],
                ),
            )
            first = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(first_parsed)))
            service.content_parser = FakeSolitaireContentParser(
                subject="低强度村落环绕慢跑晨练",
                activity_identity="明日早上七点社区门口集合出发",
                activity_type="运动娱乐",
                start_time="2026-06-26 07:00",
                participants=["小乔", "Jason"],
            )
            second = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(second_parsed)))
            activities = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        self.assertNotEqual(first.activity_id, second.activity_id)
        self.assertIs(first.is_new_activity, True)
        self.assertIs(second.is_new_activity, True)
        self.assertEqual(len(activities), 2)
        self.assertEqual(activities[first.activity_id]["start_time"], "2026-06-25 07:00")
        self.assertEqual(activities[second.activity_id]["start_time"], "2026-06-26 07:00")
        self.assertEqual(activities[second.activity_id]["participant_names"], ["小乔", "Jason"])
        self.assertTrue(any(job["activity_id"] == first.activity_id and job["start_time"] == "2026-06-25 07:00" for job in reminders.values()))
        self.assertTrue(any(job["activity_id"] == second.activity_id and job["start_time"] == "2026-06-26 07:00" for job in reminders.values()))

    def test_activity_service_keeps_planned_time_for_same_solitaire_relative_time_update(self) -> None:
        first_payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        first_raw = json.loads(first_payload["data"])
        first_raw["fromRoomId"] = 10859791146538059
        first_raw["timestamp"] = 1782314591
        first_raw["msgUniqueIdentifier"] = "family-day-first"
        first_raw["msgData"]["solitaireInfo"]["authorId"] = 7881302006036777
        first_raw["msgData"]["solitaireInfo"]["items"] = [
            {"range": "125-2", "timestamp": 1782314591, "userId": 7881302006036777}
        ]
        first_raw["msgData"]["title"] = (
            "#接龙\n"
            "明天是社区的 family day～社区十点去秦岭脚下的石井镇大集赶集\n"
            "然后，中午一起吃火锅～ 下午陶陶居喝茶聊天\n\n"
            "1. 醒醒Wake"
        )
        first_payload["fromGroup"] = "10859791146538059"
        first_payload["data"] = json.dumps(first_raw, ensure_ascii=False)
        first_parsed = parse_qiwe_payload(first_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        second_payload = copy.deepcopy(first_payload)
        second_raw = json.loads(second_payload["data"])
        second_raw["timestamp"] = 1782361228
        second_raw["msgUniqueIdentifier"] = "family-day-update"
        second_raw["msgData"]["title"] += "\n2. Jason"
        second_raw["msgData"]["solitaireInfo"]["items"].append(
            {"range": "140-5", "timestamp": 1782361228, "userId": 7881303022978115}
        )
        second_payload["data"] = json.dumps(second_raw, ensure_ascii=False)
        second_parsed = parse_qiwe_payload(second_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(
                    subject="社区family day赶集聚餐聊天活动",
                    activity_identity="社区的 family day",
                    activity_type="社区活动",
                    start_time="2026-06-25 10:00",
                    participants=["醒醒Wake"],
                ),
            )
            first = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(first_parsed)))
            service.content_parser = FakeSolitaireContentParser(
                subject="社区family day赶集聚餐聊天活动",
                activity_identity="社区的 family day",
                activity_type="社区活动",
                start_time="2026-06-26 10:00",
                participants=["醒醒Wake", "Jason"],
            )
            second = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(second_parsed)))
            activities = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        self.assertEqual(first.activity_id, second.activity_id)
        self.assertIs(second.is_new_activity, False)
        self.assertEqual(len(activities), 1)
        activity = activities[first.activity_id]
        self.assertEqual(activity["start_time"], "2026-06-25 10:00")
        self.assertEqual(activity["participant_names"], ["醒醒Wake", "Jason"])
        self.assertTrue(all(job["start_time"] == "2026-06-25 10:00" for job in reminders.values()))

    def test_activity_service_merges_same_stable_body_when_start_time_changes(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw = json.loads(payload["data"])
        raw["fromRoomId"] = 10859791146538059
        raw["msgData"]["title"] = (
            "#接龙\n"
            "这个周一下午，来和 贺妈妈 一起穿针引线\n"
            "不只是做一件小作品，更是让自己慢下来、静下来的一小段时光。\n\n"
            "📅 时间：4/15（周一）下午 2:30\n"
            "📍 地点：秦托邦一楼大厅\n\n"
            "1. 秦托邦小客服"
        )
        raw["msgUniqueIdentifier"] = "needle-first"
        payload["fromGroup"] = "10859791146538059"
        payload["data"] = json.dumps(raw, ensure_ascii=False)
        first_parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        second_payload = copy.deepcopy(payload)
        second_raw = json.loads(second_payload["data"])
        second_raw["msgData"]["title"] += "\n2. Cici"
        second_raw["msgUniqueIdentifier"] = "needle-second"
        second_payload["data"] = json.dumps(second_raw, ensure_ascii=False)
        second_parsed = parse_qiwe_payload(second_payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(
                    subject="和贺妈妈一起穿针引线",
                    activity_identity="这个周一下午，来和 贺妈妈 一起穿针引线",
                    activity_type="手作体验",
                    start_time="",
                    participants=["秦托邦小客服"],
                ),
            )
            first = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(first_parsed)))
            service.content_parser = FakeSolitaireContentParser(
                subject="和贺妈妈一起穿针引线",
                activity_identity="这个周一下午，来和 贺妈妈 一起穿针引线",
                activity_type="手作体验",
                start_time="2024-04-15 14:30",
                participants=["秦托邦小客服", "Cici"],
            )
            second = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(second_parsed)))
            activities = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))

        self.assertEqual(first.activity_id, second.activity_id)
        self.assertIs(second.is_new_activity, False)
        self.assertEqual(len(activities), 1)
        activity = activities[first.activity_id]
        self.assertEqual(activity["start_time"], "2026-06-15 14:30")
        self.assertIn("当前月份", activity["time_normalization_note"])
        self.assertEqual(activity["participant_names"], ["秦托邦小客服", "Cici"])

    def test_activity_service_merges_new_snapshot_into_legacy_raw_summary_activity(self) -> None:
        payload = copy.deepcopy(load_fixture("group_solitaire.json"))
        raw = json.loads(payload["data"])
        raw["fromRoomId"] = 10859791146538059
        raw["msgData"]["title"] = "#接龙\n7点篮球场打匹克球\n\n1. HL\n2. 小岸姐"
        raw["msgUniqueIdentifier"] = "pickleball-legacy-update"
        payload["fromGroup"] = "10859791146538059"
        payload["data"] = json.dumps(raw, ensure_ascii=False)
        parsed = parse_qiwe_payload(payload, bot_names=["二花"], bot_user_id="1688857683805864")

        with tempfile.TemporaryDirectory() as tmp:
            state_dir = Path(tmp) / "solitaire"
            state_dir.mkdir()
            legacy_activity_id = "act_legacy_pickleball"
            (state_dir / "activities.json").write_text(
                json.dumps(
                    {
                        legacy_activity_id: {
                            "activity_id": legacy_activity_id,
                            "source_group_id": "10859791146538059",
                            "source_message_id": "legacy-message",
                            "source_sender_id": "HL",
                            "activity_subject": "匹克球活动",
                            "activity_type": "运动娱乐",
                            "activity_detail": "地点：篮球场；内容：打匹克球。",
                            "start_time": "2026-06-13 19:00",
                            "participant_names": ["HL"],
                            "participant_count": 1,
                            "promo_text": "",
                            "status": "active",
                            "raw_summary": "7点篮球场打匹克球\n\n1. HL",
                            "last_seen_at": "2026-06-13T10:47:22+00:00",
                        }
                    },
                    ensure_ascii=False,
                ),
                encoding="utf-8",
            )
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(
                    subject="篮球场打匹克球",
                    activity_type="运动娱乐",
                    detail="地点：篮球场；活动内容：打匹克球。",
                    start_time="2026-06-13 19:00",
                    participants=["HL", "小岸姐"],
                ),
            )
            result = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            activities = json.loads((Path(tmp) / "solitaire" / "activities.json").read_text(encoding="utf-8"))

        self.assertEqual(result.activity_id, legacy_activity_id)
        self.assertIs(result.is_new_activity, False)
        self.assertEqual(len(activities), 1)
        self.assertEqual(activities[legacy_activity_id]["activity_subject"], "篮球场打匹克球")
        self.assertEqual(activities[legacy_activity_id]["participant_names"], ["HL", "小岸姐"])

    def test_reminder_worker_dry_run_marks_job_once(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        async def send_func(group_id, text):
            raise AssertionError("dry-run must not send")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            worker = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=True), service, send_func)
            first = asyncio.run(worker.run_once(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)))
            second = asyncio.run(worker.run_once(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        self.assertEqual(first.scanned, 1)
        self.assertEqual(first.sent, 1)
        self.assertEqual(second.scanned, 0)
        self.assertTrue(all(job["sent"] for job in reminders.values()))

    def test_reminder_worker_live_respects_allowed_groups(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        calls = []

        async def send_func(group_id, text, source_message_ref=None):
            calls.append((group_id, text, source_message_ref))
            return SendResult(success=True, message_id="reminder-1")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            blocked = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=False, allowed_groups=["other-group"]), service, send_func)
            blocked_result = asyncio.run(blocked.run_once(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)))
            self.assertEqual(calls, [])
            allowed = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=False, allowed_groups=["10789255155259073"]), service, send_func)
            allowed_result = asyncio.run(allowed.run_once(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)))

        self.assertEqual(blocked_result.skipped, 1)
        self.assertEqual(allowed_result.sent, 1)
        self.assertEqual(calls[0][0], "10789255155259073")
        self.assertIn("活动提醒：接龙数据格式测试", calls[0][1])
        self.assertEqual(calls[0][2]["msgServerId"], "1017194")

    def test_reminder_worker_live_marks_sending_before_send_and_sent_after_success(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            reminders_path = Path(tmp) / "solitaire" / "reminders.json"

            async def send_func(group_id, text, source_message_ref=None):
                during_send = json.loads(reminders_path.read_text(encoding="utf-8"))
                job = next(iter(during_send.values()))
                self.assertEqual(job["status"], "sending")
                self.assertIs(job["sent"], False)
                self.assertEqual(job["attempt_count"], 1)
                self.assertIn("sending_at", job)
                self.assertIn("last_attempt_at", job)
                return SendResult(success=True, message_id="reminder-1")

            worker = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=False), service, send_func)
            result = asyncio.run(worker.run_once(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)))
            reminders = json.loads(reminders_path.read_text(encoding="utf-8"))

        job = next(iter(reminders.values()))
        self.assertEqual(result.sent, 1)
        self.assertEqual(result.failed, 0)
        self.assertEqual(job["status"], "sent")
        self.assertIs(job["sent"], True)
        self.assertEqual(job["send_result"]["message_id"], "reminder-1")
        self.assertIn("sent_at", job)

    def test_reminder_worker_live_failure_records_failed_without_sent(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        async def send_func(group_id, text, source_message_ref=None):
            return SendResult(success=False, error="QiWe HTTP 400", retryable=False)

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            worker = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=False), service, send_func)
            result = asyncio.run(worker.run_once(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        job = next(iter(reminders.values()))
        self.assertEqual(result.sent, 0)
        self.assertEqual(result.failed, 1)
        self.assertEqual(job["status"], "failed")
        self.assertIs(job["sent"], False)
        self.assertEqual(job["error"], "QiWe HTTP 400")
        self.assertIs(job["retryable"], False)
        self.assertEqual(job["send_result"]["success"], False)

    def test_reminder_worker_live_retryable_failure_records_pending_retry_without_sent(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        async def send_func(group_id, text, source_message_ref=None):
            return SendResult(success=False, error="QiWe HTTP 503", retryable=True)

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            worker = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=False), service, send_func)
            asyncio.run(worker.run_once(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)))
            reminders = json.loads((Path(tmp) / "solitaire" / "reminders.json").read_text(encoding="utf-8"))

        job = next(iter(reminders.values()))
        self.assertEqual(job["status"], "pending_retry")
        self.assertIs(job["sent"], False)
        self.assertIs(job["retryable"], True)

    def test_due_reminders_skips_sending_job_until_timeout(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )
        now = datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc)

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            reminders_path = Path(tmp) / "solitaire" / "reminders.json"
            reminders = json.loads(reminders_path.read_text(encoding="utf-8"))
            job = next(iter(reminders.values()))
            job["status"] = "sending"
            job["sent"] = False
            job["sending_at"] = (now - timedelta(minutes=4, seconds=59)).isoformat()
            reminders_path.write_text(json.dumps(reminders, ensure_ascii=False), encoding="utf-8")

            due_before_timeout = service.due_reminders(now)
            job["sending_at"] = (now - timedelta(minutes=5)).isoformat()
            reminders_path.write_text(json.dumps(reminders, ensure_ascii=False), encoding="utf-8")
            due_after_timeout = service.due_reminders(now)

        self.assertEqual(due_before_timeout, [])
        self.assertEqual(len(due_after_timeout), 1)
        self.assertEqual(due_after_timeout[0].status, "sending")

    def test_due_reminders_interprets_legacy_sent_flag_as_status(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            reminders_path = Path(tmp) / "solitaire" / "reminders.json"
            reminders = json.loads(reminders_path.read_text(encoding="utf-8"))
            job = next(iter(reminders.values()))
            job.pop("status", None)
            job["sent"] = True
            reminders_path.write_text(json.dumps(reminders, ensure_ascii=False), encoding="utf-8")
            sent_due = service.due_reminders(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc))

            job["sent"] = False
            reminders_path.write_text(json.dumps(reminders, ensure_ascii=False), encoding="utf-8")
            pending_due = service.due_reminders(datetime(2026, 6, 10, 15, 30, tzinfo=timezone.utc))

        self.assertEqual(sent_due, [])
        self.assertEqual(len(pending_due), 1)
        self.assertEqual(pending_due[0].status, "pending")

    def test_reminder_worker_skips_stale_jobs_after_activity_time_changes(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        async def send_func(group_id, text):
            return SendResult(success=True, message_id="unexpected")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(ActivityRepository(tmp), FeishuActivityWriter(FeishuActivityMapping()), FakeSolitaireContentParser())
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            activities_path = Path(tmp) / "solitaire" / "activities.json"
            activities = json.loads(activities_path.read_text(encoding="utf-8"))
            activity = next(iter(activities.values()))
            activity["start_time"] = "2026-06-12"
            activities_path.write_text(json.dumps(activities, ensure_ascii=False), encoding="utf-8")

            worker = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=False), service, send_func)
            result = asyncio.run(worker.run_once(datetime(2026, 6, 11, 0, 0, tzinfo=timezone.utc)))

        self.assertEqual(result.scanned, 0)

    def test_reminder_jobs_include_start_time_in_job_id(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(start_time="2026-06-25 07:00"),
            )
            first = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            reminders_path = Path(tmp) / "solitaire" / "reminders.json"
            reminders = json.loads(reminders_path.read_text(encoding="utf-8"))
            first_job_id = next(job_id for job_id, job in reminders.items() if job["activity_id"] == first.activity_id)
            reminders[first_job_id]["sent"] = True
            reminders[first_job_id]["status"] = "sent"
            reminders_path.write_text(json.dumps(reminders, ensure_ascii=False), encoding="utf-8")

            activities_path = Path(tmp) / "solitaire" / "activities.json"
            activities = json.loads(activities_path.read_text(encoding="utf-8"))
            activity = activities[first.activity_id]
            activity["start_time"] = "2026-06-26 07:00"
            activities_path.write_text(json.dumps(activities, ensure_ascii=False), encoding="utf-8")

            service.content_parser = FakeSolitaireContentParser(start_time="2026-06-26 07:00", participants=["弦默", "Jason"])
            second = asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            updated = json.loads(reminders_path.read_text(encoding="utf-8"))

        self.assertEqual(first.activity_id, second.activity_id)
        self.assertIn("20260625T0700", first_job_id)
        self.assertTrue(updated[first_job_id]["sent"])
        new_jobs = [job for job_id, job in updated.items() if job_id != first_job_id and job["activity_id"] == first.activity_id]
        self.assertEqual(len(new_jobs), 1)
        self.assertEqual(new_jobs[0]["start_time"], "2026-06-26 07:00")
        self.assertEqual(new_jobs[0]["status"], "pending")

    def test_reminder_worker_skips_jobs_after_activity_start_time(self) -> None:
        parsed = parse_qiwe_payload(
            load_fixture("group_solitaire.json"),
            bot_names=["二花"],
            bot_user_id="1688857683805864",
        )

        async def send_func(group_id, text):
            raise AssertionError("past activity reminders must not send")

        with tempfile.TemporaryDirectory() as tmp:
            service = ActivityService(
                ActivityRepository(tmp),
                FeishuActivityWriter(FeishuActivityMapping()),
                FakeSolitaireContentParser(start_time="2026-06-13 15:30"),
            )
            asyncio.run(service.upsert_from_solitaire(normalized_event_from_parsed(parsed)))
            worker = ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=False), service, send_func)
            result = asyncio.run(worker.run_once(datetime(2026, 6, 13, 8, 0, tzinfo=timezone.utc)))

        self.assertEqual(result.scanned, 0)

    def test_webhook_passive_processor_error_still_acks(self) -> None:
        class FakeWeb:
            @staticmethod
            def json_response(data, status=200):
                return type("Response", (), {"status": status, "text": json.dumps(data)})()

        class FakeRequest:
            async def read(self):
                return json.dumps(load_fixture("group_solitaire.json"), ensure_ascii=False).encode("utf-8")

        class FailingPipeline:
            @property
            def enabled(self):
                return True

            async def handle(self, event):
                raise RuntimeError("boom")

        async def run_case():
            old_web = adapter_module.web
            old_disabled = adapter_module.logger.disabled
            adapter_module.web = FakeWeb
            adapter_module.logger.disabled = True
            adapter = QiWeAdapter(
                type(
                    "Config",
                    (),
                    {
                        "extra": {
                            "pipeline_enabled": True,
                            "passive_pipeline_enabled": True,
                            "solitaire_processor_enabled": True,
                        }
                    },
                )()
            )
            adapter._passive_pipeline = FailingPipeline()
            try:
                response = await adapter._handle_webhook(FakeRequest())
                self.assertEqual(response.status, 200)
                self.assertIn('"triggered": false', response.text)
                await asyncio.sleep(0)
            finally:
                adapter_module.web = old_web
                adapter_module.logger.disabled = old_disabled

        asyncio.run(run_case())

    def test_feishu_mapping_maps_internal_fields_to_configured_headers(self) -> None:
        mapping = FeishuActivityMapping.load(str(FIXTURES / "activity_mapping.json"))
        writer = FeishuActivityWriter(mapping)
        mapped = writer.map_fields(
            {
                "activity_id": "act_1",
                "activity_subject": "接龙数据格式测试",
                "participant_count": "1",
                "participant_names": ["弦默"],
                "activity_detail": "配置里没有这个字段，所以跳过",
            }
        )

        self.assertEqual(
            mapped,
            {
                "活动ID": "act_1",
                "标题A": "接龙数据格式测试",
                "人数A": 1,
                "名单A": "弦默",
            },
        )

    def test_feishu_mapping_header_rename_is_config_only(self) -> None:
        first = FeishuActivityWriter(FeishuActivityMapping.load(str(FIXTURES / "activity_mapping.json")))
        renamed = FeishuActivityWriter(FeishuActivityMapping.load(str(FIXTURES / "activity_mapping_renamed.json")))
        fields = {"activity_id": "act_1", "activity_subject": "接龙数据格式测试", "participant_count": 1}

        self.assertIn("标题A", first.map_fields(fields))
        self.assertIn("活动名称", renamed.map_fields(fields))
        self.assertNotIn("标题A", renamed.map_fields(fields))

    def test_feishu_mapping_can_write_activity_type(self) -> None:
        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "dry_run",
                "fields": {"activity_id": "活动ID", "activity_type": "活动类型"},
            }
        )
        writer = FeishuActivityWriter(mapping)

        self.assertEqual(writer.map_fields({"activity_id": "act_1", "activity_type": "运动娱乐"}), {"活动ID": "act_1", "活动类型": "运动娱乐"})

    def test_feishu_mapping_converts_status_for_activity_occurrence_table(self) -> None:
        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "dry_run",
                "fields": {"activity_id": "活动ID", "status": "活动状态"},
            }
        )
        writer = FeishuActivityWriter(mapping)

        self.assertEqual(writer.map_fields({"activity_id": "act_1", "status": "active"}), {"活动ID": "act_1", "活动状态": "待执行"})
        self.assertEqual(writer.map_fields({"activity_id": "act_2", "status": "cancelled"}), {"活动ID": "act_2", "活动状态": "已取消"})
        self.assertEqual(writer.map_fields({"activity_id": "act_3", "status": "paused"}), {"活动ID": "act_3", "活动状态": "待修正"})

    def test_feishu_mapping_applies_activity_occurrence_default_fields(self) -> None:
        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "dry_run",
                "fields": {"activity_id": "活动ID", "status": "活动状态"},
                "defaultFields": {"活动状态": "待执行", "补录状态": "未补录"},
            }
        )
        writer = FeishuActivityWriter(mapping)

        self.assertEqual(
            writer.map_fields({"activity_id": "act_1"}),
            {"活动ID": "act_1", "活动状态": "待执行", "补录状态": "未补录"},
        )
        self.assertEqual(
            writer.map_fields({"activity_id": "act_2", "status": "cancelled"}),
            {"活动ID": "act_2", "活动状态": "已取消", "补录状态": "未补录"},
        )

    def test_feishu_datetime_fields_convert_to_unix_milliseconds(self) -> None:
        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "dry_run",
                "fields": {"activity_id": "活动ID", "start_time": "活动计划时间", "solitaire_created_at": "接龙发起时间", "last_seen_at": "最后更新时间"},
                "fieldTypes": {"start_time": "datetime", "solitaire_created_at": "datetime", "last_seen_at": "datetime"},
            }
        )
        old_env = dict(os.environ)
        try:
            os.environ["QIWE_ACTIVITY_TIMEZONE"] = "Asia/Shanghai"
            writer = FeishuActivityWriter(mapping)
            mapped = writer.map_fields(
                {
                    "activity_id": "act_1",
                    "start_time": "2026-06-12 21:30:00",
                    "solitaire_created_at": "2026-06-12T13:10:00+00:00",
                    "last_seen_at": "2026/06/12 21:20",
                }
            )

            self.assertEqual(mapped["活动计划时间"], 1781271000000)
            self.assertEqual(mapped["接龙发起时间"], 1781269800000)
            self.assertEqual(mapped["最后更新时间"], 1781270400000)
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_datetime_fields_skip_unconvertible_time_only_value(self) -> None:
        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "dry_run",
                "fields": {"activity_id": "活动ID", "start_time": "活动计划时间"},
                "fieldTypes": {"start_time": "datetime"},
            }
        )
        writer = FeishuActivityWriter(mapping)
        mapped = writer.map_fields({"activity_id": "act_1", "start_time": "23:00"})

        self.assertEqual(mapped, {"活动ID": "act_1"})

    def test_feishu_live_writer_rejects_missing_upsert_mapping_before_network(self) -> None:
        writer = FeishuActivityWriter(FeishuActivityMapping.load(str(FIXTURES / "activity_mapping_missing_upsert.json")))
        result = writer.write({"activity_id": "act_1", "activity_subject": "测试"})

        self.assertIs(result.success, False)
        self.assertEqual(result.mode, "dry_run")
        self.assertIn("upsertKey", result.error)

    def test_feishu_dry_run_does_not_require_api_credentials(self) -> None:
        writer = FeishuActivityWriter(FeishuActivityMapping.load(str(FIXTURES / "activity_mapping.json")))
        result = writer.write({"activity_id": "act_1", "activity_subject": "测试"})

        self.assertIs(result.success, True)
        self.assertEqual(result.mode, "dry_run")
        self.assertEqual(result.mapped_fields["活动ID"], "act_1")

    def test_feishu_live_writer_requires_explicit_write_enable(self) -> None:
        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "live",
                "fields": {"activity_id": "活动ID"},
                "fieldTypes": {},
            }
        )
        old_env = dict(os.environ)
        try:
            os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE", None)
            writer = FeishuActivityWriter(mapping)
            result = writer.write({"activity_id": "act_1", "source_group_id": "10789255155259073"})

            self.assertIs(result.success, False)
            self.assertIn("QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE", result.error)
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_live_writer_respects_allowed_source_groups_before_network(self) -> None:
        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "live",
                "fields": {"activity_id": "活动ID"},
            }
        )
        old_env = dict(os.environ)
        try:
            os.environ["QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE"] = "true"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_ALLOWED_GROUPS"] = "allowed-group"
            writer = FeishuActivityWriter(mapping)
            result = writer.write({"activity_id": "act_1", "source_group_id": "blocked-group"})

            self.assertIs(result.success, False)
            self.assertIn("not allowed", result.error)
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_field_probe_requires_scoped_table_config(self) -> None:
        old_env = dict(os.environ)
        try:
            os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN", None)
            os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_TABLE_ID", None)
            writer = FeishuActivityWriter(FeishuActivityMapping())
            result = writer.probe_fields()

            self.assertIs(result.success, False)
            self.assertIn("QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN", result.error)
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_field_probe_reads_only_configured_table_fields(self) -> None:
        class FakeProbeWriter(FeishuActivityWriter):
            def __init__(self, mapping):
                super().__init__(mapping)
                self.requests = []

            def _tenant_access_token(self):
                return "tenant-token"

            def _request(self, path, token, payload, *, method, authorized=True, body=True):
                self.requests.append((path, token, payload, method, authorized, body))
                return {
                    "code": 0,
                    "data": {
                        "items": [
                            {"field_id": "fld_a", "field_name": "活动主题", "type": 1, "is_primary": True},
                            {"field_id": "fld_b", "field_name": "参与人数", "type": 2},
                        ]
                    },
                }

        old_env = dict(os.environ)
        try:
            os.environ["QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN"] = "app_token_test"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_TABLE_ID"] = "tbl_test"
            writer = FakeProbeWriter(FeishuActivityMapping())
            result = writer.probe_fields()

            self.assertIs(result.success, True)
            self.assertEqual(result.app_token, "app_token_test")
            self.assertEqual(result.table_id, "tbl_test")
            self.assertEqual([field.field_name for field in result.fields], ["活动主题", "参与人数"])
            self.assertEqual(
                writer.requests[0],
                ("/bitable/v1/apps/app_token_test/tables/tbl_test/fields?page_size=200", "tenant-token", {}, "GET", True, False),
            )
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_live_writer_updates_existing_record_by_upsert_key(self) -> None:
        class FakeUpsertWriter(FeishuActivityWriter):
            def __init__(self, mapping):
                super().__init__(mapping)
                self.requests = []

            def _tenant_access_token(self):
                return "tenant-token"

            def _request(self, path, token, payload, *, method, authorized=True, body=True):
                self.requests.append((path, token, payload, method))
                if path.endswith("/records/search"):
                    return {"code": 0, "data": {"items": [{"record_id": "rec_existing"}]}}
                return {"code": 0, "data": {"record": {"record_id": "rec_existing"}}}

        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "live",
                "fields": {"activity_id": "活动ID", "activity_subject": "活动内容"},
            }
        )
        old_env = dict(os.environ)
        try:
            os.environ["QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE"] = "true"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN"] = "app_token_test"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_TABLE_ID"] = "tbl_test"
            writer = FakeUpsertWriter(mapping)
            result = writer.write({"activity_id": "act_1", "activity_subject": "测试", "source_group_id": "10789255155259073"})

            self.assertIs(result.success, True)
            self.assertEqual(result.record_id, "rec_existing")
            self.assertEqual(writer.requests[0][0], "/bitable/v1/apps/app_token_test/tables/tbl_test/records/search")
            self.assertEqual(writer.requests[0][3], "POST")
            self.assertEqual(writer.requests[1][0], "/bitable/v1/apps/app_token_test/tables/tbl_test/records/rec_existing")
            self.assertEqual(writer.requests[1][3], "PUT")
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_live_writer_creates_when_upsert_key_not_found(self) -> None:
        class FakeUpsertWriter(FeishuActivityWriter):
            def __init__(self, mapping):
                super().__init__(mapping)
                self.requests = []

            def _tenant_access_token(self):
                return "tenant-token"

            def _request(self, path, token, payload, *, method, authorized=True, body=True):
                self.requests.append((path, token, payload, method))
                if path.endswith("/records/search"):
                    return {"code": 0, "data": {"items": []}}
                return {"code": 0, "data": {"record": {"record_id": "rec_created"}}}

        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "live",
                "fields": {"activity_id": "活动ID", "activity_subject": "活动内容"},
            }
        )
        old_env = dict(os.environ)
        try:
            os.environ["QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE"] = "true"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN"] = "app_token_test"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_TABLE_ID"] = "tbl_test"
            writer = FakeUpsertWriter(mapping)
            result = writer.write({"activity_id": "act_1", "activity_subject": "测试", "source_group_id": "10789255155259073"})

            self.assertIs(result.success, True)
            self.assertEqual(result.record_id, "rec_created")
            self.assertEqual(writer.requests[1][0], "/bitable/v1/apps/app_token_test/tables/tbl_test/records")
            self.assertEqual(writer.requests[1][3], "POST")
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_live_writer_rejects_duplicate_upsert_matches(self) -> None:
        class FakeUpsertWriter(FeishuActivityWriter):
            def _tenant_access_token(self):
                return "tenant-token"

            def _request(self, path, token, payload, *, method, authorized=True, body=True):
                return {"code": 0, "data": {"items": [{"record_id": "rec_1"}, {"record_id": "rec_2"}]}}

        mapping = FeishuActivityMapping.from_dict(
            {
                "enabled": True,
                "mode": "live",
                "fields": {"activity_id": "活动ID", "activity_subject": "活动内容"},
            }
        )
        old_env = dict(os.environ)
        try:
            os.environ["QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE"] = "true"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN"] = "app_token_test"
            os.environ["QINTOPIA_FEISHU_ACTIVITY_TABLE_ID"] = "tbl_test"
            writer = FakeUpsertWriter(mapping)
            result = writer.write({"activity_id": "act_1", "activity_subject": "测试", "source_group_id": "10789255155259073"})

            self.assertIs(result.success, False)
            self.assertIs(result.retryable, False)
            self.assertIn("multiple Feishu records", result.error)
        finally:
            os.environ.clear()
            os.environ.update(old_env)

    def test_feishu_writer_can_opt_in_to_hermes_config_credentials(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            config_path = Path(tmp) / "config.yaml"
            config_path.write_text(
                "feishu:\n"
                "  enabled: false\n"
                "  app_id: cli_test\n"
                "  app_secret: secret_test\n",
                encoding="utf-8",
            )
            old_env = dict(os.environ)
            try:
                os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_APP_ID", None)
                os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_APP_SECRET", None)
                os.environ["QINTOPIA_FEISHU_ACTIVITY_USE_HERMES_CONFIG"] = "true"
                os.environ["QINTOPIA_FEISHU_ACTIVITY_HERMES_CONFIG"] = str(config_path)
                writer = FeishuActivityWriter(FeishuActivityMapping())

                self.assertEqual(writer._activity_app_credentials(), ("cli_test", "secret_test"))
            finally:
                os.environ.clear()
                os.environ.update(old_env)

    def test_feishu_writer_does_not_read_hermes_config_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            config_path = Path(tmp) / "config.yaml"
            config_path.write_text(
                "feishu:\n"
                "  app_id: cli_test\n"
                "  app_secret: secret_test\n",
                encoding="utf-8",
            )
            old_env = dict(os.environ)
            try:
                os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_APP_ID", None)
                os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_APP_SECRET", None)
                os.environ.pop("QINTOPIA_FEISHU_ACTIVITY_USE_HERMES_CONFIG", None)
                os.environ["QINTOPIA_FEISHU_ACTIVITY_HERMES_CONFIG"] = str(config_path)
                writer = FeishuActivityWriter(FeishuActivityMapping())

                self.assertEqual(writer._activity_app_credentials(), ("", ""))
            finally:
                os.environ.clear()
                os.environ.update(old_env)


if __name__ == "__main__":
    unittest.main()
