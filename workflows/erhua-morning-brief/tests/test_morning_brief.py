from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


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
        self.assertIn("今天群里暂时没有安排好的活动", result.stdout)
        self.assertIn("发起一个小活动", result.stdout)
        self.assertIn("OpenAI 发布新的 Agent 编排实践", result.stdout)
        self.assertIn("Anthropic 更新企业安全评估", result.stdout)
        self.assertIn("OpenAI launches realtime agent evaluations", result.stdout)
        self.assertIn("OpenAI 发布实时 Agent 评估更新", result.stdout)
        self.assertNotIn("已确认", result.stdout)
        self.assertNotIn("可宣发", result.stdout)
        self.assertNotIn("需要前置", result.stdout)
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
        self.assertIn("这周暂时还没有可以直接报名的活动", result.stdout)
        self.assertNotIn("可宣发", result.stdout)
        self.assertNotIn("需要前置", result.stdout)
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
        self.assertIn('"ai_news_item_count": 5', result.stdout)
        self.assertIn('"external_send_executed": false', result.stdout)

    def test_internal_planning_wording_blocks_chat_facing_brief(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir) / "activity-internal.json"
            fixture.write_text(
                json.dumps(
                    {
                        "success": True,
                        "publishable_count": 1,
                        "announcement_text": "今日活动预告\n\n1. AI 工具共学\n需要前置：确认活动室。",
                        "requires_human_confirmation": True,
                        "external_send_executed": False,
                    },
                    ensure_ascii=False,
                ),
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--date",
                    "2026-08-08",
                    "--activity-fixture",
                    str(fixture),
                    "--news-fixture",
                    str(FIXTURES / "qunmind-ai-report.md"),
                    "--json",
                ],
                check=False,
                capture_output=True,
                text=True,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("morning brief contains internal planning wording", result.stderr)

    def test_activity_status_fields_block_before_brief_composition(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir) / "activity-status.json"
            fixture.write_text(
                json.dumps(
                    {
                        "success": True,
                        "publishable_count": 1,
                        "announcement_text": (
                            "今日活动预告\n\n"
                            "1. AI 工具共学\n"
                            "时间：10:00\n"
                            "地点：社区客厅\n"
                            "活动状态：已确认\n"
                            "宣发状态：可宣发"
                        ),
                        "requires_human_confirmation": True,
                        "external_send_executed": False,
                    },
                    ensure_ascii=False,
                ),
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--date",
                    "2026-08-08",
                    "--activity-fixture",
                    str(fixture),
                    "--news-fixture",
                    str(FIXTURES / "qunmind-ai-report.md"),
                    "--json",
                ],
                check=False,
                capture_output=True,
                text=True,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("morning brief contains internal planning wording", result.stderr)

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
                "--news-feed-url",
                "https://127.0.0.1:1/rss",
                "--news-feed-timeout-seconds",
                "1",
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
                "--news-feed-url",
                "https://127.0.0.1:1/rss",
                "--news-feed-timeout-seconds",
                "1",
                "--allow-news-unavailable",
            ],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("QunMind 的公开新闻源暂时没读到", result.stdout)

    def test_rss_fallback_parser_extracts_public_items(self):
        module = load_module()
        rss = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>OpenAI 发布新的研究更新</title>
      <description><![CDATA[<p>重点是更稳定的工具调用与评估。</p>]]></description>
    </item>
    <item>
      <title>Anthropic 更新企业安全实践</title>
      <description>面向团队协作的模型治理建议。</description>
    </item>
  </channel>
</rss>"""

        items = module._extract_feed_news_items(rss, 2)

        self.assertEqual(
            [item.title for item in items],
            ["OpenAI 发布新的研究更新", "Anthropic 更新企业安全实践"],
        )
        self.assertIn("工具调用", items[0].summary)

    def test_rss_fallback_parser_skips_english_items_without_translation(self):
        module = load_module()
        rss = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>OpenAI launches a new agent guide</title>
      <description>Teams can use the guide before connecting agents to production systems.</description>
    </item>
    <item>
      <title>Google 发布 Gemini 更新</title>
      <description>面向开发者的模型工具更新。</description>
    </item>
  </channel>
</rss>"""

        items = module._extract_feed_news_items(rss, 5)

        self.assertEqual([item.title for item in items], ["Google 发布 Gemini 更新"])

    def test_rss_fallback_translates_english_items_when_llm_configured(self):
        module = load_module()
        rss = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>OpenAI launches a new agent guide</title>
      <description>Teams can use the guide before connecting agents to production systems.</description>
    </item>
  </channel>
</rss>"""
        args = SimpleNamespace(
            news_llm_base_url="https://llm.example.test/v1",
            news_llm_api_key="fixture-key",
            news_llm_model="gpt-5.2",
            news_feed_timeout_seconds=12,
        )
        captured = {}

        class FakeResponse:
            def raise_for_status(self) -> None:
                return None

            def json(self) -> dict:
                return {
                    "choices": [
                        {
                            "message": {
                                "content": (
                                    '{"title_zh": "OpenAI 发布新的智能体指南", '
                                    '"summary_zh": "团队可以在连接生产系统前使用该指南。"}'
                                )
                            }
                        }
                    ]
                }

        def fake_post(url, headers=None, json=None, timeout=None):
            captured["url"] = url
            captured["headers"] = headers
            captured["payload"] = json
            return FakeResponse()

        original_httpx = sys.modules.get("httpx")
        sys.modules["httpx"] = SimpleNamespace(post=fake_post)
        try:
            items = module._extract_feed_news_items(rss, 5, args)
        finally:
            if original_httpx is None:
                sys.modules.pop("httpx", None)
            else:
                sys.modules["httpx"] = original_httpx

        self.assertEqual(len(items), 1)
        self.assertEqual(items[0].title, "OpenAI launches a new agent guide")
        self.assertEqual(items[0].title_zh, "OpenAI 发布新的智能体指南")
        self.assertIn("连接生产系统", items[0].summary_zh)
        self.assertTrue(captured["url"].endswith("/chat/completions"))
        self.assertEqual(captured["payload"]["model"], "gpt-5.2")

    def test_rss_fallback_translation_failure_keeps_english_item_skipped(self):
        module = load_module()
        rss = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>OpenAI launches a new agent guide</title>
      <description>Teams can use the guide before connecting agents to production systems.</description>
    </item>
  </channel>
</rss>"""
        args = SimpleNamespace(
            news_llm_base_url="https://llm.example.test/v1",
            news_llm_api_key="fixture-key",
            news_llm_model="gpt-5.2",
            news_feed_timeout_seconds=12,
        )

        def failing_post(url, headers=None, json=None, timeout=None):
            raise RuntimeError("translation endpoint unavailable")

        original_httpx = sys.modules.get("httpx")
        sys.modules["httpx"] = SimpleNamespace(post=failing_post)
        try:
            items = module._extract_feed_news_items(rss, 5, args)
        finally:
            if original_httpx is None:
                sys.modules.pop("httpx", None)
            else:
                sys.modules["httpx"] = original_httpx

        self.assertEqual(items, [])

    def test_rss_fallback_without_llm_config_skips_english(self):
        module = load_module()
        rss = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>OpenAI launches a new agent guide</title>
      <description>Teams can use the guide before connecting agents to production systems.</description>
    </item>
  </channel>
</rss>"""
        args = SimpleNamespace(
            news_llm_base_url="",
            news_llm_api_key="",
            news_llm_model="",
            news_feed_timeout_seconds=12,
        )
        items = module._extract_feed_news_items(rss, 5, args)
        self.assertEqual(items, [])

    def test_rss_fallback_parser_rejects_dtd_and_entities(self):
        module = load_module()

        for xml in (
            """<?xml version="1.0"?><!DOCTYPE rss [<!ENTITY xxe "blocked">]><rss />""",
            """<?xml version="1.0"?><rss><!ENTITY xxe "blocked"></rss>""",
        ):
            with self.subTest(xml=xml):
                with self.assertRaisesRegex(RuntimeError, "DTD or entity"):
                    module._extract_feed_news_items(xml, 1)

    def test_rss_fallback_skips_unsafe_feed_and_uses_next_feed(self):
        module = load_module()
        args = SimpleNamespace(
            news_feed_url=[
                "https://openai.com/news/rss.xml",
                "https://blog.google/technology/ai/rss/",
            ],
            news_feed_timeout_seconds=1,
            news_limit=1,
        )
        unsafe = b'<?xml version="1.0"?><!DOCTYPE rss [<!ENTITY xxe "blocked">]><rss />'
        valid = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>Google 发布 Gemini 更新</title>
      <description>面向开发者的模型工具更新。</description>
    </item>
  </channel>
</rss>""".encode()
        calls = []

        class FakeResponse:
            def __init__(self, body: bytes, final_url: str):
                self.body = body
                self.final_url = final_url

            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc_value, traceback):
                return False

            def geturl(self) -> str:
                return self.final_url

            def read(self, limit: int) -> bytes:
                return self.body[:limit]

        class FakeOpener:
            def open(self, request, timeout):
                calls.append((request.full_url, timeout))
                if request.full_url.startswith("https://openai.com/"):
                    return FakeResponse(unsafe, request.full_url)
                return FakeResponse(valid, request.full_url)

        original_build_opener = module.urllib.request.build_opener
        module.urllib.request.build_opener = lambda *_handlers: FakeOpener()
        try:
            items = module._fetch_feed_news_items(args)
        finally:
            module.urllib.request.build_opener = original_build_opener

        self.assertEqual([url for url, _timeout in calls], args.news_feed_url)
        self.assertEqual([item.title for item in items], ["Google 发布 Gemini 更新"])

    def test_rss_fallback_rejects_unsafe_final_response_url(self):
        module = load_module()
        args = SimpleNamespace(
            news_feed_url=[
                "https://openai.com/news/rss.xml",
                "https://blog.google/technology/ai/rss/",
            ],
            news_feed_timeout_seconds=1,
            news_limit=1,
        )
        rss = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>Should Not Be Read</title>
      <description>Unsafe redirected target.</description>
    </item>
  </channel>
</rss>""".encode()
        valid = """<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <item>
      <title>Google 发布 Gemini 更新</title>
      <description>面向开发者的模型工具更新。</description>
    </item>
  </channel>
</rss>""".encode()
        reads = []

        class FakeResponse:
            def __init__(self, body: bytes, final_url: str):
                self.body = body
                self.final_url = final_url

            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc_value, traceback):
                return False

            def geturl(self) -> str:
                return self.final_url

            def read(self, limit: int) -> bytes:
                reads.append(self.final_url)
                return self.body[:limit]

        class FakeOpener:
            def open(self, request, timeout):
                if request.full_url.startswith("https://openai.com/"):
                    return FakeResponse(rss, "https://169.254.169.254/latest/meta-data")
                return FakeResponse(valid, request.full_url)

        original_build_opener = module.urllib.request.build_opener
        module.urllib.request.build_opener = lambda *_handlers: FakeOpener()
        try:
            items = module._fetch_feed_news_items(args)
        finally:
            module.urllib.request.build_opener = original_build_opener

        self.assertEqual(reads, ["https://blog.google/technology/ai/rss/"])
        self.assertEqual([item.title for item in items], ["Google 发布 Gemini 更新"])

    def test_news_feed_redirect_handler_rejects_redirects(self):
        module = load_module()
        handler = module.NoNewsFeedRedirect()
        request = module.urllib.request.Request("https://openai.com/news/rss.xml")

        try:
            handler.redirect_request(
                request,
                None,
                302,
                "Found",
                {},
                "https://127.0.0.1/internal",
            )
            self.fail("redirect_request must raise HTTPError")
        except module.urllib.error.HTTPError as exc:
            self.assertEqual(exc.code, 302)

    def test_feed_urls_reject_non_allowlisted_hosts(self):
        module = load_module()
        args = SimpleNamespace(
            news_feed_url=[
                "https://127.0.0.1:1/rss",
                "https://openai.com/news/rss.xml",
            ]
        )

        self.assertEqual(module._feed_urls(args), ["https://openai.com/news/rss.xml"])

    def test_ai_section_parser_handles_qunmind_headings(self):
        module = load_module()
        markdown = (FIXTURES / "qunmind-ai-report.md").read_text(encoding="utf-8")

        items = module._extract_ai_news_items(markdown, 2)

        self.assertEqual(
            [item.title for item in items],
            ["OpenAI 发布新的 Agent 编排实践", "Anthropic 更新企业安全评估"],
        )
        self.assertIn("多工具协作", items[0].summary)

    def test_ai_section_parser_keeps_chinese_translation_for_english_items(self):
        module = load_module()
        markdown = (FIXTURES / "qunmind-ai-report.md").read_text(encoding="utf-8")

        items = module._extract_ai_news_items(markdown, 5)
        translated = items[3]

        self.assertEqual(translated.title, "OpenAI launches realtime agent evaluations")
        self.assertEqual(translated.title_zh, "OpenAI 发布实时 Agent 评估更新")
        self.assertIn("repeatable checks", translated.summary)
        self.assertIn("可重复检查", translated.summary_zh)
        self.assertIn(
            "中文：OpenAI 发布实时 Agent 评估更新",
            "\n".join(module._news_item_lines(4, translated)),
        )

    def test_ai_section_parser_rejects_english_items_without_chinese_translation(self):
        module = load_module()
        markdown = """## AI 前沿

### AI｜OpenAI launches a new agent guide

**Summary**: Teams can use the guide before connecting agents to production systems.
"""

        with self.assertRaisesRegex(RuntimeError, "requires Chinese"):
            module._extract_ai_news_items(markdown, 5)

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
