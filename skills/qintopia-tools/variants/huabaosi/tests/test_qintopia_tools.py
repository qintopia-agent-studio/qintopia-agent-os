from __future__ import annotations

import importlib.util
import os
import unittest
from pathlib import Path


def load_plugin():
    plugin_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("qintopia_tools_huabaosi", plugin_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class HuabaosiCompletionPolicyTest(unittest.TestCase):
    def setUp(self) -> None:
        self.module = load_plugin()
        self.previous_profile = os.environ.get("HERMES_PROFILE")
        os.environ["HERMES_PROFILE"] = "huabaosi"

    def tearDown(self) -> None:
        if self.previous_profile is None:
            os.environ.pop("HERMES_PROFILE", None)
        else:
            os.environ["HERMES_PROFILE"] = self.previous_profile

    def call(self, args):
        return self.module._on_pre_tool_call(tool_name="kanban_complete", args=args)

    def test_blocks_missing_output_record(self):
        result = self.call({"summary": "设计已完成", "metadata": {}})
        self.assertEqual(result["action"], "block")
        self.assertIn("record_id", result["message"])

    def test_allows_output_record_without_artifacts(self):
        self.assertIsNone(self.call({"summary": "已写入 recABCDEFGH"}))

    def test_blocks_artifacts_without_finished_image_status(self):
        result = self.call(
            {"summary": "已写入 recABCDEFGH", "metadata": {"artifacts": ["poster.png"]}}
        )
        self.assertEqual(result["action"], "block")
        self.assertIn("成品图", result["message"])

    def test_allows_artifacts_with_finished_image_status(self):
        self.assertIsNone(
            self.call(
                {
                    "result": "产出库 recABCDEFGH",
                    "metadata": {"artifacts": ["poster.png"], "成品图": "已上传"},
                }
            )
        )

    def test_is_inert_for_other_profile_or_tool(self):
        os.environ["HERMES_PROFILE"] = "default"
        self.assertIsNone(self.call({}))
        os.environ["HERMES_PROFILE"] = "huabaosi"
        self.assertIsNone(self.module._on_pre_tool_call(tool_name="kanban_block", args={}))


if __name__ == "__main__":
    unittest.main()
