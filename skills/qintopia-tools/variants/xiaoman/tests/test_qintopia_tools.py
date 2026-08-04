from __future__ import annotations

import base64
import importlib.util
import hashlib
import hmac
import json
import logging
import os
import socket
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace


def load_plugin():
    plugin_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("qintopia_tools_plugin", plugin_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_fixture(index_dir: Path) -> None:
    index_dir.mkdir(parents=True, exist_ok=True)
    body = """# 秦托邦 GIS 位置信息

| 名称 | 经度 | 纬度 | 门头图片 |
|------|------|------|----------|
| 秦托邦社区 | 108.572935 | 34.023305 | ![社区](https://example.test/community.jpg) |
| 秦托邦1栋 | 108.572849 | 34.024317 | ![1栋](https://example.test/1.jpg) |
| 秦托邦2栋 | 108.572225 | 34.023833 | ![2栋](https://example.test/2.jpg) |
"""
    record = {
        "source_id": "gis123",
        "title": "秦托邦 GIS 位置信息",
        "path": "gis-locations.md",
        "information_class": "Public",
        "updated_at": "2026-06-01T14:41:20+00:00",
        "body": body,
    }
    product_record = {
        "source_id": "product123",
        "title": "公开 Agent OS 产品介绍 FAQ",
        "path": "agent-os-public-faq.md",
        "information_class": "Public",
        "updated_at": "2026-06-07T00:00:00+00:00",
        "body": (
            "Qintopia Agent OS 是面向组织协作场景的 Agent 工作系统。"
            "系统可以支持需求收集、知识检索、方案草拟、演示准备、任务流转和人工审批。"
        ),
    }
    (index_dir / "public.jsonl").write_text(
        json.dumps(record, ensure_ascii=False) + "\n" + json.dumps(product_record, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    (index_dir / "internal.jsonl").write_text("", encoding="utf-8")
    (index_dir / "member-scoped.jsonl").write_text("", encoding="utf-8")


class QintopiaToolsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmpdir = tempfile.TemporaryDirectory()
        self.index_dir = Path(self.tmpdir.name)
        write_fixture(self.index_dir)
        self.old_index = os.environ.get("QINTOPIA_KB_INDEX_DIR")
        os.environ["QINTOPIA_KB_INDEX_DIR"] = str(self.index_dir)
        self.old_dify_env = {
            name: os.environ.get(name)
            for name in [
                "QINTOPIA_DIFY_KB_BASE_URL",
                "QINTOPIA_DIFY_KB_API_KEY",
                "QINTOPIA_DIFY_ALLOWED_DATASET_IDS",
                "QINTOPIA_DIFY_LOOKUP_DATASET_ID",
                "QINTOPIA_PROFILE_ID",
                "QINTOPIA_DIFY_RAW_TOOLS_ENABLE",
                "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
                "QINTOPIA_SIDECAR_BIN",
                "QINTOPIA_XIAOMAN_ACTIVITY_FIXTURE_PATH",
                "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
                "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
                "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_TIMEOUT_SECONDS",
                "QINTOPIA_OPERATIONS_INTAKE_SOCKET",
                "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE",
                "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY",
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
                "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
                "QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID",
                "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED",
                "HERMES_SESSION_PLATFORM",
                "HERMES_SESSION_CONVERSATION_TYPE",
                "HERMES_SESSION_CHAT_ID",
                "HERMES_SESSION_USER_ID",
                "HERMES_SESSION_MESSAGE_ID",
            ]
        }
        os.environ["QINTOPIA_DIFY_KB_BASE_URL"] = "http://dify.example.test/v1"
        os.environ["QINTOPIA_DIFY_KB_API_KEY"] = "test-knowledge-key"
        os.environ["QINTOPIA_DIFY_ALLOWED_DATASET_IDS"] = "ds_allowed"
        os.environ.pop("QINTOPIA_DIFY_LOOKUP_DATASET_ID", None)
        os.environ.pop("QINTOPIA_PROFILE_ID", None)
        os.environ.pop("QINTOPIA_DIFY_RAW_TOOLS_ENABLE", None)
        os.environ.pop("QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE", None)
        os.environ.pop("QINTOPIA_SIDECAR_BIN", None)
        os.environ.pop("QINTOPIA_XIAOMAN_ACTIVITY_FIXTURE_PATH", None)
        os.environ.pop("QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE", None)
        os.environ.pop("QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE", None)
        os.environ.pop("QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_TIMEOUT_SECONDS", None)
        os.environ.pop("QINTOPIA_OPERATIONS_INTAKE_SOCKET", None)
        os.environ.pop("QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE", None)
        os.environ.pop("QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY", None)
        os.environ.pop("QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE", None)
        os.environ.pop("QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY", None)
        os.environ.pop("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID", None)
        os.environ.pop("QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED", None)
        os.environ.pop("HERMES_SESSION_PLATFORM", None)
        os.environ.pop("HERMES_SESSION_CONVERSATION_TYPE", None)
        os.environ.pop("HERMES_SESSION_CHAT_ID", None)
        os.environ.pop("HERMES_SESSION_USER_ID", None)
        os.environ.pop("HERMES_SESSION_MESSAGE_ID", None)
        self.module = load_plugin()

    def tearDown(self) -> None:
        if self.old_index is None:
            os.environ.pop("QINTOPIA_KB_INDEX_DIR", None)
        else:
            os.environ["QINTOPIA_KB_INDEX_DIR"] = self.old_index
        for name, value in self.old_dify_env.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        self.tmpdir.cleanup()

    def enable_xiaoman_activity_wrappers(self) -> None:
        os.environ["QINTOPIA_PROFILE_ID"] = "xiaoman"
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"] = "1"
        os.environ["QINTOPIA_SIDECAR_BIN"] = "/tmp/qintopia-message-sidecar"
        self.module._xiaoman_activity_validate_read_through_worker = lambda worker_bin: Path(worker_bin)

    def trust_poster_session(
        self,
        *,
        chat_id: str = "oc_chat_fixture",
        user_id: str = "ou_user_fixture",
        message_id: str = "om_message_fixture",
        conversation_type: str = "direct",
        ingress_enabled: bool = True,
    ) -> None:
        values = {
            "HERMES_SESSION_PLATFORM": "feishu",
            "HERMES_SESSION_CONVERSATION_TYPE": conversation_type,
            "HERMES_SESSION_CHAT_ID": chat_id,
            "HERMES_SESSION_USER_ID": user_id,
            "HERMES_SESSION_MESSAGE_ID": message_id,
        }
        self.module._strict_poster_session_value = lambda name: values.get(name, "")
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = (
            "1" if ingress_enabled else "0"
        )

    def feishu_message_event(
        self,
        *,
        chat_id: str = "oc_chat_fixture",
        chat_type: str = "p2p",
        sender_id: str = "ou_user_fixture",
        sender_type: str = "user",
        message_id: str = "om_message_fixture",
        text: str = "请生成海报，活动时间 8 月 1 日 16:00，地点线上",
        mention_bot: bool = False,
        source_chat_id: str | None = None,
        source_chat_type: str | None = None,
    ):
        bot_open_id = "ou_xiaoman_bot_fixture"
        mentions = (
            [SimpleNamespace(id=SimpleNamespace(open_id=bot_open_id))]
            if mention_bot
            else []
        )
        return SimpleNamespace(
            text=text,
            message_type=SimpleNamespace(value="text"),
            source=SimpleNamespace(
                platform=SimpleNamespace(value="feishu"),
                chat_id=source_chat_id or chat_id,
                user_id=sender_id,
                chat_type=source_chat_type
                or ("group" if chat_type == "group" else "dm"),
            ),
            raw_message=SimpleNamespace(
                header=SimpleNamespace(event_id=f"evt_{message_id}"),
                event=SimpleNamespace(
                    sender=SimpleNamespace(
                        sender_id=SimpleNamespace(open_id=sender_id),
                        sender_type=sender_type,
                    ),
                    message=SimpleNamespace(
                        message_id=message_id,
                        chat_id=chat_id,
                        chat_type=chat_type,
                        message_type="text",
                        content=json.dumps({"text": text}, ensure_ascii=False),
                        mentions=mentions,
                        root_id="",
                        parent_id="",
                        create_time="1785571200000",
                    ),
                ),
            ),
            message_id=message_id,
        )

    def poster_review_event(
        self,
        *,
        source_chat_id: str = "oc_chat_fixture",
        callback_chat_id: str = "oc_chat_fixture",
        source_user_id: str = "ou_user_fixture",
        operator_open_id: str = "ou_user_fixture",
        chat_type: str = "group",
        callback_kind: str = "xiaoman_poster_review",
    ):
        token = "card_token_fixture"
        return SimpleNamespace(
            text="/card button",
            message_type=SimpleNamespace(value="command"),
            source=SimpleNamespace(
                platform=SimpleNamespace(value="feishu"),
                chat_id=source_chat_id,
                user_id=source_user_id,
                chat_type=chat_type,
            ),
            raw_message=SimpleNamespace(
                header=SimpleNamespace(event_id="evt_fixture_1"),
                event=SimpleNamespace(
                    token=token,
                    context=SimpleNamespace(open_chat_id=callback_chat_id),
                    operator=SimpleNamespace(open_id=operator_open_id),
                    action=SimpleNamespace(
                        tag="button",
                        value={
                            "schema_version": 1,
                            "callback_kind": callback_kind,
                            "notification_id": "11111111-1111-4111-8111-111111111111",
                            "artifact_id": "22222222-2222-4222-8222-222222222222",
                            "action": "approve",
                        },
                    ),
                ),
            ),
            message_id=token,
        )

    def write_fake_xiaoman_sidecar(self, report: dict) -> Path:
        path = self.index_dir / "fake-xiaoman-sidecar.py"
        path.write_text(
            "#!/usr/bin/env python3\n"
            "import json\n"
            "import sys\n"
            f"report = json.loads({json.dumps(report, ensure_ascii=False)!r})\n"
            "payload = json.loads(sys.argv[sys.argv.index('--payload-json') + 1])\n"
            "report['operation'] = sys.argv[2]\n"
            "report['actor_agent'] = payload['actor_agent']\n"
            "report['dry_run'] = '--dry-run' in sys.argv\n"
            "print(json.dumps(report, ensure_ascii=False))\n",
            encoding="utf-8",
        )
        path.chmod(0o700)
        return path

    def write_raw_xiaoman_sidecar(self, body: str, *, stderr: str = "", exit_code: int = 0) -> Path:
        path = self.index_dir / "raw-xiaoman-sidecar.py"
        path.write_text(
            "#!/usr/bin/env python3\n"
            "import sys\n"
            f"sys.stdout.write({body!r})\n"
            f"sys.stderr.write({stderr!r})\n"
            f"sys.exit({exit_code})\n",
            encoding="utf-8",
        )
        path.chmod(0o700)
        return path

    def write_env_echo_xiaoman_sidecar(self) -> Path:
        path = self.index_dir / "env-echo-xiaoman-sidecar.py"
        path.write_text(
            "#!/usr/bin/env python3\n"
            "import json\n"
            "import os\n"
            "report = {\n"
            "  'success': True,\n"
            "  'worker': 'xiaoman-activity',\n"
            "  'source': 'fixture',\n"
            "  'record_count': 1,\n"
            "  'records': [{\n"
            "    'table_role': 'activity_occurrence',\n"
            "    'record_ref': 'activity_occurrence:abc123def456',\n"
            "    'title': os.environ.get('SECRET_TOKEN', 'secret-not-inherited'),\n"
            "    'activity_date': '2026-07-16',\n"
            "    'location': '秦托邦共享厨房',\n"
            "    'status': '待宣传',\n"
            "  }],\n"
            "  'summaries': [os.environ.get('SECRET_TOKEN', 'secret-not-inherited')],\n"
            "}\n"
            "print(json.dumps(report, ensure_ascii=False))\n",
            encoding="utf-8",
        )
        path.chmod(0o700)
        return path

    def test_xiaoman_activity_read_through_rejects_non_release_worker(self):
        fake_sidecar = self.write_raw_xiaoman_sidecar("{}")

        with self.assertRaises(PermissionError):
            self.module._xiaoman_activity_validate_read_through_worker(str(fake_sidecar))

    def test_xiaoman_activity_read_through_rejects_raw_symlink_worker_path(self):
        release_root = self.index_dir / "releases"
        release_dir = release_root / ("a" * 40) / "sidecar"
        release_dir.mkdir(parents=True)
        trusted_sidecar = release_dir / "trusted-sidecar"
        trusted_sidecar.write_text("#!/bin/sh\nprintf '{}'\n", encoding="utf-8")
        trusted_sidecar.chmod(0o700)
        symlink_sidecar = release_dir / "qintopia-message-sidecar"
        symlink_sidecar.symlink_to(trusted_sidecar)
        self.module.XIAOMAN_ACTIVITY_READ_THROUGH_RELEASE_ROOT = release_root

        with self.assertRaises(PermissionError):
            self.module._xiaoman_activity_validate_read_through_worker(str(symlink_sidecar))

    def test_xiaoman_activity_read_through_rejects_owner_writable_worker_path(self):
        release_root = self.index_dir / "immutable-releases"
        release_dir = release_root / ("b" * 40) / "sidecar"
        release_dir.mkdir(parents=True)
        worker = release_dir / "qintopia-message-sidecar"
        worker.write_text("#!/bin/sh\nprintf '{}'\n", encoding="utf-8")
        release_root.chmod(0o555)
        release_dir.parent.chmod(0o555)
        release_dir.chmod(0o555)
        worker.chmod(0o700)
        self.module.XIAOMAN_ACTIVITY_READ_THROUGH_RELEASE_ROOT = release_root

        try:
            with self.assertRaises(PermissionError):
                self.module._xiaoman_activity_validate_read_through_worker(str(worker))
        finally:
            worker.chmod(0o700)
            release_dir.chmod(0o700)
            release_dir.parent.chmod(0o700)
            release_root.chmod(0o700)

    def test_xiaoman_activity_read_through_executes_validated_worker_path(self):
        self.enable_xiaoman_activity_wrappers()
        trusted_sidecar = self.write_fake_xiaoman_sidecar(
            {
                "success": True,
                "worker": "xiaoman-activity",
                "source": "trusted",
                "record_count": 1,
                "records": [
                    {
                        "table_role": "activity_occurrence",
                        "record_ref": "activity_occurrence:abc123def456",
                        "title": "trusted worker",
                        "activity_date": "2026-07-16",
                    }
                ],
            }
        )
        raw_sidecar = self.write_raw_xiaoman_sidecar(
            json.dumps(
                {
                    "success": True,
                    "records": [
                        {
                            "table_role": "activity_occurrence",
                            "record_ref": "activity_occurrence:abc123def456",
                            "title": "raw worker should not run",
                        }
                    ],
                }
            )
        )
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(raw_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"
        self.module._xiaoman_activity_validate_read_through_worker = lambda _worker_bin: trusted_sidecar

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertEqual(report["records"][0]["title"], "trusted worker")

    def test_gis_lookup_1_building(self):
        payload = json.loads(self.module.handle_qintopia_gis_location_lookup({"query": "1 栋"}))

        self.assertTrue(payload["success"])
        self.assertTrue(payload["matched"])
        self.assertEqual(payload["location"]["name"], "秦托邦1栋")
        self.assertEqual(payload["location"]["longitude"], 108.572849)
        self.assertEqual(payload["location"]["latitude"], 34.024317)
        self.assertTrue(payload["location"]["amap_url"].startswith("https://uri.amap.com/marker?"))
        self.assertEqual(payload["scope_used"], ["Public"])

    def test_kb_search_defaults_public_only(self):
        payload = json.loads(self.module.handle_qintopia_kb_search({"query": "秦托邦1栋"}))

        self.assertTrue(payload["success"])
        self.assertEqual(payload["scope_used"], ["Public"])
        self.assertIn("Member-scoped", payload["not_accessed"])
        self.assertEqual(payload["results"][0]["path"], "gis-locations.md")

    def test_xiaoman_status_update_matches_event_signal_worker_contract(self):
        self.enable_xiaoman_activity_wrappers()
        schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_STATUS_UPDATE_SCHEMA["parameters"]

        self.assertEqual(schema["required"], ["event_signal_id", "mutation_id", "status"])
        self.assertNotIn("record_id", schema["properties"])
        self.assertNotIn("table_role", schema["properties"])

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_status_update(
                {
                    "event_signal_id": "66666666-6666-4666-8666-666666666666",
                    "mutation_id": "77777777-7777-4777-8777-777777777777",
                    "status": "处理中",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertTrue(report["dry_run"])
        self.assertEqual(report["action"]["command"][-1], "--dry-run")
        command = report["action"]["command"]
        worker_payload = json.loads(command[command.index("--payload-json") + 1])
        self.assertEqual(worker_payload, report["payload"])
        self.assertEqual(
            set(worker_payload),
            {
                "event_signal_id",
                "mutation_id",
                "status",
                "actor_agent",
                "operation",
                "dry_run",
            },
        )

    def test_xiaoman_gap_update_matches_event_signal_worker_contract(self):
        self.enable_xiaoman_activity_wrappers()
        schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_GAP_UPDATE_SCHEMA["parameters"]

        self.assertEqual(schema["required"], ["event_signal_id", "mutation_id", "gap_summary"])
        self.assertNotIn("record_id", schema["properties"])
        self.assertNotIn("table_role", schema["properties"])

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_gap_update(
                {
                    "event_signal_id": "66666666-6666-4666-8666-666666666666",
                    "mutation_id": "88888888-8888-4888-8888-888888888888",
                    "gap_summary": "缺少报名截止时间",
                    "dry_run": False,
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertFalse(report["dry_run"])
        self.assertEqual(report["action"]["command"][-1], "--apply")
        command = report["action"]["command"]
        worker_payload = json.loads(command[command.index("--payload-json") + 1])
        self.assertEqual(worker_payload, report["payload"])
        self.assertEqual(
            set(worker_payload),
            {
                "event_signal_id",
                "mutation_id",
                "gap_summary",
                "actor_agent",
                "operation",
                "dry_run",
            },
        )

    def test_xiaoman_phase_update_matches_event_signal_worker_contract(self):
        self.enable_xiaoman_activity_wrappers()
        schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_PHASE_UPDATE_SCHEMA["parameters"]

        self.assertEqual(schema["required"], ["event_signal_id", "mutation_id", "activity_phase"])
        self.assertNotIn("record_id", schema["properties"])
        self.assertNotIn("table_role", schema["properties"])
        self.assertEqual(schema["properties"]["activity_phase"]["enum"], ["pre_event", "in_event", "post_event"])

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_phase_update(
                {
                    "event_signal_id": "66666666-6666-4666-8666-666666666666",
                    "mutation_id": "99999999-9999-4999-8999-999999999999",
                    "activity_phase": "in_event",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertTrue(report["dry_run"])
        self.assertEqual(report["operation"], "phase-update")
        self.assertEqual(report["action"]["command"][-1], "--dry-run")
        command = report["action"]["command"]
        worker_payload = json.loads(command[command.index("--payload-json") + 1])
        self.assertEqual(worker_payload, report["payload"])
        self.assertEqual(
            set(worker_payload),
            {
                "event_signal_id",
                "mutation_id",
                "activity_phase",
                "actor_agent",
                "operation",
                "dry_run",
            },
        )

    def test_xiaoman_mutations_reject_missing_ids_and_overlong_gap(self):
        self.enable_xiaoman_activity_wrappers()

        missing_mutation = json.loads(
            self.module.handle_qintopia_xiaoman_activity_status_update(
                {
                    "event_signal_id": "66666666-6666-4666-8666-666666666666",
                    "status": "处理中",
                }
            )
        )
        overlong_gap = json.loads(
            self.module.handle_qintopia_xiaoman_activity_gap_update(
                {
                    "event_signal_id": "66666666-6666-4666-8666-666666666666",
                    "mutation_id": "88888888-8888-4888-8888-888888888888",
                    "gap_summary": "长" * 501,
                }
            )
        )

        self.assertFalse(missing_mutation["success"])
        self.assertEqual(missing_mutation["error"], "mutation_id is required")
        self.assertFalse(overlong_gap["success"])
        self.assertEqual(overlong_gap["error"], "gap_summary must be 500 characters or fewer")

    def test_xiaoman_phase_update_rejects_unknown_phase(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_phase_update(
                {
                    "event_signal_id": "66666666-6666-4666-8666-666666666666",
                    "mutation_id": "99999999-9999-4999-8999-999999999999",
                    "activity_phase": "during",
                }
            )
        )

        self.assertFalse(report["success"])
        self.assertEqual(report["error"], "activity_phase is not allowed")

    def test_xiaoman_activity_list_by_date_defaults_to_worker_command(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertFalse(report["dry_run"])
        self.assertEqual(report["action"]["tool"], "agentos_worker_command")
        self.assertTrue(report["action"]["requires_local_execution"])
        self.assertNotIn("records", report)
        self.assertEqual(report["action"]["command"][-1], "--apply")

    def test_xiaoman_activity_announcement_prepare_text_only_mvp(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    "date": "2026-07-21",
                    "operator_name": "刘珊",
                    "records": [
                        {
                            "table_role": "activity_plan",
                            "record_ref": "activity_plan:abc123def456",
                            "title": "付费木作体验课",
                            "activity_date": "2026-07-21",
                            "start_time": "15:00",
                            "end_time": "17:00",
                            "location": "秦托邦工坊",
                            "owner_name": "阿成",
                            "promotion_status": "待确认",
                        },
                        {
                            "table_role": "activity_plan",
                            "record_ref": "activity_plan:def456abc123",
                            "title": "临时约饭",
                            "activity_date": "2026-07-21",
                            "start_time": "12:30",
                            "location": "共享厨房",
                            "owner_name": "刘珊",
                        },
                        {
                            "table_role": "activity_plan",
                            "record_ref": "activity_plan:111111aaaaaa",
                            "title": "晚间共创会",
                            "activity_date": "2026-07-21",
                        },
                    ],
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertTrue(report["success"])
        self.assertEqual(report["publishable_count"], 2)
        self.assertEqual(report["skipped_count"], 1)
        self.assertIn("付费木作体验课", report["announcement_text"])
        self.assertIn("晚间共创会", report["announcement_text"])
        self.assertNotIn("临时约饭", report["announcement_text"])
        self.assertIn("地点", report["missing_followups"][0]["missing_fields"])
        self.assertIn("负责人", report["missing_followups"][0]["missing_fields"])
        self.assertTrue(report["requires_human_confirmation"])
        self.assertFalse(report["external_send_executed"])
        self.assertFalse(report["safe_for_member_chat"])
        self.assertNotIn("record_ref", rendered)

    def test_xiaoman_activity_announcement_prepare_post_event_material_followup_stages(self):
        self.enable_xiaoman_activity_wrappers()

        base_args = {
            "date": "2026-07-21",
            "mode": "post_event_followup",
            "operator_name": "刘珊",
            "operations_lead_name": "小满运营负责人",
            "records": [
                {
                    "table_role": "activity_occurrence",
                    "record_ref": "activity_occurrence:abc123def456",
                    "title": "木作体验课",
                    "activity_date": "2026-07-20",
                    "end_time": "17:00",
                    "location": "秦托邦工坊",
                    "owner_name": "阿成",
                }
            ],
        }

        first = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    **base_args,
                    "post_event_elapsed_hours": 24,
                }
            )
        )
        second = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    **base_args,
                    "post_event_elapsed_hours": 48,
                }
            )
        )
        third = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    **base_args,
                    "post_event_elapsed_hours": 72,
                }
            )
        )

        self.assertEqual(first["post_event_followup_stage"], "24h_first_reminder")
        self.assertEqual(first["material_followup_reminders"][0]["stage"], 1)
        self.assertFalse(first["material_followup_reminders"][0]["work_omission_candidate"])
        self.assertIn("第 1 次", first["material_followup_reminders"][0]["reminder_text"])
        self.assertEqual(second["post_event_followup_stage"], "48h_second_reminder")
        self.assertIn("第 2 次", second["material_followup_reminders"][0]["reminder_text"])
        self.assertEqual(third["post_event_followup_stage"], "72h_third_miss")
        self.assertTrue(third["material_followup_reminders"][0]["work_omission_candidate"])
        self.assertIn("工作遗漏", third["material_escalations"][0]["escalation_text"])
        self.assertIn("小满运营负责人", third["material_escalations"][0]["escalation_text"])
        self.assertIn("运营升级草稿", third["operator_review_message"])
        self.assertFalse(third["external_send_executed"])
        self.assertFalse(third["safe_for_member_chat"])
        self.assertNotIn("record_ref", json.dumps(third, ensure_ascii=False))

    def test_xiaoman_activity_text_group_message_request_prepare_requires_approved_artifact(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_text_group_message_request_prepare(
                {
                    "source_record_id": "activity_plan:abc123def456",
                    "approved_artifact_id": "44444444-4444-4444-8444-444444444444",
                    "message_text": "明天下午 3 点木作体验课在秦托邦工坊集合。",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertTrue(report["dry_run"])
        self.assertTrue(report["requires_approved_artifact"])
        self.assertTrue(report["requires_human_final_confirmation"])
        self.assertFalse(report["external_send_executed"])
        self.assertEqual(report["action"]["command"][1], "operations-create")
        self.assertEqual(report["action"]["command"][-1], "--dry-run")
        payload = json.loads(report["action"]["command"][3])
        expected_hash = "sha256:" + hashlib.sha256(
            "明天下午 3 点木作体验课在秦托邦工坊集合。".encode("utf-8")
        ).hexdigest()
        self.assertEqual(payload["requester_agent"], "xiaoman")
        self.assertEqual(payload["target_agent"], "erhua")
        self.assertEqual(payload["capability_key"], "erhua.send_group_message")
        self.assertEqual(payload["work_item_type"], "group_message_request")
        self.assertEqual(
            payload["approved_artifact_id"],
            "44444444-4444-4444-8444-444444444444",
        )
        self.assertEqual(payload["payload"]["approved_artifact_type"], "text_announcement")
        self.assertEqual(payload["payload"]["approved_artifact_content_hash"], expected_hash)
        self.assertEqual(payload["payload"]["target_channel"], "qiwe")
        self.assertEqual(payload["payload"]["target_group_alias"], "community_activity_group")
        self.assertFalse(payload["payload"]["send_executed"])

    def test_xiaoman_activity_text_group_message_request_prepare_rejects_missing_artifact(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_text_group_message_request_prepare(
                {
                    "source_record_id": "activity_plan:abc123def456",
                    "message_text": "明天下午 3 点木作体验课在秦托邦工坊集合。",
                }
            )
        )

        self.assertFalse(report["success"])
        self.assertEqual(report["error"], "approved_artifact_id must be a uuid")

    def test_xiaoman_activity_text_group_message_request_prepare_rejects_sensitive_text(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_text_group_message_request_prepare(
                {
                    "source_record_id": "activity_plan:abc123def456",
                    "approved_artifact_id": "44444444-4444-4444-8444-444444444444",
                    "message_text": "请查看 https://example.test/raw-record",
                }
            )
        )

        self.assertFalse(report["success"])
        self.assertEqual(
            report["error"],
            "message_text contains disallowed sensitive or raw internal content",
        )
        self.assertTrue(report["human_rewrite_required"])
        self.assertIn("用人话重述", report["user_visible_message"])

        for message_text in [
            "请看表格 tblIRfkR6vmwiynbu 的配置。",
            "按 view vew5C6T2FF 发送。",
            "record_id=recABCDEFG123456789 obj_token=secretish",
            "⏳ Working — 3 min — iteration 2/90, execute_code",
            (
                "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9."
                "eyJzdWIiOiIxMjM0NTY3ODkwIn0."
                "sflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
            ),
            (
                "这是因为定时任务的自动执行回执和正常发消息混在了一起。"
                "后台智能体会先产生内部协作内容，包括发送工具的调用结果、"
                "记录 ID、命令或调试信息，以及已生成回复准备发送之类的内部状态。"
            ),
            (
                "企业微信的安全过滤会拦截这类内容，不允许把执行信息直接发给人；"
                "刚才测试消息能够成功，说明发送通道本身是通的；"
                "问题出在执行过程被转成给用户看的回复，本质上是保护机制在兜底。"
            ),
        ]:
            with self.subTest(message_text=message_text):
                report = json.loads(
                    self.module.handle_qintopia_xiaoman_activity_text_group_message_request_prepare(
                        {
                            "source_record_id": "activity_plan:abc123def456",
                            "approved_artifact_id": "44444444-4444-4444-8444-444444444444",
                            "message_text": message_text,
                        }
                    )
                )

                self.assertFalse(report["success"])
                self.assertEqual(
                    report["error"],
                    "message_text contains disallowed sensitive or raw internal content",
                )

    def test_xiaoman_activity_text_group_message_request_prepare_returns_public_rewrite_draft(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_text_group_message_request_prepare(
                {
                    "source_record_id": "activity_plan:abc123def456",
                    "approved_artifact_id": "44444444-4444-4444-8444-444444444444",
                    "message_text": (
                        "⏳ Working — 3 min — iteration 2/90, execute_code\n"
                        "记录 ID recABCDEFG123456789，路径 /home/ubuntu/qintopia/debug.log"
                    ),
                    "public_conclusion": "下周活动预告草稿已经准备好。",
                    "public_next_step": "请确认时间、地点和负责人，确认后我再整理成可发到群里的版本。",
                }
            )
        )

        self.assertFalse(report["success"])
        self.assertTrue(report["hidden_original"])
        self.assertEqual(report["rewrite_trigger"], "用人话重述")
        self.assertEqual(
            report["public_rewrite_draft"],
            "下周活动预告草稿已经准备好。\n下一步：请确认时间、地点和负责人，确认后我再整理成可发到群里的版本。",
        )
        serialized = json.dumps(report, ensure_ascii=False)
        self.assertNotIn("recABCDEFG123456789", serialized)
        self.assertNotIn("/home/ubuntu/qintopia/debug.log", serialized)
        self.assertNotIn("execute_code", serialized)

    def test_xiaoman_public_reply_rewrite_keeps_only_conclusion_and_next_step(self):
        report = json.loads(
            self.module.handle_qintopia_xiaoman_public_reply_rewrite(
                {
                    "blocked_reply": (
                        "⏳ Working — 3 min — iteration 2/90, execute_code\n"
                        "记录 ID recABCDEFG123456789，路径 /home/ubuntu/qintopia/debug.log"
                    ),
                    "public_conclusion": "下周活动预告草稿已经准备好。",
                    "public_next_step": "请确认时间、地点和负责人，确认后我再整理成可发到群里的版本。",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertTrue(report["hidden_original"])
        self.assertEqual(report["rewrite_trigger"], "用人话重述")
        self.assertEqual(
            report["public_reply"],
            "下周活动预告草稿已经准备好。\n下一步：请确认时间、地点和负责人，确认后我再整理成可发到群里的版本。",
        )
        serialized = json.dumps(report, ensure_ascii=False)
        self.assertNotIn("recABCDEFG123456789", serialized)
        self.assertNotIn("/home/ubuntu/qintopia/debug.log", serialized)
        self.assertNotIn("execute_code", serialized)
    def test_xiaoman_public_reply_rewrite_rejects_internal_mechanism_explanations(self):
        report = json.loads(
            self.module.handle_qintopia_xiaoman_public_reply_rewrite(
                {
                    "blocked_reply": "为什么总是回复内部执行信息已自动隐藏？",
                    "public_conclusion": (
                        "这是因为定时任务的自动执行回执和正常发消息混在了一起，"
                        "后台智能体会产生工具调用结果和内部状态。"
                    ),
                    "public_next_step": "后续会把任务改成只发送固定正文。",
                }
            )
        )

        self.assertFalse(report["success"])
        self.assertTrue(report["hidden_original"])
        self.assertEqual(report["rewrite_trigger"], "用人话重述")
        self.assertEqual(report["missing_rewrite_fields"], ["public_conclusion"])

    def test_xiaoman_public_reply_rewrite_rejects_guard_mechanism_explanations(self):
        report = json.loads(
            self.module.handle_qintopia_xiaoman_public_reply_rewrite(
                {
                    "blocked_reply": "为什么总是回复内部执行信息已自动隐藏？",
                    "public_conclusion": (
                        "企业微信的安全过滤会拦截这类内容，说明发送通道本身是通的，"
                        "问题出在执行过程被转成给用户看的回复。"
                    ),
                    "public_next_step": "后续要避免执行过程变成人看的回复。",
                }
            )
        )

        self.assertFalse(report["success"])
        self.assertTrue(report["hidden_original"])
        self.assertEqual(report["rewrite_trigger"], "用人话重述")
        self.assertEqual(
            report["missing_rewrite_fields"],
            ["public_conclusion", "public_next_step"],
        )

    def test_xiaoman_activity_announcement_prepare_rejects_non_integer_material_followup(self):
        self.enable_xiaoman_activity_wrappers()

        elapsed_report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    "date": "2026-07-21",
                    "mode": "post_event_followup",
                    "post_event_elapsed_hours": "72",
                    "records": [{"title": "木作体验课"}],
                }
            )
        )
        attempt_report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    "date": "2026-07-21",
                    "mode": "post_event_followup",
                    "material_followup_attempt": True,
                    "records": [{"title": "木作体验课"}],
                }
            )
        )

        self.assertFalse(elapsed_report["success"])
        self.assertEqual(elapsed_report["error"], "post_event_elapsed_hours must be an integer")
        self.assertFalse(attempt_report["success"])
        self.assertEqual(attempt_report["error"], "material_followup_attempt must be an integer")

    def test_xiaoman_activity_announcement_prepare_needs_records_or_read_through(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    "date": "2026-07-21",
                }
            )
        )

        self.assertFalse(report["success"])
        self.assertEqual(report["error"], "records are required unless read-through is enabled")
        self.assertFalse(report["action"]["external_send_executed"])

    def test_xiaoman_activity_list_by_date_read_through_returns_records(self):
        self.enable_xiaoman_activity_wrappers()
        fake_sidecar = self.write_fake_xiaoman_sidecar(
            {
                "success": True,
                "worker": "xiaoman-activity",
                "source": "fixture",
                "apply_requested": True,
                "validation_status": "ok",
                "action_status": "read_ok",
                "safe_for_chat": False,
                "record_count": 1,
                "records": [
                    {
                        "table_role": "activity_occurrence",
                        "record_ref": "activity_occurrence:abc123def456",
                        "title": "今日共创晚餐",
                        "activity_date": "2026-07-16",
                        "location": "秦托邦共享厨房",
                        "status": "待宣传",
                    }
                ],
                "summaries": ["今日共创晚餐｜2026-07-16｜秦托邦共享厨房｜待宣传"],
                "limitations": ["fixture-backed read"],
                "guardrails": ["record_ref is hashed"],
            }
        )
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertTrue(report["read_through"])
        self.assertEqual(report["action"]["tool"], "agentos_worker_read_through")
        self.assertFalse(report["action"]["requires_local_execution"])
        self.assertEqual(report["record_count"], 1)
        self.assertEqual(report["records"][0]["title"], "今日共创晚餐")
        self.assertEqual(report["summaries"], ["今日共创晚餐｜2026-07-16｜秦托邦共享厨房｜待宣传"])
        self.assertNotIn("command", report["action"])

    def test_xiaoman_activity_read_through_uses_minimal_environment(self):
        self.enable_xiaoman_activity_wrappers()
        fake_sidecar = self.write_env_echo_xiaoman_sidecar()
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"
        os.environ["SECRET_TOKEN"] = "do-not-pass-this"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertTrue(report["success"])
        self.assertNotIn("do-not-pass-this", rendered)
        self.assertIn("secret-not-inherited", rendered)

    def test_xiaoman_activity_read_through_filters_record_fields_and_raw_summaries(self):
        self.enable_xiaoman_activity_wrappers()
        fake_sidecar = self.write_fake_xiaoman_sidecar(
            {
                "success": True,
                "worker": "xiaoman-activity",
                "source": "fixture",
                "record_count": 1,
                "records": [
                    {
                        "table_role": "activity_occurrence",
                        "record_ref": "activity_occurrence:abc123def456",
                        "title": "今日共创晚餐",
                        "activity_date": "2026-07-16",
                        "location": "秦托邦共享厨房",
                        "status": "待宣传",
                        "notes": "postgres://secret",
                        "raw_table_id": "tbl_secret",
                        "secret_url": "postgres://secret",
                    }
                ],
                "summaries": ["raw tbl_secret postgres://secret"],
                "limitations": ["raw tbl_secret"],
                "guardrails": ["postgres://secret"],
            }
        )
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertTrue(report["success"])
        self.assertEqual(set(report["records"][0]), {
            "table_role",
            "record_ref",
            "title",
            "activity_date",
            "location",
            "status",
        })
        self.assertEqual(report["summaries"], ["今日共创晚餐｜2026-07-16｜秦托邦共享厨房｜待宣传"])
        self.assertNotIn("tbl_secret", rendered)
        self.assertNotIn("postgres://secret", rendered)

    def test_xiaoman_activity_read_through_filters_actual_sensitive_env_values(self):
        self.enable_xiaoman_activity_wrappers()
        sensitive_token = "plain-random-token-value"
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN"] = sensitive_token
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS"] = "allowed-token-one,allowed-token-two"
        fake_sidecar = self.write_fake_xiaoman_sidecar(
            {
                "success": True,
                "worker": "xiaoman-activity",
                "source": "fixture",
                "record_count": 1,
                "records": [
                    {
                        "table_role": "activity_occurrence",
                        "record_ref": "activity_occurrence:abc123def456",
                        "title": sensitive_token,
                        "activity_date": "2026-07-16",
                        "location": "location includes allowed-token-two",
                        "status": "待宣传",
                        "notes": "ordinary note",
                    }
                ],
            }
        )
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertTrue(report["success"])
        self.assertNotIn("title", report["records"][0])
        self.assertNotIn("location", report["records"][0])
        self.assertEqual(report["records"][0]["notes"], "ordinary note")
        self.assertNotIn(sensitive_token, rendered)
        self.assertNotIn("allowed-token-two", rendered)

    def test_xiaoman_activity_read_through_does_not_return_raw_payload(self):
        self.enable_xiaoman_activity_wrappers()
        fake_sidecar = self.write_fake_xiaoman_sidecar(
            {
                "success": True,
                "worker": "xiaoman-activity",
                "source": "fixture",
                "record_count": 1,
                "records": [
                    {
                        "table_role": "activity_occurrence",
                        "record_ref": "activity_occurrence:abc123def456",
                        "title": "今日共创晚餐",
                        "activity_date": "2026-07-16",
                        "location": "秦托邦共享厨房",
                        "status": "待宣传",
                    }
                ],
            }
        )
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_record_get(
                {
                    "record_id": "rec_secret_raw_id",
                    "table_role": "activity_occurrence",
                    "dry_run": False,
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertTrue(report["success"])
        self.assertNotIn("payload", report)
        self.assertNotIn("rec_secret_raw_id", rendered)
        self.assertEqual(report["query"]["operation"], "record-get")
        self.assertEqual(report["query"]["table_role"], "activity_occurrence")

    def test_xiaoman_activity_read_through_does_not_return_child_error_output(self):
        self.enable_xiaoman_activity_wrappers()
        fake_sidecar = self.write_raw_xiaoman_sidecar(
            "postgres://secret-in-stdout",
            stderr="feishu table token secret-in-stderr",
            exit_code=2,
        )
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertFalse(report["success"])
        self.assertEqual(report["error"], "xiaoman activity worker command failed")
        self.assertNotIn("postgres://secret-in-stdout", rendered)
        self.assertNotIn("secret-in-stderr", rendered)
        self.assertNotIn("worker_stderr_summary", report)

    def test_xiaoman_activity_read_through_does_not_return_invalid_json_body(self):
        self.enable_xiaoman_activity_wrappers()
        fake_sidecar = self.write_raw_xiaoman_sidecar("not-json secret table-id")
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertFalse(report["success"])
        self.assertEqual(report["error"], "xiaoman activity worker returned invalid JSON")
        self.assertNotIn("not-json secret table-id", rendered)

    def test_xiaoman_activity_read_through_rejects_large_output(self):
        self.enable_xiaoman_activity_wrappers()
        fake_sidecar = self.write_raw_xiaoman_sidecar("x" * (70 * 1024))
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(fake_sidecar)
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"] = "1"

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_list_by_date(
                {
                    "date": "2026-07-16",
                    "table_role": "activity_occurrence",
                }
            )
        )

        rendered = json.dumps(report, ensure_ascii=False)
        self.assertFalse(report["success"])
        self.assertEqual(report["error"], "xiaoman activity read-through output is too large")
        self.assertNotIn("x" * 100, rendered)

    def test_xiaoman_activity_promotion_review_draft_returns_human_review_payload(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_promotion_review_draft(
                {
                    "records": [
                        {
                            "table_role": "activity_occurrence",
                            "record_ref": "activity_occurrence:abc123def456",
                            "title": "今日共创晚餐",
                            "activity_date": "2026-07-17",
                            "start_time": "19:00",
                            "location": "秦托邦共享厨房",
                            "status": "待宣传",
                            "promotion_status": "可宣传",
                            "material_summary": "适合邻里共创和新朋友参与",
                            "owner_name": "小满",
                            "preannounce_decision": "需要",
                            "preannounce_channels": "朋友圈",
                            "human_reviewer": "刘珊",
                        }
                    ],
                    "audience": "秦托邦成员群",
                    "promotion_goal": "邀请成员今晚参与共创晚餐",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertEqual(report["read_model"], "already_read_sanitized_activity_record")
        self.assertEqual(report["promotion_assessment"], "promote")
        self.assertIn("今日共创晚餐", report["activity_summary"])
        self.assertIn("秦托邦共享厨房", report["copy_draft"]["group_message"])
        self.assertTrue(report["copy_draft"]["human_review_required"])
        self.assertEqual(report["poster_brief"]["title"], "今日共创晚餐")
        next_path = report["after_human_confirmation"]
        self.assertEqual(next_path["status"], "ready_for_human_confirmation")
        self.assertEqual(next_path["after_confirmation_tool"], "qintopia_xiaoman_activity_handoff_create")
        self.assertTrue(next_path["dry_run_first"])
        self.assertTrue(next_path["payload"]["dry_run"])
        self.assertEqual(next_path["payload"]["target_agent"], "huabaosi")
        self.assertEqual(next_path["payload"]["source_record_id"], "activity_occurrence:abc123def456")
        rendered = json.dumps(report, ensure_ascii=False)
        self.assertNotIn("command", rendered)
        self.assertIn("does not read Feishu", rendered)

    def test_xiaoman_activity_promotion_review_draft_holds_missing_fields(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_promotion_review_draft(
                {
                    "activity": {
                        "table_role": "activity_occurrence",
                        "record_ref": "activity_occurrence:abc123def456",
                        "title": "今日共创晚餐",
                        "status": "待宣传",
                    }
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertEqual(report["promotion_assessment"], "needs_more_info")
        self.assertEqual(report["after_human_confirmation"]["status"], "needs_human_review")
        self.assertIn("活动时间", report["missing_fields"])
        self.assertIn("活动地点", report["missing_fields"])
        self.assertIn("负责人", report["missing_fields"])
        self.assertIn("是否需要前宣", report["missing_fields"])
        self.assertNotIn("payload", report["after_human_confirmation"])

    def test_xiaoman_activity_promotion_details_update_builds_agentos_command(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_promotion_details_update(
                {
                    "event_signal_id": "11111111-1111-4111-8111-111111111111",
                    "mutation_id": "22222222-2222-4222-8222-222222222222",
                    "activity_owner_name": "小满",
                    "location": "秦托邦社区公区一楼",
                    "preannounce_decision": "需要",
                    "preannounce_channels": ["朋友圈"],
                    "human_reviewer": "刘珊",
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertEqual(report["operation"], "promotion-details-update")
        self.assertTrue(report["dry_run"])
        self.assertFalse(report["payload"].get("record_id"))
        self.assertEqual(report["payload"]["preannounce_channels"], ["朋友圈"])
        self.assertIn("promotion-details-update", report["action"]["command"])

    def test_xiaoman_activity_promotion_details_complete_draft_and_handoff_payload(self):
        self.enable_xiaoman_activity_wrappers()

        report = json.loads(
            self.module.handle_qintopia_xiaoman_activity_promotion_review_draft(
                {
                    "activity": {
                        "table_role": "activity_plan",
                        "record_ref": "activity_plan:abc123def456",
                        "title": "瑜伽体验",
                        "activity_date": "2026-07-27",
                        "start_time": "周六下午",
                        "status": "待宣传",
                    },
                    "promotion_details": {
                        "activity_owner_name": "小满",
                        "location": "秦托邦社区公区一楼",
                        "preannounce_decision": "需要",
                        "preannounce_channels": ["朋友圈"],
                        "human_reviewer": "刘珊",
                    },
                    "source_event_signal_id": "33333333-3333-4333-8333-333333333333",
                }
            )
        )

        self.assertEqual(report["missing_fields"], [])
        self.assertEqual(report["promotion_assessment"], "promote")
        self.assertIn("负责人：小满", report["after_human_confirmation"]["payload"]["brief_summary"])
        self.assertEqual(
            report["after_human_confirmation"]["payload"]["source_event_signal_id"],
            "33333333-3333-4333-8333-333333333333",
        )

    def test_xiaoqin_product_search_is_public_only_and_has_baselines(self):
        payload = json.loads(
            self.module.handle_qintopia_external_product_kb_search(
                {"query": "Agent OS 可以做什么", "purpose": "回答外部客户"}
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["scope_used"], ["Public"])
        self.assertGreaterEqual(payload["result_count"], 1)
        self.assertEqual(payload["results"][0]["path"], "agent-os-public-faq.md")
        self.assertIn("Internal", payload["not_accessed"])
        self.assertTrue(payload["approved_public_baselines"])

    def test_xiaoqin_public_case_search_does_not_invent_cases(self):
        payload = json.loads(self.module.handle_qintopia_public_case_search({"query": "客户案例"}))

        self.assertTrue(payload["success"])
        self.assertEqual(payload["result_count"], 0)
        self.assertFalse(payload["approved_public_cases_available"])
        self.assertTrue(payload["needs_human_review"])
        self.assertIn("没有检索到已批准公开的客户案例", payload["safe_customer_message"])
        self.assertNotIn("Human Owner", payload["safe_customer_message"])

    def test_xiaoqin_customer_context_lookup_is_current_channel_only(self):
        payload = json.loads(
            self.module.handle_qintopia_customer_context_lookup(
                {
                    "customer_display_name": "某客户",
                    "source_channel": "wechat_external",
                    "source_conversation_id": "conv_1",
                    "customer_provided_context": "想了解 AI 客服试点。",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["mode"], "current_channel_context_only")
        self.assertFalse(payload["stored_context_found"])
        self.assertIn("CRM", payload["not_accessed"])

    def test_dify_dataset_list_filters_configured_allowlist(self):
        os.environ["QINTOPIA_DIFY_ALLOWED_DATASET_IDS"] = "ds_allowed"

        def fake_request(method, path, *, params=None, body=None):
            self.assertEqual(method, "GET")
            self.assertEqual(path, "/datasets")
            self.assertEqual(params["page"], 1)
            return {
                "success": True,
                "status": 200,
                "data": {
                    "data": [
                        {"id": "ds_allowed", "name": "Allowed"},
                        {"id": "ds_other", "name": "Other"},
                    ],
                    "total": 2,
                },
            }

        self.module._dify_request = fake_request
        payload = json.loads(self.module.handle_qintopia_dify_dataset_list({"limit": 50}))

        self.assertTrue(payload["success"])
        self.assertTrue(payload["read_only"])
        self.assertTrue(payload["filtered_to_allowed_datasets"])
        self.assertEqual(payload["data"]["data"], [{"id": "ds_allowed", "name": "Allowed"}])
        self.assertTrue(payload["data"]["filtered_by_allowlist"])

    def test_dify_retrieve_uses_fixed_read_only_endpoint(self):
        os.environ["QINTOPIA_DIFY_ALLOWED_DATASET_IDS"] = "ds_allowed"
        captured = {}

        def fake_request(method, path, *, params=None, body=None):
            captured.update({"method": method, "path": path, "params": params, "body": body})
            return {
                "success": True,
                "status": 200,
                "data": {"records": [{"segment": {"content": "秦托邦知识片段"}, "score": 0.91}]},
            }

        self.module._dify_request = fake_request
        payload = json.loads(
            self.module.handle_qintopia_dify_knowledge_retrieve(
                {
                    "dataset_id": "ds_allowed",
                    "query": "秦托邦是什么",
                    "top_k": 5,
                    "score_threshold_enabled": False,
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(captured["method"], "POST")
        self.assertEqual(captured["path"], "/datasets/ds_allowed/retrieve")
        self.assertIsNone(captured["params"])
        self.assertEqual(captured["body"]["query"], "秦托邦是什么")
        self.assertEqual(captured["body"]["retrieval_model"]["search_method"], "semantic_search")
        self.assertEqual(captured["body"]["retrieval_model"]["top_k"], 5)
        self.assertFalse(captured["body"]["retrieval_model"]["score_threshold_enabled"])
        self.assertFalse(captured["body"]["retrieval_model"]["reranking_enable"])
        self.assertTrue(payload["read_only"])
        self.assertNotIn("test-knowledge-key", json.dumps(payload, ensure_ascii=False))

    def test_dify_read_tools_block_unallowed_dataset_before_network(self):
        os.environ["QINTOPIA_DIFY_ALLOWED_DATASET_IDS"] = "ds_allowed"

        def fail_request(*args, **kwargs):
            raise AssertionError("network should not be called for denied dataset")

        self.module._dify_request = fail_request
        payload = json.loads(
            self.module.handle_qintopia_dify_document_list(
                {"dataset_id": "ds_denied", "page": 1, "limit": 10}
            )
        )

        self.assertFalse(payload["success"])
        self.assertEqual(payload["dataset_id"], "ds_denied")
        self.assertIn("allowlist", payload["error"])

    def test_wenyuange_lookup_returns_safe_basis_without_raw_long_chunk(self):
        long_content = "秦托邦有共享办公区、活动空间和来访须知。" * 30

        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.82,
                                "segment": {
                                    "id": "seg_1",
                                    "document_id": "doc_1",
                                    "content": long_content,
                                    "document": {"name": "公开 FAQ.md"},
                                },
                            }
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "来访前要知道什么",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区成员问题",
                    "top_k": 3,
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["can_answer"])
        self.assertEqual(payload["result_count"], 1)
        self.assertLessEqual(len(payload["answer_basis"]), 1000)
        self.assertNotEqual(payload["answer_basis"], long_content)
        self.assertEqual(payload["sources"][0]["segment_id"], "seg_1")
        self.assertNotIn("test-knowledge-key", json.dumps(payload, ensure_ascii=False))

    def test_wenyuange_lookup_blocks_xiaoqin_internal_or_member_content(self):
        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.72,
                                "segment": {
                                    "id": "seg_2",
                                    "document_id": "doc_2",
                                    "content": "内部客户案例涉及成员资料和服务器日志，未公开。",
                                    "document": {"name": "internal-case.md"},
                                },
                            }
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "能介绍客户案例吗",
                    "caller_profile": "xiaoqin",
                    "audience": "external_customer",
                    "purpose": "回答外部客户",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertFalse(payload["can_answer"])
        self.assertEqual(payload["answer_basis"], "")
        self.assertIn("member_scoped", payload["risk_flags"])
        self.assertIn("internal_information", payload["risk_flags"])

    def test_wenyuange_lookup_blocks_erhua_member_privacy_and_complaint(self):
        def fake_retrieve(args, **kwargs):
            return json.dumps(
                {
                    "success": True,
                    "data": {
                        "records": [
                            {
                                "score": 0.66,
                                "segment": {
                                    "id": "seg_3",
                                    "document_id": "doc_3",
                                    "content": "成员档案包含房间、生日和入住时间。",
                                    "document": {"name": "profiles.md"},
                                },
                            }
                        ]
                    },
                },
                ensure_ascii=False,
            )

        self.module.handle_qintopia_dify_knowledge_retrieve = fake_retrieve
        payload = json.loads(
            self.module.handle_qintopia_wenyuange_lookup(
                {
                    "query": "我要投诉入住体验不好，问一下他的房间",
                    "caller_profile": "erhua",
                    "audience": "member_reply",
                    "purpose": "回答社区群消息",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertFalse(payload["can_answer"])
        self.assertIn("member_privacy", payload["risk_flags"])
        self.assertIn("complaint_or_service_recovery", payload["risk_flags"])

    def test_xiaoqin_lead_capture_creates_controlled_sales_task(self):
        self.module._kanban_create_sales_task = lambda title, body, task_type, priority, key: ("t_sales", "created")

        payload = json.loads(
            self.module.handle_qintopia_lead_capture(
                {
                    "task_type": "demo_request",
                    "customer_display_name": "某客户",
                    "source_channel": "wechat_external",
                    "source_conversation_id": "conv_1",
                    "source_message_id": "msg_1",
                    "customer_request": "想看 Agent OS 销售客服演示。",
                    "business_scenario": "企业微信客户咨询分流。",
                    "budget_range": "待确认",
                    "urgency": "本月",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["task_id"], "t_sales")
        self.assertEqual(payload["task_type"], "demo_request")
        action = payload["actions"][0]
        self.assertEqual(action["action"], "kanban_task_create_request")
        self.assertEqual(action["assignee"], "xiaoqin")
        self.assertEqual(action["status"], "triage")
        self.assertIn("safe_customer_message", payload)
        self.assertIn("我先帮您记录下来", payload["safe_customer_message"])
        self.assertNotIn("t_sales", payload["safe_customer_message"])
        self.assertNotIn("任务", payload["safe_customer_message"])
        self.assertNotIn("复核", payload["safe_customer_message"])
        self.assertIn("Use safe_customer_message", payload["customer_response_policy"][0])
        self.assertNotIn("Human Owner", json.dumps(payload, ensure_ascii=False))
        self.assertNotIn("kanban_assign", json.dumps(payload, ensure_ascii=False))

    def test_xiaoqin_sales_task_create_uses_hermes_initial_status_contract(self):
        captured = []

        class FakeConnection:
            def close(self):
                captured.append({"closed": True})

        class FakeKanban:
            def create_task(self, conn, **kwargs):
                captured.append(kwargs)
                return "t_sales"

        self.module._kanban_runtime = lambda: (FakeKanban(), FakeConnection())

        task_id, status = self.module._kanban_create_sales_task("标题", "正文", "proposal", 1, "key-1")
        self.assertEqual(task_id, "t_sales")
        self.assertEqual(status, "created")
        self.assertEqual(captured[0]["initial_status"], "running")
        self.assertFalse(captured[0]["triage"])

        captured.clear()
        self.module._kanban_create_sales_task("标题", "正文", "external_disclosure_review", 1, "key-2")
        self.assertEqual(captured[0]["initial_status"], "blocked")

    def test_xiaoqin_lead_capture_rejects_uncontrolled_task_type(self):
        payload = json.loads(
            self.module.handle_qintopia_lead_capture(
                {
                    "task_type": "engineering",
                    "source_channel": "wechat_external",
                    "source_conversation_id": "conv_1",
                    "customer_request": "帮我改服务器。",
                }
            )
        )

        self.assertFalse(payload["success"])
        self.assertIn("not allowed", payload["error"])

    def test_xiaoqin_proposal_and_demo_are_review_gated(self):
        proposal = json.loads(
            self.module.handle_qintopia_proposal_outline_generate(
                {
                    "customer_display_name": "某客户",
                    "business_scenario": "客户想把客服咨询沉淀成任务。",
                    "goals": "降低漏跟进。",
                }
            )
        )
        demo = json.loads(
            self.module.handle_qintopia_demo_script_generate(
                {
                    "demo_goal": "展示需求收集到任务交接",
                    "business_scenario": "企业微信销售客服咨询。",
                    "allowed_materials": "公开样例。",
                }
            )
        )

        self.assertTrue(proposal["success"])
        self.assertTrue(proposal["requires_human_review_before_external_send"])
        self.assertIn("草案", proposal["draft"])
        self.assertTrue(demo["success"])
        self.assertTrue(demo["requires_human_review_before_external_send"])
        self.assertIn("公开样例", demo["script"])

    def test_xiaoqin_disclosure_filter_blocks_sensitive_draft(self):
        payload = json.loads(
            self.module.handle_qintopia_external_disclosure_filter(
                {
                    "draft_answer": "我们可以给你固定报价和 SLA，也能展示内部服务器日志。",
                    "purpose": "回复外部客户",
                    "recipient": "客户",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertTrue(payload["approval_required"])
        self.assertIn("commercial_commitment", payload["matched_risk_categories"])
        self.assertIn("internal_information", payload["matched_risk_categories"])
        self.assertNotIn("服务器日志", payload["public_safe_draft"])

    def test_xiaoqin_conversation_summary_suggests_disclosure_review_on_risk(self):
        payload = json.loads(
            self.module.handle_qintopia_conversation_summary(
                {
                    "conversation_text": "客户想看内部客户案例和报价合同。",
                    "customer_display_name": "某客户",
                    "source_channel": "wechat_external",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["suggested_task_type"], "external_disclosure_review")
        self.assertIn("需要团队负责人决策", payload["summary"])
        self.assertNotIn("Human Owner", payload["summary"])

    def test_complaint_intake_create_is_controlled(self):
        self.module._kanban_create_complaint = lambda title, body, priority, key: ("t_test", "created")

        payload = json.loads(
            self.module.handle_qintopia_complaint_intake_create(
                {
                    "source_channel": "qiwe_group_internal",
                    "source_conversation_id": "conv_1",
                    "source_message_id": "msg_1",
                    "requester_display_name": "小秦",
                    "requester_channel_user_id": "user_1",
                    "original_message": "我要投诉入住体验，晚上太吵了。",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["task_id"], "t_test")
        self.assertEqual(payload["task_type"], "complaint_intake")
        self.assertEqual(payload["owner_profile"], "default")
        self.assertEqual(payload["tenant"], "qintopia")
        kanban_action = payload["actions"][0]
        self.assertEqual(kanban_action["action"], "kanban_task_create_request")
        self.assertEqual(kanban_action["task_type"], "complaint_intake")
        self.assertEqual(kanban_action["assignee"], "default")
        self.assertEqual(kanban_action["status"], "triage")
        self.assertNotIn("silaoshi", json.dumps(payload, ensure_ascii=False))
        private_action = payload["actions"][1]
        self.assertEqual(private_action["tool"], "qiwe_send_direct_message")
        self.assertEqual(private_action["recipient_user_id"], "user_1")
        self.assertEqual(private_action["conversation_scope"], "private")
        self.assertEqual(private_action["purpose"], "complaint_intake_detail_collection")
        self.assertEqual(private_action["idempotency_key"], f'{kanban_action["idempotency_key"]}:direct:intake')
        self.assertIn("为了避免在群里公开你的细节", private_action["message"])

    def test_complaint_intake_create_uses_qiwe_session_sender_id(self):
        old_user_id = os.environ.get("HERMES_SESSION_USER_ID")
        captured = {}

        def fake_create(title, body, priority, key):
            captured["body"] = body
            captured["key"] = key
            return "t_test", "created"

        self.module._kanban_create_complaint = fake_create
        os.environ["HERMES_SESSION_USER_ID"] = "7881303308049798"
        try:
            payload = json.loads(
                self.module.handle_qintopia_complaint_intake_create(
                    {
                        "source_channel": "qiwe_group_internal",
                        "source_conversation_id": "room_1",
                        "source_message_id": "msg_1",
                        "requester_display_name": "弦默",
                        "original_message": "我要投诉，房间门锁有问题。",
                    }
                )
            )
        finally:
            if old_user_id is None:
                os.environ.pop("HERMES_SESSION_USER_ID", None)
            else:
                os.environ["HERMES_SESSION_USER_ID"] = old_user_id

        self.assertTrue(payload["success"])
        self.assertTrue(payload["requester_channel_user_id_resolved"])
        self.assertIn("7881303308049798", captured["body"])
        self.assertIn("不要再创建或派发“二花私聊补充受理”子任务", captured["body"])
        private_action = payload["actions"][1]
        self.assertEqual(private_action["recipient_user_id"], "7881303308049798")
        self.assertEqual(private_action["purpose"], "complaint_intake_detail_collection")
        self.assertEqual(private_action["idempotency_key"], f"{captured['key']}:direct:intake")

    def test_complaint_intake_update_appends_comment_only(self):
        self.module._kanban_add_complaint_comment = lambda task_id, body: (12, "comment_added")

        payload = json.loads(
            self.module.handle_qintopia_complaint_intake_update(
                {
                    "task_id": "t_test",
                    "requester_display_name": "小秦",
                    "details": "昨晚 11 点后 2 栋走廊持续很吵。",
                    "location_or_area": "2 栋走廊",
                    "expected_resolution": "希望有人回复处理方式。",
                }
            )
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["task_id"], "t_test")
        self.assertEqual(payload["comment_id"], 12)
        action = payload["actions"][0]
        self.assertEqual(action["action"], "kanban_comment_add_request")
        self.assertTrue(action["does_not_assign_executor"])
        self.assertNotIn("kanban_assign", json.dumps(payload, ensure_ascii=False))

    def test_complaint_followup_requires_approved_resolution_and_private_user(self):
        missing = json.loads(
            self.module.handle_qintopia_complaint_followup_send(
                {
                    "task_id": "t_test",
                    "requester_channel_user_id": "user_1",
                    "approved_resolution": "",
                }
            )
        )
        self.assertFalse(missing["success"])

        self.module._kanban_add_complaint_comment = lambda task_id, body: (13, "comment_added")
        payload = json.loads(
            self.module.handle_qintopia_complaint_followup_send(
                {
                    "task_id": "t_test",
                    "requester_channel_user_id": "user_1",
                    "requester_display_name": "小秦",
                    "approved_resolution": "已安排工作人员检查并完成走廊夜间提醒。",
                }
            )
        )

        self.assertTrue(payload["success"])
        action = payload["actions"][0]
        self.assertEqual(action["tool"], "qiwe_send_direct_message")
        self.assertEqual(action["recipient_user_id"], "user_1")
        self.assertEqual(action["conversation_scope"], "private")
        self.assertEqual(action["purpose"], "complaint_resolution_followup")
        self.assertIn("idempotency_key", action)
        self.assertTrue(action["requires_approved_resolution"])
        self.assertIn("已安排工作人员检查", action["message"])

    def test_xiaoman_activity_mutations_use_agentos_event_signal_contract(self):
        old_profile = os.environ.get("QINTOPIA_PROFILE_ID")
        old_enabled = os.environ.get("QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE")
        os.environ["QINTOPIA_PROFILE_ID"] = "xiaoman"
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"] = "1"
        try:
            payload = json.loads(
                self.module.handle_qintopia_xiaoman_activity_status_update(
                    {
                        "event_signal_id": "66666666-6666-4666-8666-666666666666",
                        "mutation_id": "77777777-7777-4777-8777-777777777777",
                        "status": "处理中",
                    }
                )
            )
            gap_payload = json.loads(
                self.module.handle_qintopia_xiaoman_activity_gap_update(
                    {
                        "event_signal_id": "88888888-8888-4888-888888888888",
                        "mutation_id": "99999999-9999-4999-8999-999999999999",
                        "gap_summary": "缺少报名截止时间",
                    }
                )
            )
        finally:
            if old_profile is None:
                os.environ.pop("QINTOPIA_PROFILE_ID", None)
            else:
                os.environ["QINTOPIA_PROFILE_ID"] = old_profile
            if old_enabled is None:
                os.environ.pop("QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE", None)
            else:
                os.environ["QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"] = old_enabled

        self.assertTrue(payload["success"])
        self.assertEqual(payload["operation"], "status-update")
        self.assertEqual(payload["payload"]["event_signal_id"], "66666666-6666-4666-8666-666666666666")
        self.assertEqual(payload["payload"]["mutation_id"], "77777777-7777-4777-8777-777777777777")
        self.assertNotIn("record_id", payload["payload"])
        self.assertNotIn("table_role", payload["payload"])
        status_payload_arg = json.loads(payload["action"]["command"][4])
        self.assertEqual(status_payload_arg["event_signal_id"], payload["payload"]["event_signal_id"])
        self.assertEqual(status_payload_arg["mutation_id"], payload["payload"]["mutation_id"])
        self.assertTrue(gap_payload["success"])
        self.assertEqual(gap_payload["operation"], "gap-update")
        self.assertNotIn("record_id", gap_payload["payload"])
        self.assertNotIn("table_role", gap_payload["payload"])

    def test_xiaoman_activity_handoff_schema_matches_mapped_rust_boundary(self):
        handoff_schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_HANDOFF_CREATE_SCHEMA["parameters"]
        self.assertEqual(handoff_schema["properties"]["handoff_type"]["enum"], ["visual_asset_request"])
        self.assertEqual(handoff_schema["properties"]["target_agent"]["enum"], ["huabaosi"])

        draft_schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_REVIEW_DRAFT_SCHEMA["parameters"]
        self.assertIn("records", draft_schema["properties"])
        self.assertIn("activity", draft_schema["properties"])
        self.assertNotIn("send", draft_schema["properties"])
        self.assertNotIn("feishu_record_id", draft_schema["properties"])

        text_group_schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_TEXT_GROUP_MESSAGE_REQUEST_PREPARE_SCHEMA[
            "parameters"
        ]
        self.assertEqual(
            text_group_schema["required"],
            ["source_record_id", "approved_artifact_id", "message_text"],
        )
        self.assertNotIn("target_group_id", text_group_schema["properties"])

        status_schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_STATUS_UPDATE_SCHEMA["parameters"]
        self.assertEqual(status_schema["required"], ["event_signal_id", "mutation_id", "status"])
        self.assertNotIn("record_id", status_schema["properties"])
        self.assertNotIn("table_role", status_schema["properties"])

        details_schema = self.module.QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_DETAILS_UPDATE_SCHEMA[
            "parameters"
        ]
        self.assertIn("activity_owner_name", details_schema["properties"])
        self.assertEqual(
            details_schema["properties"]["preannounce_decision"]["enum"],
            ["需要", "不需要"],
        )
        self.assertNotIn("record_id", details_schema["properties"])

    def test_register_exposes_frontline_tools_without_raw_dify_by_default(self):
        class FakeCtx:
            def __init__(self) -> None:
                self.names = []
                self.hooks = {}

            def register_tool(self, **kwargs) -> None:
                self.names.append(kwargs["name"])

            def register_hook(self, name, callback) -> None:
                self.hooks[name] = callback

        os.environ["QINTOPIA_PROFILE_ID"] = "xiaoqin"
        os.environ.pop("QINTOPIA_DIFY_RAW_TOOLS_ENABLE", None)
        ctx = FakeCtx()
        self.module.register(ctx)

        self.assertIn("qintopia_wenyuange_lookup", ctx.names)
        self.assertIn("qintopia_complaint_intake_create", ctx.names)
        self.assertIn("qintopia_complaint_intake_update", ctx.names)
        self.assertIn("qintopia_complaint_followup_send", ctx.names)
        self.assertIn("qintopia_external_product_kb_search", ctx.names)
        self.assertIn("qintopia_public_case_search", ctx.names)
        self.assertIn("qintopia_customer_context_lookup", ctx.names)
        self.assertIn("qintopia_lead_capture", ctx.names)
        self.assertIn("qintopia_proposal_outline_generate", ctx.names)
        self.assertIn("qintopia_demo_script_generate", ctx.names)
        self.assertIn("qintopia_external_disclosure_filter", ctx.names)
        self.assertIn("qintopia_conversation_summary", ctx.names)
        self.assertIn("qintopia_xiaoman_activity_promotion_details_update", ctx.names)
        self.assertIn("qintopia_xiaoman_poster_production_request", ctx.names)
        self.assertIn("qintopia_xiaoman_poster_workflow_status", ctx.names)
        self.assertIs(
            ctx.hooks["pre_gateway_dispatch"],
            self.module.handle_xiaoman_gateway_dispatch,
        )
        self.assertNotIn("qintopia_dify_dataset_list", ctx.names)
        self.assertNotIn("qintopia_dify_knowledge_retrieve", ctx.names)

    def test_register_exposes_raw_dify_only_for_wenyuange_opt_in(self):
        class FakeCtx:
            def __init__(self) -> None:
                self.names = []
                self.hooks = {}

            def register_tool(self, **kwargs) -> None:
                self.names.append(kwargs["name"])

            def register_hook(self, name, callback) -> None:
                self.hooks[name] = callback

        os.environ["QINTOPIA_PROFILE_ID"] = "wenyuange"
        os.environ["QINTOPIA_DIFY_RAW_TOOLS_ENABLE"] = "1"
        ctx = FakeCtx()
        self.module.register(ctx)

        self.assertIn("qintopia_wenyuange_lookup", ctx.names)
        self.assertIn("qintopia_dify_dataset_list", ctx.names)
        self.assertIn("qintopia_dify_dataset_get", ctx.names)
        self.assertIn("qintopia_dify_knowledge_retrieve", ctx.names)
        self.assertIn("qintopia_dify_document_list", ctx.names)
        self.assertIn("qintopia_dify_document_get", ctx.names)
        self.assertIn("qintopia_dify_indexing_status_get", ctx.names)
        self.assertIn("qintopia_dify_segment_list", ctx.names)
        self.assertIn("qintopia_dify_segment_get", ctx.names)

    def test_poster_request_uses_trusted_session_and_returns_accepted_status(self):
        self.trust_poster_session()
        captured = {}

        def fake_intake(payload):
            captured.update(payload)
            return {
                "success": True,
                "accepted": True,
                "deduped": False,
                "workflow_root_id": "11111111-1111-4111-8111-111111111111",
                "visual_work_item_id": "22222222-2222-4222-8222-222222222222",
                "workflow_status": "queued",
            }

        self.module._poster_intake_call = fake_intake
        response = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {
                    "request": "请为周末晚餐生成海报，时间周六18:00，地点秦托邦会客厅",
                    "activity_facts": {
                        "source": "originating_request",
                        "title": "周末晚餐",
                        "schedule": "周六18:00",
                        "location": "秦托邦会客厅",
                    },
                }
            )
        )

        self.assertTrue(response["accepted"])
        self.assertEqual(response["user_status"], "已接单")
        self.assertEqual(captured["session"]["conversation_id"], "oc_chat_fixture")
        self.assertEqual(captured["session"]["requester_user_id"], "ou_user_fixture")
        self.assertNotIn("conversation_type", captured["session"])
        self.assertEqual(captured["schema_version"], 3)
        self.assertTrue(captured["idempotency_key"].startswith("poster_production_request:sha256:"))
        expected_digest = hashlib.sha256()
        for part in (
            "poster-intake-v3",
            "feishu",
            "om_message_fixture",
            "poster_production_request",
        ):
            expected_digest.update(part.encode("utf-8"))
            expected_digest.update(b"\0")
        self.assertEqual(
            captured["idempotency_key"],
            "poster_production_request:sha256:" + expected_digest.hexdigest(),
        )
        self.assertEqual(captured["activity_facts"]["title"], "周末晚餐")

    def test_poster_request_v3_group_never_supplies_policy_target_or_reviewer(self):
        self.trust_poster_session(
            chat_id="oc_internal_fixture",
            user_id="ou_group_member_fixture",
            message_id="om_group_request_fixture",
            conversation_type="group",
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"] = "1"
        captured = {}

        def fake_intake(payload):
            captured.update(payload)
            return {
                "success": True,
                "accepted": True,
                "deduped": False,
                "conversation_type": "group",
                "workflow_root_id": "11111111-1111-4111-8111-111111111111",
                "visual_work_item_id": "22222222-2222-4222-8222-222222222222",
                "user_status": "已接单",
            }

        self.module._poster_intake_call = fake_intake
        response = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {
                    "request": "@小满 请生成内部活动海报，时间 8 月 1 日 16:00，地点线上",
                    "activity_facts": {
                        "source": "originating_request",
                        "title": "内部活动",
                        "schedule": "8 月 1 日 16:00",
                        "location": "线上",
                    },
                }
            )
        )

        self.assertTrue(response["accepted"])
        self.assertEqual(captured["schema_version"], 3)
        self.assertNotIn("conversation_type", captured["session"])
        self.assertNotIn("target_chat_id", captured)
        self.assertNotIn("reviewer_id", captured)
        self.assertNotIn("provider", captured)

    def test_poster_request_keeps_v2_direct_compatibility_until_ingress_cutover(self):
        self.trust_poster_session(ingress_enabled=False)
        captured = {}

        def fake_intake(payload):
            captured.update(payload)
            return {"success": True, "accepted": True, "user_status": "已接单"}

        self.module._poster_intake_call = fake_intake
        response = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {"request": "请生成一张活动海报"}
            )
        )

        self.assertTrue(response["accepted"])
        self.assertEqual(captured["schema_version"], 2)
        self.assertEqual(captured["session"]["conversation_type"], "direct")

    def test_invalid_ingress_mode_does_not_downgrade_to_v2(self):
        self.trust_poster_session()
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = "invalid"
        self.module._poster_intake_call = lambda _payload: self.fail(
            "invalid ingress mode must fail before intake"
        )

        response = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {"request": "请生成一张活动海报"}
            )
        )

        self.assertFalse(response["accepted"])
        self.assertEqual(response["error"], "trusted_feishu_session_required")

    def test_poster_review_hook_forwards_verified_card_without_model_dispatch(self):
        os.environ["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"] = "fixture-callback-key"
        captured = {}

        def fake_callback(envelope):
            captured.update(envelope)
            return {
                "success": True,
                "action_status": "review_recorded",
                "decision": "approved",
                "notification_id": "11111111-1111-4111-8111-111111111111",
                "artifact_id": "22222222-2222-4222-8222-222222222222",
                "deduped": False,
                "group_send_authorized": False,
                "external_send_executed": False,
            }

        self.module._poster_review_callback_call = fake_callback
        result = self.module.handle_xiaoman_poster_review_card(self.poster_review_event())

        self.assertEqual(
            result,
            {"action": "skip", "reason": "xiaoman_poster_review_forwarded"},
        )
        body = base64.b64decode(captured["body_base64"])
        payload = json.loads(body)
        self.assertEqual(payload["header"]["event_id"], "evt_fixture_1")
        self.assertEqual(payload["event"]["operator"]["open_id"], "ou_user_fixture")
        self.assertEqual(
            payload["event"]["action"]["value"]["callback_kind"],
            "xiaoman_poster_review",
        )
        digest = hashlib.sha256()
        digest.update(captured["timestamp"].encode("ascii"))
        digest.update(captured["nonce"].encode("ascii"))
        digest.update(b"fixture-callback-key")
        digest.update(body)
        self.assertEqual(captured["signature"], digest.hexdigest())

    def test_poster_review_hook_uses_framed_unix_socket_contract(self):
        os.environ["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"] = (
            "fixture-callback-key"
        )
        socket_root = tempfile.TemporaryDirectory(prefix="poster-review-", dir="/tmp")
        socket_path = str(Path(socket_root.name) / "callback.sock")
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(listener.close)
        self.addCleanup(socket_root.cleanup)
        listener.bind(socket_path)
        listener.listen(1)
        captured = {}

        def serve_callback() -> None:
            connection, _ = listener.accept()
            with connection:
                chunks = []
                while True:
                    chunk = connection.recv(8192)
                    if not chunk:
                        break
                    chunks.append(chunk)
                raw = b"".join(chunks)
                captured.update(json.loads(raw.split(b"\n", 1)[0]))
                connection.sendall(
                    (
                        json.dumps(
                            {
                                "success": True,
                                "action_status": "review_recorded",
                                "decision": "approved",
                                "notification_id": "11111111-1111-4111-8111-111111111111",
                                "artifact_id": "22222222-2222-4222-8222-222222222222",
                                "deduped": False,
                                "group_send_authorized": False,
                                "external_send_executed": False,
                            },
                            separators=(",", ":"),
                        )
                        + "\n"
                    ).encode("utf-8")
                )

        server = threading.Thread(target=serve_callback, daemon=True)
        server.start()
        original_socket = self.module.XIAOMAN_POSTER_REVIEW_CALLBACK_SOCKET
        self.module.XIAOMAN_POSTER_REVIEW_CALLBACK_SOCKET = socket_path
        try:
            result = self.module.handle_xiaoman_poster_review_card(
                self.poster_review_event()
            )
            server.join(timeout=2)
        finally:
            self.module.XIAOMAN_POSTER_REVIEW_CALLBACK_SOCKET = original_socket
            listener.close()
            socket_root.cleanup()

        self.assertFalse(server.is_alive(), "callback socket server did not complete")
        self.assertEqual(
            result,
            {"action": "skip", "reason": "xiaoman_poster_review_forwarded"},
        )
        self.assertEqual(
            set(captured), {"timestamp", "nonce", "signature", "body_base64"}
        )
        body = json.loads(base64.b64decode(captured["body_base64"]))
        self.assertEqual(body["event"]["context"]["open_chat_id"], "oc_chat_fixture")
        self.assertEqual(body["event"]["operator"]["open_id"], "ou_user_fixture")

    def test_poster_review_envelope_matches_rust_contract_fixture(self):
        fixture_path = (
            Path(__file__).resolve().parent
            / "fixtures"
            / "poster-review-callback-envelope.json"
        )
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        payload = json.loads(base64.b64decode(fixture["envelope"]["body_base64"]))
        original_time = self.module.time.time
        original_token_hex = self.module.secrets.token_hex
        self.module.time.time = lambda: fixture["now"]
        self.module.secrets.token_hex = lambda _length: fixture["envelope"]["nonce"]
        try:
            envelope = self.module._poster_review_callback_envelope(
                payload, fixture["callback_key"]
            )
        finally:
            self.module.time.time = original_time
            self.module.secrets.token_hex = original_token_hex

        self.assertEqual(envelope, fixture["envelope"])

    def test_poster_review_log_filter_redacts_hermes_card_identifiers(self):
        privacy_filter = self.module._PosterReviewLogPrivacyFilter()
        records = [
            logging.LogRecord(
                "gateway.platforms.feishu",
                logging.INFO,
                __file__,
                1,
                "[Feishu] Routing card action %r from %s in %s as synthetic command",
                ("button", "ou_sensitive_user", "oc_sensitive_chat"),
                None,
            ),
            logging.LogRecord(
                "gateway.run",
                logging.INFO,
                __file__,
                1,
                "pre_gateway_dispatch skip: reason=%s platform=%s chat=%s",
                ("xiaoman_poster_review_forwarded", "feishu", "oc_sensitive_chat"),
                None,
            ),
            logging.LogRecord(
                "gateway.platforms.feishu",
                logging.DEBUG,
                __file__,
                1,
                "[Feishu] Dropping duplicate card action token: %s",
                ("sensitive-card-token",),
                None,
            ),
        ]

        for record in records:
            self.assertTrue(privacy_filter.filter(record))
            rendered = record.getMessage()
            self.assertNotIn("ou_sensitive_user", rendered)
            self.assertNotIn("oc_sensitive_chat", rendered)
            self.assertNotIn("sensitive-card-token", rendered)

    def test_poster_review_log_filter_installation_is_idempotent(self):
        targets = [
            logging.getLogger("gateway.platforms.feishu"),
            logging.getLogger("gateway.run"),
        ]
        self.module._install_poster_review_log_privacy_filter()
        self.module._install_poster_review_log_privacy_filter()
        for target in targets:
            matching = [
                item
                for item in target.filters
                if getattr(item, "_marker", "")
                == self.module._PosterReviewLogPrivacyFilter._marker
            ]
            self.assertEqual(len(matching), 1)

    def test_poster_review_hook_does_not_capture_plain_card_command(self):
        event = SimpleNamespace(
            text=(
                '/card button {"callback_kind":"xiaoman_poster_review",'
                '"action":"approve"}'
            ),
            message_type=SimpleNamespace(value="command"),
            source=SimpleNamespace(
                platform=SimpleNamespace(value="feishu"),
                chat_id="oc_chat_fixture",
                user_id="ou_user_fixture",
                chat_type="dm",
            ),
            raw_message=SimpleNamespace(event=SimpleNamespace(action=None)),
            message_id="om_message_fixture",
        )

        self.assertIsNone(self.module.handle_xiaoman_poster_review_card(event))

    def test_poster_review_hook_rejects_forged_card_binding_before_socket(self):
        self.module._poster_review_callback_call = lambda _envelope: self.fail(
            "forged callback must not reach socket"
        )

        wrong_user = self.module.handle_xiaoman_poster_review_card(
            self.poster_review_event(source_user_id="ou_attacker")
        )
        wrong_chat = self.module.handle_xiaoman_poster_review_card(
            self.poster_review_event(callback_chat_id="oc_other")
        )
        expected = {"action": "skip", "reason": "xiaoman_poster_review_rejected"}
        self.assertEqual(wrong_user, expected)
        self.assertEqual(wrong_chat, expected)

    def test_poster_review_hook_disabled_fails_closed_without_socket(self):
        self.module._poster_review_callback_call = lambda _envelope: self.fail(
            "disabled callback must not reach socket"
        )

        result = self.module.handle_xiaoman_poster_review_card(self.poster_review_event())

        self.assertEqual(
            result,
            {"action": "skip", "reason": "xiaoman_poster_review_disabled"},
        )

    def test_poster_review_hook_rejects_unbound_sidecar_response(self):
        os.environ["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"] = "fixture-callback-key"
        self.module._poster_review_callback_call = lambda _envelope: {
            "success": True,
            "action_status": "review_recorded",
            "decision": "approved",
            "notification_id": "11111111-1111-4111-8111-111111111111",
            "artifact_id": "33333333-3333-4333-8333-333333333333",
            "group_send_authorized": False,
            "external_send_executed": False,
        }

        result = self.module.handle_xiaoman_poster_review_card(self.poster_review_event())

        self.assertEqual(
            result,
            {"action": "skip", "reason": "xiaoman_poster_review_unavailable"},
        )

    def test_poster_review_hook_rejects_wrong_sidecar_decision(self):
        os.environ["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"] = (
            "fixture-callback-key"
        )
        self.module._poster_review_callback_call = lambda _envelope: {
            "success": True,
            "action_status": "review_recorded",
            "decision": "rejected",
            "notification_id": "11111111-1111-4111-8111-111111111111",
            "artifact_id": "22222222-2222-4222-8222-222222222222",
            "deduped": False,
            "group_send_authorized": False,
            "external_send_executed": False,
        }

        result = self.module.handle_xiaoman_poster_review_card(
            self.poster_review_event()
        )

        self.assertEqual(
            result,
            {"action": "skip", "reason": "xiaoman_poster_review_unavailable"},
        )

    def test_poster_request_without_activity_facts_still_uses_async_intake(self):
        self.trust_poster_session()
        captured = {}

        def fake_intake(payload):
            captured.update(payload)
            return {"success": True, "accepted": True, "user_status": "需补充"}

        self.module._poster_intake_call = fake_intake
        response = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {"request": "生成海报"}
            )
        )
        self.assertEqual(response["user_status"], "需补充")
        self.assertEqual(captured["activity_facts"], {})

    def test_poster_request_rejects_forged_process_environment_without_intake(self):
        called = False

        def fake_intake(_payload):
            nonlocal called
            called = True
            return {}

        self.module._poster_intake_call = fake_intake
        os.environ["HERMES_SESSION_PLATFORM"] = "feishu"
        os.environ["HERMES_SESSION_CONVERSATION_TYPE"] = "direct"
        os.environ["HERMES_SESSION_CHAT_ID"] = "oc_forged_fixture"
        os.environ["HERMES_SESSION_USER_ID"] = "ou_user_fixture"
        os.environ["HERMES_SESSION_MESSAGE_ID"] = "om_message_fixture"
        rejected = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {"request": "生成海报"}
            )
        )

        self.assertEqual(rejected["error"], "trusted_feishu_session_required")
        self.assertFalse(called)

    def test_poster_revision_uses_same_trusted_intake_without_target_arguments(self):
        self.trust_poster_session(message_id="om_revision_fixture")
        captured = {}

        def fake_intake(payload):
            captured.update(payload)
            return {"success": True, "accepted": True, "workflow_status": "生成中"}

        self.module._poster_intake_call = fake_intake
        response = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {
                    "request": "标题再大一点，其他活动事实保持不变",
                    "workflow_root_id": "11111111-1111-4111-8111-111111111111",
                    "revision_of_artifact_id": "22222222-2222-4222-8222-222222222222",
                }
            )
        )
        self.assertTrue(response["accepted"])
        self.assertEqual(captured["operation"], "poster_revision_request")
        self.assertNotIn("conversation_id", captured)
        self.assertNotIn("reviewer_id", captured)

    def test_feishu_direct_sdk_message_is_signed_before_model_dispatch(self):
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"] = (
            "fixture-ingress-key-with-at-least-32-bytes"
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"] = (
            "ou_xiaoman_bot_fixture"
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"] = "0"
        captured = {}

        def fake_ingress(envelope):
            captured.update(envelope)
            return {
                "success": True,
                "accepted": True,
                "deduped": False,
                "external_send_executed": False,
                "group_send_authorized": False,
            }

        self.module._feishu_message_ingress_call = fake_ingress
        event = self.feishu_message_event()
        self.assertIsNone(getattr(event.source, "message_id", None))
        result = self.module.handle_xiaoman_gateway_dispatch(event)

        self.assertIsNone(result)
        self.assertEqual(event.source.message_id, event.message_id)
        body = base64.b64decode(captured["body_base64"])
        payload = json.loads(body)
        self.assertEqual(payload["operation"], "feishu_message_ingest")
        self.assertEqual(payload["schema_version"], 3)
        self.assertEqual(payload["message"]["chat_type"], "direct")
        self.assertEqual(payload["message"]["sender_type"], "user")
        self.assertNotIn("raw_message", payload)
        signed = (
            self.module.XIAOMAN_FEISHU_MESSAGE_INGRESS_SIGNATURE_DOMAIN
            + captured["timestamp"].encode("ascii")
            + b"\n"
            + captured["nonce"].encode("ascii")
            + b"\n"
            + body
        )
        expected = hmac.new(
            os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"].encode(),
            signed,
            hashlib.sha256,
        ).hexdigest()
        self.assertEqual(captured["signature"], expected)

    def test_feishu_ingress_envelope_matches_shared_rust_fixture(self):
        fixture_path = (
            Path(__file__).resolve().parents[5]
            / "fixtures"
            / "feishu"
            / "xiaoman-message-ingress-envelope-v3.json"
        )
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        payload = json.loads(base64.b64decode(fixture["envelope"]["body_base64"]))
        original_time = self.module.time.time
        original_token_hex = self.module.secrets.token_hex
        self.module.time.time = lambda: fixture["now"]
        self.module.secrets.token_hex = lambda _length: fixture["envelope"]["nonce"]
        try:
            envelope = self.module._feishu_message_ingress_envelope(
                payload, fixture["hmac_key"]
            )
        finally:
            self.module.time.time = original_time
            self.module.secrets.token_hex = original_token_hex

        self.assertEqual(envelope, fixture["envelope"])

    def test_feishu_group_ingress_requires_exact_mention_human_and_feature(self):
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"] = (
            "fixture-ingress-key-with-at-least-32-bytes"
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"] = (
            "ou_xiaoman_bot_fixture"
        )
        calls = []
        self.module._feishu_message_ingress_call = lambda envelope: (
            calls.append(envelope)
            or {
                "success": True,
                "accepted": True,
                "external_send_executed": False,
                "group_send_authorized": False,
            }
        )

        os.environ["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"] = "0"
        self.module.handle_xiaoman_gateway_dispatch(
            self.feishu_message_event(
                chat_id="oc_group_fixture", chat_type="group", mention_bot=True
            )
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"] = "1"
        self.module.handle_xiaoman_gateway_dispatch(
            self.feishu_message_event(
                chat_id="oc_group_fixture", chat_type="group", mention_bot=False
            )
        )
        self.module.handle_xiaoman_gateway_dispatch(
            self.feishu_message_event(
                chat_id="oc_group_fixture",
                chat_type="group",
                mention_bot=True,
                sender_type="bot",
            )
        )
        self.module.handle_xiaoman_gateway_dispatch(
            self.feishu_message_event(
                chat_id="oc_group_fixture", chat_type="group", mention_bot=True
            )
        )

        self.assertEqual(len(calls), 1)
        payload = json.loads(base64.b64decode(calls[0]["body_base64"]))
        self.assertEqual(payload["message"]["chat_type"], "group")
        self.assertTrue(payload["message"]["is_mention_bot"])
        self.assertEqual(
            payload["message"]["thread_root_message_id"], "om_message_fixture"
        )
        self.assertTrue(
            payload["message"]["mentioned_bot_ref"].startswith("sha256:")
        )

    def test_feishu_ingress_rejects_malformed_sdk_binding_without_socket(self):
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"] = (
            "fixture-ingress-key-with-at-least-32-bytes"
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"] = (
            "ou_xiaoman_bot_fixture"
        )
        self.module._feishu_message_ingress_call = lambda _envelope: self.fail(
            "malformed SDK binding must not reach the socket"
        )

        result = self.module.handle_xiaoman_gateway_dispatch(
            self.feishu_message_event(source_chat_id="oc_other_fixture")
        )
        self.assertIsNone(result)
        result = self.module.handle_xiaoman_gateway_dispatch(
            self.feishu_message_event(source_chat_type="group")
        )
        self.assertIsNone(result)

    def test_feishu_ingress_identifier_matches_sidecar_ascii_contract(self):
        self.assertEqual(
            self.module._feishu_ingress_identifier("om_ascii-1:fixture.test"),
            "om_ascii-1:fixture.test",
        )
        self.assertEqual(self.module._feishu_ingress_identifier("消息一"), "")

    def test_feishu_ingress_unavailable_does_not_skip_model_dispatch(self):
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"] = (
            "fixture-ingress-key-with-at-least-32-bytes"
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"] = (
            "ou_xiaoman_bot_fixture"
        )
        self.module._feishu_message_ingress_call = lambda _envelope: (_ for _ in ()).throw(
            OSError("fixture socket unavailable")
        )

        event = self.feishu_message_event()
        self.assertIsNone(self.module.handle_xiaoman_gateway_dispatch(event))
        self.assertIsNone(getattr(event.source, "message_id", None))

    def test_feishu_ingress_success_supplies_real_hermes_poster_session(self):
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = "1"
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"] = (
            "fixture-ingress-key-with-at-least-32-bytes"
        )
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"] = "0"
        event = self.feishu_message_event()
        self.module._feishu_message_ingress_call = lambda _envelope: {
            "success": True,
            "accepted": True,
            "external_send_executed": False,
            "group_send_authorized": False,
        }
        self.module._poster_intake_call = lambda payload: {
            "success": True,
            "accepted": payload["session"]["source_message_id"]
            == event.message_id,
            "user_status": "已接单",
        }

        self.module.handle_xiaoman_gateway_dispatch(event)
        session_values = {
            "HERMES_SESSION_PLATFORM": "feishu",
            "HERMES_SESSION_CHAT_ID": event.source.chat_id,
            "HERMES_SESSION_USER_ID": event.source.user_id,
            "HERMES_SESSION_MESSAGE_ID": event.source.message_id,
        }
        self.module._strict_poster_session_value = lambda name: session_values.get(
            name, ""
        )
        response = json.loads(
            self.module.handle_qintopia_xiaoman_poster_production_request(
                {"request": "请生成海报，活动时间 8 月 1 日 16:00，地点线上"}
            )
        )

        self.assertTrue(response["accepted"])
        self.assertEqual(response["user_status"], "已接单")

    def test_feishu_ingress_parser_failure_does_not_skip_model_dispatch(self):
        os.environ["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] = "1"
        self.module._feishu_ingress_message_payload = lambda _event: (_ for _ in ()).throw(
            RuntimeError("fixture parser failure")
        )

        self.assertIsNone(
            self.module.handle_xiaoman_gateway_dispatch(self.feishu_message_event())
        )

    def test_xiaoman_activity_announcement_prepare_weekly_minimum_loop_drafts(self):
        self.enable_xiaoman_activity_wrappers()

        recruit = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    "date": "2026-W32",
                    "mode": "weekly_recruitment_form",
                    "operator_name": "小乔",
                    "community_audience": "居民群",
                    "form_label": "活动招募表单",
                }
            )
        )
        confirmation = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    "date": "2026-W32",
                    "mode": "weekly_plan_confirmation",
                    "operator_name": "小乔",
                    "community_audience": "营造司群",
                    "confirmation_owner_name": "张百忍",
                    "plan_sheet_label": (
                        "https://ranuox3qst4.feishu.cn/wiki/VvAmwU3zCikSa2kVgK7c8JySnle"
                        "?table=tblIRfkR6vmwiynbu&view=vew5C6T2FF"
                    ),
                }
            )
        )
        preview = json.loads(
            self.module.handle_qintopia_xiaoman_activity_announcement_prepare(
                {
                    "date": "2026-W32",
                    "mode": "weekly_preview",
                    "operator_name": "小乔",
                    "community_audience": "居民群",
                    "records": [
                        {
                            "table_role": "activity_plan",
                            "record_ref": "activity_plan:abc123def456",
                            "title": "周末共创晚餐",
                            "activity_date": "2026-08-08",
                            "start_time": "18:30",
                            "location": "秦托邦一楼",
                            "owner_name": "张百忍",
                            "promotion_status": "已确认排期",
                        }
                    ],
                }
            )
        )

        self.assertTrue(recruit["success"])
        self.assertEqual(recruit["workflow_step"], "weekly_recruitment_form")
        self.assertEqual(recruit["record_source"], "not_required")
        self.assertIn("活动招募表单", recruit["announcement_text"])
        self.assertFalse(recruit["external_send_executed"])

        rendered_confirmation = json.dumps(confirmation, ensure_ascii=False)
        self.assertTrue(confirmation["success"])
        self.assertEqual(confirmation["workflow_step"], "weekly_plan_confirmation")
        self.assertEqual(confirmation["record_source"], "not_required")
        self.assertEqual(confirmation["mentions"], ["张百忍"])
        self.assertIn("@张百忍", confirmation["operator_review_message"])
        self.assertIn("周日 20:00", confirmation["operator_review_message"])
        self.assertIn("下周活动计划表", confirmation["announcement_text"])
        self.assertNotIn("https://", rendered_confirmation)
        self.assertNotIn("tblIRfkR6vmwiynbu", rendered_confirmation)
        self.assertNotIn("vew5C6T2FF", rendered_confirmation)
        self.assertFalse(confirmation["external_send_executed"])

        self.assertTrue(preview["success"])
        self.assertEqual(preview["workflow_step"], "weekly_preview")
        self.assertEqual(preview["publishable_count"], 1)
        self.assertIn("下周活动预告", preview["announcement_text"])
        self.assertIn("周末共创晚餐", preview["announcement_text"])
        self.assertTrue(preview["requires_human_confirmation"])
        self.assertFalse(preview["external_send_executed"])
        self.assertNotIn("record_ref", json.dumps(preview, ensure_ascii=False))


if __name__ == "__main__":
    unittest.main()
