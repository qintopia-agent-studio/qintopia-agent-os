from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import unittest
from pathlib import Path


WORKFLOW_DIR = Path(__file__).resolve().parents[1]
SCRIPT = WORKFLOW_DIR / "morning_brief.py"
FIXTURES = Path(__file__).resolve().parent / "fixtures"
APPROVED_ARTIFACT_ID = "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5"


def load_module():
    spec = importlib.util.spec_from_file_location("erhua_morning_brief", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class ErhuaMorningBriefTests(unittest.TestCase):
    def run_script(self, *extra: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--date",
                "2026-08-08",
                "--activity-fixture",
                str(FIXTURES / "activity-empty.json"),
                "--news-fixture",
                str(FIXTURES / "qunmind-ai-report.md"),
                *extra,
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_empty_activity_recommends_starting_one_and_extracts_ai_news(self):
        result = self.run_script("--json")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("今天暂时没有已确认活动", result.stdout)
        self.assertIn("发起一个小活动", result.stdout)
        self.assertIn("OpenAI 发布新的 Agent 编排实践", result.stdout)
        self.assertIn("Anthropic 更新企业安全评估", result.stdout)
        self.assertNotIn("https://example.test", result.stdout)
        self.assertNotIn("链上治理工具更新", result.stdout)
        self.assertIn('"sunday_no_publishable_activity_followup": false', result.stdout)

    def test_sunday_no_publishable_activity_sends_second_collection_prompt(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--date",
                "2026-08-09",
                "--activity-fixture",
                str(FIXTURES / "activity-empty.json"),
                "--news-fixture",
                str(FIXTURES / "qunmind-ai-report.md"),
                "--json",
            ],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("暂时还没有可宣发的本周活动", result.stdout)
        self.assertNotIn("计划表里暂时还没有", result.stdout)
        self.assertIn("今天早上二花再轻轻补提醒一下", result.stdout)
        self.assertIn('"sunday_no_publishable_activity_followup": true', result.stdout)
        self.assertIn('"external_send_executed": false', result.stdout)

    def test_activity_and_ai_news_are_combined_without_sending(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--date",
                "2026-08-08",
                "--activity-fixture",
                str(FIXTURES / "activity-one.json"),
                "--news-fixture",
                str(FIXTURES / "qunmind-ai-report.md"),
                "--json",
            ],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("AI 工具共学", result.stdout)
        self.assertIn('"activity_publishable_count": 1', result.stdout)
        self.assertIn('"ai_news_item_count": 3', result.stdout)
        self.assertIn('"external_send_executed": false', result.stdout)

    def test_missing_news_fails_closed_by_default(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--date",
                "2026-08-08",
                "--activity-fixture",
                str(FIXTURES / "activity-empty.json"),
                "--news-fixture",
                str(FIXTURES / "missing.md"),
            ],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ERROR:", result.stderr)

    def test_missing_news_can_be_explicitly_degraded(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--date",
                "2026-08-08",
                "--activity-fixture",
                str(FIXTURES / "activity-empty.json"),
                "--news-fixture",
                str(FIXTURES / "missing.md"),
                "--allow-news-unavailable",
            ],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("QunMind 的公开新闻源暂时没读到", result.stdout)

    def test_ai_section_parser_handles_qunmind_headings(self):
        module = load_module()
        markdown = (FIXTURES / "qunmind-ai-report.md").read_text(encoding="utf-8")

        items = module._extract_ai_news_items(markdown, 2)

        self.assertEqual(
            [item.title for item in items],
            ["OpenAI 发布新的 Agent 编排实践", "Anthropic 更新企业安全评估"],
        )
        self.assertIn("多工具协作", items[0].summary)

    def test_prepare_send_request_builds_text_announcement_group_payload(self):
        result = self.run_script(
            "--json",
            "--prepare-send-request",
            "--approved-artifact-id",
            APPROVED_ARTIFACT_ID,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        send_payload = payload["send_request"]["payload"]
        nested = send_payload["payload"]
        self.assertEqual(send_payload["work_item_type"], "group_message_request")
        self.assertEqual(send_payload["capability_key"], "erhua.send_group_message")
        self.assertEqual(nested["workflow_type"], "text_activity_announcement")
        self.assertEqual(nested["approved_artifact_type"], "text_announcement")
        self.assertEqual(nested["approved_artifact_id"], APPROVED_ARTIFACT_ID)
        self.assertTrue(nested["approved_artifact_content_hash"].startswith("sha256:"))
        self.assertIn("operations-work-item-create", payload["send_request"]["shell_preview"])
        self.assertFalse(payload["send_request"]["execute_requested"])
        self.assertFalse(payload["external_send_executed"])

    def test_prepare_send_request_requires_uuid(self):
        result = self.run_script(
            "--prepare-send-request",
            "--approved-artifact-id",
            "not-a-uuid",
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("approved_artifact_id must be a uuid", result.stderr)

    def test_prepare_artifact_builds_text_announcement_create_payload(self):
        result = self.run_script("--json", "--prepare-artifact")

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        artifact_payload = payload["artifact_create"]["payload"]
        self.assertEqual(artifact_payload["date"], "2026-08-08")
        self.assertIn("二花早报来啦", artifact_payload["message_text"])
        self.assertEqual(artifact_payload["source_record_ref"], "erhua_morning_brief:2026-08-08")
        self.assertIn(
            "operations-text-announcement-artifact-create",
            payload["artifact_create"]["shell_preview"],
        )
        self.assertFalse(payload["artifact_create"]["execute_requested"])
        self.assertFalse(payload["database_writes"])

    def test_publish_plan_includes_manual_release_steps(self):
        result = self.run_script("--json", "--prepare-artifact", "--publish-plan")

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        plan = payload["publish_plan"]
        step_names = [step["name"] for step in plan["steps"]]
        self.assertIn("approve_text_artifact", step_names)
        self.assertIn("final_confirm_group_message_request", step_names)
        self.assertIn("record_send_ready", step_names)
        self.assertIn("manual_qiwe_post_if_adapter_unavailable", step_names)
        commands = "\n".join(step["command"] for step in plan["steps"])
        self.assertIn("operations-artifact-review-decision", commands)
        self.assertIn("operations-group-message-confirm", commands)
        self.assertIn("run-group-message-send-worker", commands)
        self.assertIn("二花早报来啦", plan["manual_post_text"])
        self.assertFalse(plan["external_send_executed"])


if __name__ == "__main__":
    unittest.main()
