import importlib.util
import json
import os
import sys
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).resolve().parents[1] / "__init__.py"
SPEC = importlib.util.spec_from_file_location("feishu_base_skill", MODULE_PATH)
feishu_base = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = feishu_base
SPEC.loader.exec_module(feishu_base)


class FeishuBaseSkillTest(unittest.TestCase):
    def test_invalid_record_id_returns_structured_error(self) -> None:
        payload = json.loads(
            feishu_base.handle_qintopia_xiaoman_activity_record_get(
                {"record_id": "../../etc/passwd"}
            )
        )

        self.assertFalse(payload["success"])
        self.assertEqual(payload["error"], "valid record_id is required")

    def test_missing_env_does_not_raise_traceback(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            payload = json.loads(
                feishu_base.handle_qintopia_huabaosi_design_record_get(
                    {"record_id": "recAbc123", "purpose": "verify"}
                )
            )

        self.assertFalse(payload["success"])
        self.assertEqual(payload["skill"], "qintopia_huabaosi_design_record_get")
        self.assertIn("QINTOPIA_BASE_READ_HUABAOSI_DESIGN_BASE_TOKEN", payload["error"])
        self.assertIn("fallback_rule", payload)
        self.assertIn("required_env", payload)

    def test_success_payload_does_not_echo_base_identifiers(self) -> None:
        raw = {
            "data": {
                "record": {
                    "fields": {
                        "任务标题": "端午活动海报",
                        "任务编号": "HB-001",
                        "任务状态": "完成待审核",
                        "成品图": [{"name": "poster.png", "file_token": "file_token", "size": 12}],
                    }
                }
            }
        }

        with mock.patch.object(
            feishu_base,
            "_base_config",
            return_value=("base-token-from-env", "table-id-from-env"),
        ), mock.patch.object(feishu_base, "_feishu_base_record_get", return_value=raw):
            payload = json.loads(
                feishu_base.handle_qintopia_huabaosi_design_record_get(
                    {"record_id": "recAbc123", "purpose": "verify"}
                )
            )

        self.assertTrue(payload["success"])
        self.assertNotIn("base_token", payload)
        self.assertNotIn("table_id", payload)
        self.assertEqual(
            payload["source"]["base_token_env"],
            "QINTOPIA_BASE_READ_HUABAOSI_DESIGN_BASE_TOKEN",
        )
        self.assertEqual(payload["facts"]["任务状态"], "完成待审核")

    def test_registers_expected_tools(self) -> None:
        class Ctx:
            def __init__(self) -> None:
                self.tools = []

            def register_tool(self, **kwargs):
                self.tools.append(kwargs)

        ctx = Ctx()
        feishu_base.register(ctx)

        self.assertEqual(
            [tool["name"] for tool in ctx.tools],
            [
                "qintopia_xiaoman_activity_record_get",
                "qintopia_huabaosi_design_record_get",
            ],
        )
        self.assertTrue(all(tool["toolset"] == "qintopia" for tool in ctx.tools))


if __name__ == "__main__":
    unittest.main()
