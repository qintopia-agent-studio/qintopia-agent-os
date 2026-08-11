from __future__ import annotations

import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path


def load_plugin():
    plugin_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("xiaoman_activity_plugin", plugin_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Ctx:
    def __init__(self):
        self.tools = {}

    def register_tool(self, **kwargs):
        self.tools[kwargs["name"]] = kwargs


class XiaomanActivitySkillTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmpdir = tempfile.TemporaryDirectory()
        self.old_env = {
            name: os.environ.get(name)
            for name in [
                "QINTOPIA_PROFILE_ID",
                "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
                "QINTOPIA_SIDECAR_BIN",
            ]
        }
        os.environ["QINTOPIA_PROFILE_ID"] = "xiaoman"
        os.environ["QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"] = "1"
        os.environ["QINTOPIA_SIDECAR_BIN"] = str(Path(self.tmpdir.name) / "sidecar")
        self.module = load_plugin()
        self.legacy = self.module._legacy_plugin()
        self.legacy._xiaoman_activity_validate_read_through_worker = lambda worker_bin: Path(worker_bin)

    def tearDown(self) -> None:
        for name, value in self.old_env.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        self.tmpdir.cleanup()

    def test_registers_xiaoman_activity_tools(self):
        ctx = Ctx()
        self.module.register(ctx)

        self.assertEqual(set(self.module.TOOL_NAMES), set(ctx.tools))
        announcement = ctx.tools["qintopia_xiaoman_activity_announcement_prepare"]
        self.assertEqual(
            announcement["schema"]["description"],
            self.legacy.QINTOPIA_XIAOMAN_ACTIVITY_ANNOUNCEMENT_PREPARE_SCHEMA["description"],
        )
        self.assertTrue(callable(announcement["handler"]))

    def test_announcement_handler_preserves_legacy_behavior(self):
        ctx = Ctx()
        self.module.register(ctx)
        handler = ctx.tools["qintopia_xiaoman_activity_announcement_prepare"]["handler"]

        report = json.loads(
            handler(
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
                            "location": "秦托邦工坊",
                            "owner_name": "阿成",
                            "promotion_status": "待确认",
                        }
                    ],
                }
            )
        )

        self.assertTrue(report["success"])
        self.assertIn("付费木作体验课", report["announcement_text"])
        self.assertFalse(report["external_send_executed"])
