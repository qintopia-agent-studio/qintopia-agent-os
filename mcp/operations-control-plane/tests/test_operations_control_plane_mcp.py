from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path


BIN = Path(__file__).resolve().parents[1] / "bin" / "qintopia-operations-control-plane-mcp"


def run_tool(tool: str, args: dict) -> tuple[int, dict]:
    result = subprocess.run(
        [sys.executable, str(BIN), "--tool", tool, "--args-json", json.dumps(args, ensure_ascii=False)],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.returncode, json.loads(result.stdout)


class OperationsControlPlaneMcpTest(unittest.TestCase):
    def test_workflow_start_activity_promotion_dry_run(self):
        code, payload = run_tool(
            "qintopia_operations_workflow_start",
            {
                "workflow_type": "activity_promotion",
                "requester_agent": "xiaoman",
                "source_type": "xiaoman_activity",
                "source_refs": {"source_record_ref": "activity_plan:demo"},
            },
        )

        self.assertEqual(code, 0)
        self.assertTrue(payload["success"])
        self.assertTrue(payload["dry_run"])
        self.assertEqual(payload["workflow_root"]["work_item_type"], "activity_promotion_request")
        self.assertEqual([item["work_item_type"] for item in payload["initial_work_items"]], ["evidence_request", "visual_asset_request"])
        self.assertFalse(payload["workflow_root"]["database_write_executed"])

    def test_work_item_create_rejects_apply(self):
        code, payload = run_tool(
            "qintopia_operations_work_item_create",
            {
                "apply": True,
                "work_item_type": "visual_asset_request",
                "capability_key": "huabaosi.create_visual_asset",
                "requester_agent": "xiaoman",
                "target_agent": "huabaosi",
                "source_type": "xiaoman_activity",
                "source_refs": {"source_record_ref": "activity_plan:demo"},
            },
        )

        self.assertEqual(code, 1)
        self.assertFalse(payload["success"])
        self.assertEqual(payload["error"], "apply mode is not available on the dry-run MCP wrapper")

    def test_group_message_request_starts_awaiting_publish(self):
        code, payload = run_tool(
            "qintopia_operations_work_item_create",
            {
                "work_item_type": "group_message_request",
                "capability_key": "erhua.send_group_message",
                "requester_agent": "xiaoman",
                "target_agent": "erhua",
                "source_type": "operations_workflow",
                "source_refs": {"source_record_ref": "artifact:text_announcement:demo"},
            },
        )

        self.assertEqual(code, 0)
        self.assertEqual(payload["work_item"]["status"], "awaiting_publish")
        self.assertFalse(payload["work_item"]["external_send_executed"])

    def test_status_is_read_only_preview(self):
        code, payload = run_tool(
            "qintopia_operations_status",
            {"workflow_root_id": "activity_promotion_request:demo", "include_events": True, "max_events": 500},
        )

        self.assertEqual(code, 0)
        self.assertTrue(payload["action"]["requires_sidecar_status_reader"])
        self.assertEqual(payload["status_lookup"]["max_events"], 100)

    def test_status_rejects_invalid_max_events(self):
        for max_events in ("many", {"limit": 20}):
            with self.subTest(max_events=max_events):
                code, payload = run_tool(
                    "qintopia_operations_status",
                    {"workflow_root_id": "activity_promotion_request:demo", "max_events": max_events},
                )

                self.assertEqual(code, 1)
                self.assertFalse(payload["success"])
                self.assertEqual(payload["error"], "max_events must be an integer between 1 and 100")


if __name__ == "__main__":
    unittest.main()
