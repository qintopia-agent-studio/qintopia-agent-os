from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path
from types import SimpleNamespace

try:
    from PIL import Image as _PILImage

    _PIL_AVAILABLE = True
except Exception:  # pragma: no cover - exercised only without Pillow
    _PIL_AVAILABLE = False


WORKFLOW_DIR = Path(__file__).resolve().parents[1]
SCRIPT = WORKFLOW_DIR / "morning_brief.py"
FIXTURES = Path(__file__).resolve().parent / "fixtures"
APPROVED_ARTIFACT_ID = "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5"


def _renderer_module_for_check():
    sys.path.insert(0, str(WORKFLOW_DIR))
    spec = importlib.util.spec_from_file_location(
        "erhua_morning_brief_renderer_guard", WORKFLOW_DIR / "morning_brief_renderer.py"
    )
    renderer = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = renderer
    spec.loader.exec_module(renderer)
    return renderer


def _render_is_supported() -> bool:
    """Whether ``render()`` can actually produce an image on this host.

    render() prefers Playwright; the Pillow fallback fails closed unless a
    CJK-capable font exists. A minimal host with neither a browser nor a CJK
    font would fail closed and leave no file, so the render-output assertions
    must skip there instead of asserting a file that never gets written.
    """
    try:
        from playwright.sync_api import sync_playwright  # noqa: F401

        return True
    except Exception:
        pass
    if not _PIL_AVAILABLE:
        return False
    try:
        renderer = _renderer_module_for_check()
    except Exception:
        return False
    return any(Path(p).exists() for p in renderer._font_candidates())


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
        self.assertIn("来源：https://example.test/openai-agent", result.stdout)
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

    def test_qunmind_news_tops_up_from_rss_when_below_default_limit(self):
        module = load_module()
        old_argv = sys.argv
        sys.argv = ["morning_brief.py", "--date", "2026-08-08"]
        try:
            args = module._parse_args()
        finally:
            sys.argv = old_argv

        rss_items = [
            module.AiNewsItem(title="RSS 补充一", summary="社区可读的一句话摘要。"),
            module.AiNewsItem(title="RSS 补充二", summary="第二条补位新闻。"),
            module.AiNewsItem(title="RSS 补充三", summary="第三条补位新闻。"),
        ]
        with mock.patch.object(
            module,
            "_run_qunmind_report",
            return_value=(FIXTURES / "qunmind-ai-report.md").read_text(encoding="utf-8"),
        ), mock.patch.object(
            module,
            "_fetch_feed_news_items",
            return_value=rss_items,
        ), mock.patch.object(
            module,
            "_prepare_activity",
            return_value={"success": True, "publishable_count": 1, "announcement_text": "今日活动预告\nAI 工具共学"},
        ), mock.patch.object(
            module,
            "_prepare_weather",
            return_value=None,
        ):
            result = module.build_morning_brief(args)

        self.assertEqual(result["ai_news_item_count"], 8)
        self.assertEqual(result["ai_news_source"], "qunmind_public_only_with_public_rss_top_up")
        self.assertIn("RSS 补充三", result["morning_brief_text"])

    def test_newsletter_style_news_keeps_source_links(self):
        module = load_module()
        markdown = """📮 TinTinAI Weekly｜AI 一周资讯（08.24-08.30）

字节AI 生产力整合：TRAE、扣子并入豆包，将推统一办公品牌 "豆包工作"
https://36kr.com/p/3953230805876099

Claude Code反超GitHub Copilot登顶第一、90%程序员已用上Agent，最新AI编码调查报告来了
https://36kr.com/p/3954326503144584

你最关注哪条 AI 新闻？欢迎在群里一起讨论！
"""

        items = module._extract_ai_news_items(markdown, 8)

        self.assertEqual(len(items), 2)
        self.assertEqual(items[0].url, "https://36kr.com/p/3953230805876099")
        self.assertIn("豆包工作", items[0].title)
        self.assertNotIn("你最关注", [item.title for item in items])

    def test_public_source_urls_must_be_https_without_credentials(self):
        module = load_module()

        self.assertEqual(
            module._sanitize_public_url("https://36kr.com/p/3953230805876099"),
            "https://36kr.com/p/3953230805876099",
        )
        self.assertEqual(module._sanitize_public_url("http://36kr.com/p/3953230805876099"), "")
        self.assertEqual(module._sanitize_public_url("https://user:pass@example.com/ai"), "")

    def test_news_display_uses_source_and_discussion_prompt(self):
        module = load_module()
        text, blocks, _ = module._compose_brief(
            date="2026-08-31",
            weekday_label="星期一",
            weather=None,
            activity_text="今天暂时没有安排好的活动。",
            activity_count=0,
            news_items=[
                module.AiNewsItem(
                    title="Claude Code 反超 GitHub Copilot 登顶第一",
                    summary="最新 AI 编码调查显示 Agent 已经进入高频开发工作流。",
                    url="https://36kr.com/p/3954326503144584",
                )
            ],
            news_unavailable=False,
        )

        self.assertIn("二花 AI 早报｜今日资讯", text)
        self.assertIn("看点：最新 AI 编码调查显示 Agent 已经进入高频开发工作流。", text)
        self.assertIn("来源：https://36kr.com/p/3954326503144584", text)
        self.assertIn("你最关注哪条 AI 新闻？欢迎在群里一起讨论。", text)

    def test_news_parser_ignores_label_like_rows(self):
        module = load_module()
        markdown = """## AI 前沿

- 摘要：本段只是栏目说明，不应当变成新闻标题。
- 来源：https://example.test/source-index
- OpenAI 发布新的 Agent 编排实践：多工具协作进入更稳定的工作流。
- 你最关注哪条 AI 新闻？欢迎在群里一起讨论。
"""

        items = module._extract_ai_news_items(markdown, 8)

        self.assertEqual(len(items), 1)
        self.assertEqual(items[0].title, "OpenAI 发布新的 Agent 编排实践")

    def test_news_parser_keeps_open_source_titles(self):
        module = load_module()
        markdown = """## AI 前沿

### AI｜Open-source AI agents gain a new deployment guide

Summary: Teams can use the guide before connecting agents to production systems.
中文标题：开源 AI Agent 部署指南发布
中文摘要：团队可以用这份指南把 Agent 更稳地接入生产环境。
"""

        items = module._extract_ai_news_items(markdown, 8)

        self.assertEqual(len(items), 1)
        self.assertEqual(items[0].title, "Open-source AI agents gain a new deployment guide")
        self.assertEqual(items[0].title_zh, "开源 AI Agent 部署指南发布")

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

    # --- News domain (Rust sidecar) -------------------------------------------
    # The fetch / RSS parse / recency / cross-day dedup logic now lives in the
    # Rust sidecar. Python is a thin orchestrator that shells out for the raw
    # items and translates them. These tests cover the thin caller boundary and
    # the success-path history record; the logic itself is unit-tested in Rust.

    def _news_args(self, **overrides):
        base = dict(
            sidecar_bin="qintopia-message-sidecar",
            news_feed_timeout_seconds=12,
            news_limit=8,
            news_feed_url=[],
            news_recency_days=0,
            news_dedup_days=0,
            news_history_path="",
            allow_news_unavailable=False,
            news_llm_base_url="",
            news_llm_api_key="",
            news_llm_model="",
        )
        base.update(overrides)
        return SimpleNamespace(**base)

    def _fake_sidecar_completed(self, items):
        class FakeCompleted:
            returncode = 0
            stdout = json.dumps(items, ensure_ascii=False)
            stderr = ""

        return FakeCompleted()

    def test_fetch_feed_news_items_shells_out_to_sidecar(self):
        # The thin caller must invoke the Rust sidecar news-fetch subcommand and
        # map its JSON payload into AiNewsItem rows.
        module = load_module()
        args = self._news_args(
            news_recency_days=14,
            news_dedup_days=7,
            news_feed_url=["https://openai.com/news/rss.xml"],
        )
        captured = {}

        def fake_run(command, check=False, capture_output=False, text=False):
            captured["command"] = command
            return self._fake_sidecar_completed(
                [
                    {"title": "OpenAI 发布新的研究更新", "summary": "更稳定的工具调用。"},
                    {"title": "Google 发布 Gemini 更新", "summary": "面向开发者。"},
                ]
            )

        original = module.subprocess.run
        module.subprocess.run = fake_run
        try:
            items = module._fetch_feed_news_items(args)
        finally:
            module.subprocess.run = original

        self.assertEqual(len(items), 2)
        self.assertEqual(items[0].title, "OpenAI 发布新的研究更新")
        self.assertIn("operations-erhua-morning-brief-news-fetch", captured["command"])
        self.assertIn("--news-recency-days", captured["command"])
        self.assertIn("14", captured["command"])
        self.assertIn("--news-dedup-days", captured["command"])
        self.assertIn("--news-limit", captured["command"])
        self.assertIn("8", captured["command"])
        self.assertIn("--news-feed-url", captured["command"])
        self.assertIn("https://openai.com/news/rss.xml", captured["command"])

    def test_fetch_feed_news_items_translates_english_items(self):
        # English rows returned by the sidecar are translated via the LLM before
        # entering the brief.
        module = load_module()
        args = self._news_args(
            news_llm_base_url="https://llm.example.test/v1",
            news_llm_api_key="fixture-key",
            news_llm_model="gpt-5.2",
        )

        class FakeResponse:
            def raise_for_status(self):
                return None

            def json(self):
                return {
                    "choices": [
                        {
                            "message": {
                                "content": (
                                    '{"title_zh": "OpenAI 发布新的智能体指南", '
                                    '"summary_zh": "团队可在连接生产系统前使用该指南。"}'
                                )
                            }
                        }
                    ]
                }

        def fake_post(url, headers=None, json=None, timeout=None):
            return FakeResponse()

        def fake_run(command, check=False, capture_output=False, text=False):
            return self._fake_sidecar_completed(
                [
                    {
                        "title": "OpenAI launches a new agent guide",
                        "summary": "Teams can use the guide before connecting agents.",
                    }
                ]
            )

        original_run = module.subprocess.run
        module.subprocess.run = fake_run
        original_httpx = sys.modules.get("httpx")
        sys.modules["httpx"] = SimpleNamespace(post=fake_post)
        try:
            items = module._fetch_feed_news_items(args)
        finally:
            module.subprocess.run = original_run
            if original_httpx is None:
                sys.modules.pop("httpx", None)
            else:
                sys.modules["httpx"] = original_httpx

        self.assertEqual(len(items), 1)
        self.assertEqual(items[0].title, "OpenAI launches a new agent guide")
        self.assertEqual(items[0].title_zh, "OpenAI 发布新的智能体指南")

    def test_fetch_feed_news_items_skips_english_without_translation(self):
        # Without an LLM config, English-only rows are dropped rather than sent raw.
        module = load_module()
        args = self._news_args()

        def fake_run(command, check=False, capture_output=False, text=False):
            return self._fake_sidecar_completed(
                [
                    {
                        "title": "OpenAI launches a new agent guide",
                        "summary": "Teams can use the guide.",
                    }
                ]
            )

        original = module.subprocess.run
        module.subprocess.run = fake_run
        try:
            items = module._fetch_feed_news_items(args)
        finally:
            module.subprocess.run = original
        self.assertEqual(items, [])

    def test_fetch_feed_news_items_degrades_when_sidecar_fails(self):
        # With --allow-news-unavailable the brief must tolerate a sidecar failure.
        module = load_module()
        args = self._news_args(allow_news_unavailable=True)

        class FakeFailed:
            returncode = 1
            stdout = ""
            stderr = "boom"

        def fake_run(command, check=False, capture_output=False, text=False):
            return FakeFailed()

        original = module.subprocess.run
        module.subprocess.run = fake_run
        try:
            items = module._fetch_feed_news_items(args)
        finally:
            module.subprocess.run = original
        self.assertEqual(items, [])

    def test_fetch_feed_news_items_raises_when_sidecar_fails_non_allowable(self):
        # Without --allow-news-unavailable a sidecar failure is a hard error.
        module = load_module()
        args = self._news_args()

        class FakeFailed:
            returncode = 1
            stdout = ""
            stderr = "boom"

        def fake_run(command, check=False, capture_output=False, text=False):
            return FakeFailed()

        original = module.subprocess.run
        module.subprocess.run = fake_run
        try:
            with self.assertRaises(RuntimeError):
                module._fetch_feed_news_items(args)
        finally:
            module.subprocess.run = original

    def test_build_morning_brief_records_rss_history_on_success(self):
        # Regression: the dedup record is a Rust sidecar call made only on the
        # brief's success path (RSS fallback + artifact committed). Drive
        # build_morning_brief end-to-end with the sidecar (and QunMind) stubbed.
        module = load_module()
        tmp_root = tempfile.mkdtemp()
        history = os.path.join(tmp_root, "news-history.json")
        render_path = os.path.join(tmp_root, "card.png")

        recorded = {}
        fetched = False

        def fake_run(command, check=False, capture_output=False, text=False):
            if "operations-erhua-morning-brief-news-fetch" in command:
                nonlocal fetched
                fetched = True
                return self._fake_sidecar_completed(
                    [{"title": "OpenAI 发布新的研究更新", "summary": "更稳定的工具调用。"}]
                )
            if "operations-erhua-morning-brief-news-record" in command:
                recorded["command"] = command
            return self._fake_sidecar_completed([])

        argv = [
            "morning_brief.py",
            "--date", "2026-08-20",
            "--render-image", render_path,
            "--render-image-format", "jpeg",
            "--prepare-artifact",
            "--execute-artifact-create",
            "--apply-artifact-create",
            "--news-recency-days", "0",
            "--news-dedup-days", "7",
            "--news-history-path", history,
        ]
        old_argv = sys.argv
        sys.argv = argv
        try:
            args = module._parse_args()
        finally:
            sys.argv = old_argv

        original_run = module.subprocess.run
        module.subprocess.run = fake_run
        try:
            with mock.patch.object(module, "_run_qunmind_report", side_effect=RuntimeError("no qunmind")), \
                    mock.patch.object(module, "_prepare_activity", return_value={"success": True, "publishable_count": 0, "announcement_text": ""}), \
                    mock.patch.object(module, "_activity_section", return_value=("", 0, False)), \
                    mock.patch.object(module, "_prepare_weather", return_value=None), \
                    mock.patch.object(module, "_build_card", return_value=None), \
                    mock.patch.object(module.morning_brief_renderer, "render", side_effect=lambda card, path, image_format: Path(path).write_text("x")), \
                    mock.patch.object(module, "_artifact_create_payload", return_value={}), \
                    mock.patch.object(module, "_artifact_create_action", return_value={}), \
                    mock.patch.object(module, "_publish_plan", return_value={}):
                result = module.build_morning_brief(args)
        finally:
            module.subprocess.run = original_run

        self.assertEqual(result["ai_news_source"], "public_rss_fallback")
        self.assertTrue(fetched)
        self.assertIn(
            "operations-erhua-morning-brief-news-record",
            recorded["command"],
            "success path must record sent titles via the sidecar",
        )
        self.assertIn(
            '"OpenAI 发布新的研究更新"',
            " ".join(recorded["command"]),
            "selected titles must be passed to the sidecar record call",
        )

    def test_qunmind_top_up_records_only_rss_history_titles_on_success(self):
        module = load_module()
        tmp_root = tempfile.mkdtemp()
        history = os.path.join(tmp_root, "news-history.json")

        recorded = {}
        rss_items = [
            module.AiNewsItem(title="RSS 补充一", summary="社区可读的一句话摘要。"),
            module.AiNewsItem(title="RSS 补充二", summary="第二条补位新闻。"),
            module.AiNewsItem(title="RSS 补充三", summary="第三条补位新闻。"),
        ]

        def fake_run(command, check=False, capture_output=False, text=False):
            if "operations-erhua-morning-brief-news-record" in command:
                recorded["command"] = command
            return self._fake_sidecar_completed([])

        argv = [
            "morning_brief.py",
            "--date", "2026-08-20",
            "--prepare-artifact",
            "--execute-artifact-create",
            "--apply-artifact-create",
            "--news-dedup-days", "7",
            "--news-history-path", history,
        ]
        old_argv = sys.argv
        sys.argv = argv
        try:
            args = module._parse_args()
        finally:
            sys.argv = old_argv

        original_run = module.subprocess.run
        module.subprocess.run = fake_run
        try:
            with mock.patch.object(
                module,
                "_run_qunmind_report",
                return_value=(FIXTURES / "qunmind-ai-report.md").read_text(encoding="utf-8"),
            ), mock.patch.object(
                module,
                "_fetch_feed_news_items",
                return_value=rss_items,
            ), mock.patch.object(
                module,
                "_prepare_activity",
                return_value={"success": True, "publishable_count": 0, "announcement_text": ""},
            ), mock.patch.object(
                module,
                "_activity_section",
                return_value=("", 0, False),
            ), mock.patch.object(
                module,
                "_prepare_weather",
                return_value=None,
            ), mock.patch.object(
                module,
                "_artifact_create_payload",
                return_value={},
            ), mock.patch.object(
                module,
                "_artifact_create_action",
                return_value={},
            ):
                result = module.build_morning_brief(args)
        finally:
            module.subprocess.run = original_run

        self.assertEqual(result["ai_news_source"], "qunmind_public_only_with_public_rss_top_up")
        self.assertIn("operations-erhua-morning-brief-news-record", recorded["command"])
        titles_index = recorded["command"].index("--titles-json") + 1
        recorded_titles = json.loads(recorded["command"][titles_index])
        self.assertEqual(recorded_titles, ["RSS 补充一", "RSS 补充二", "RSS 补充三"])
        self.assertNotIn("OpenAI 发布新的 Agent 编排实践", recorded_titles)

    def test_news_llm_args_fall_back_to_shared_llm_env(self):
        module = load_module()
        old_base = os.environ.get("QINTOPIA_LLM_BASE_URL")
        old_key = os.environ.get("QINTOPIA_LLM_API_KEY")
        old_model = os.environ.get("QINTOPIA_LLM_MODEL")
        old_news = os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL")
        os.environ["QINTOPIA_LLM_BASE_URL"] = "https://llm.example.test/v1"
        os.environ["QINTOPIA_LLM_API_KEY"] = "shared-key"
        os.environ["QINTOPIA_LLM_MODEL"] = "gpt-5.2"
        os.environ.pop("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL", None)
        old_argv = sys.argv
        sys.argv = ["morning_brief.py", "--date", "2026-08-08"]
        try:
            args = module._parse_args()
        finally:
            sys.argv = old_argv
            for key, old in (
                ("QINTOPIA_LLM_BASE_URL", old_base),
                ("QINTOPIA_LLM_API_KEY", old_key),
                ("QINTOPIA_LLM_MODEL", old_model),
                ("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LLM_BASE_URL", old_news),
            ):
                if old is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = old
        self.assertEqual(args.news_llm_base_url, "https://llm.example.test/v1")
        self.assertEqual(args.news_llm_api_key, "shared-key")
        self.assertEqual(args.news_llm_model, "gpt-5.2")

    def test_parse_args_uses_eight_news_default_and_env_feeds(self):
        module = load_module()
        old_feed_urls = os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_FEED_URLS")
        old_limit = os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LIMIT")
        os.environ["QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_FEED_URLS"] = (
            "https://openai.com/news/rss.xml, https://huggingface.co/blog/feed.xml"
        )
        os.environ.pop("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LIMIT", None)
        old_argv = sys.argv
        sys.argv = ["morning_brief.py", "--date", "2026-08-08"]
        try:
            args = module._parse_args()
        finally:
            sys.argv = old_argv
            for key, old in (
                ("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_FEED_URLS", old_feed_urls),
                ("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_LIMIT", old_limit),
            ):
                if old is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = old

        self.assertEqual(args.news_limit, 8)
        self.assertEqual(
            args.news_feed_url,
            ["https://openai.com/news/rss.xml", "https://huggingface.co/blog/feed.xml"],
        )

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
            "4. OpenAI 发布实时 Agent 评估更新",
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

    def test_run_sidecar_action_returns_preview_when_not_executed(self):
        module = load_module()
        args = SimpleNamespace(execute_artifact_create=False, apply_artifact_create=False)
        command = ["echo", "hello"]
        action = module._run_sidecar_action(
            command,
            args=args,
            execute_flag="execute_artifact_create",
            apply_flag="apply_artifact_create",
            error_message="sidecar action failed",
        )
        self.assertEqual(action["command"], command)
        self.assertEqual(action["shell_preview"], "echo hello")
        self.assertFalse(action["execute_requested"])
        self.assertFalse(action["apply_requested"])
        self.assertNotIn("returncode", action)

    def test_run_sidecar_action_executes_command_when_requested(self):
        module = load_module()
        args = SimpleNamespace(execute_artifact_create=True, apply_artifact_create=True)
        command = [sys.executable, "-c", "print('{\"ok\": true}')"]
        action = module._run_sidecar_action(
            command,
            args=args,
            execute_flag="execute_artifact_create",
            apply_flag="apply_artifact_create",
            error_message="sidecar action failed",
        )
        self.assertEqual(action["returncode"], 0)
        self.assertIn("ok", action["stdout"])

    def test_run_sidecar_action_raises_on_failure(self):
        module = load_module()
        args = SimpleNamespace(execute_artifact_create=True, apply_artifact_create=True)
        command = [sys.executable, "-c", "import sys; sys.exit(1)"]
        with self.assertRaisesRegex(RuntimeError, "sidecar action failed"):
            module._run_sidecar_action(
                command,
                args=args,
                execute_flag="execute_artifact_create",
                apply_flag="apply_artifact_create",
                error_message="sidecar action failed",
            )

    def test_weather_fixture_appears_in_brief_and_blocks(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--date",
                "2026-08-08",
                "--activity-fixture",
                str(FIXTURES / "activity-empty.json"),
                "--news-fixture",
                str(FIXTURES / "qunmind-ai-report.md"),
                "--weather-fixture",
                str(FIXTURES / "weather.json"),
                "--json",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertTrue(payload["weather_available"])
        self.assertIn("今日天气", payload["morning_brief_text"])
        self.assertIn("阴", payload["morning_brief_text"])
        self.assertEqual(payload["brief_blocks"][1]["title"], "今日天气")
        self.assertIn("23.1°", payload["brief_blocks"][1]["body"])
        self.assertTrue(payload["highlight"].startswith("今天重点关注"))

    def test_missing_weather_degrades_gracefully(self):
        result = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--date",
                "2026-08-08",
                "--activity-fixture",
                str(FIXTURES / "activity-empty.json"),
                "--news-fixture",
                str(FIXTURES / "qunmind-ai-report.md"),
                "--weather-fixture",
                str(FIXTURES / "missing.json"),
                "--json",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertFalse(payload["weather_available"])
        self.assertIn("今日天气稍后补充", payload["morning_brief_text"])
        self.assertTrue(payload["highlight"].startswith("今天重点关注"))

    def test_render_image_produces_poster_file(self):
        if not _render_is_supported():
            self.skipTest("no renderer backend available (need Playwright or Pillow + CJK font)")
        with tempfile.TemporaryDirectory() as temp_dir:
            image_path = Path(temp_dir) / "card.png"
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
                    "--weather-fixture",
                    str(FIXTURES / "weather.json"),
                    "--render-image",
                    str(image_path),
                    "--render-image-format",
                    "png",
                    "--json",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(result.stdout)
            self.assertEqual(payload["rendered_image_path"], str(image_path.resolve()))
            self.assertTrue(image_path.exists())
            self.assertGreater(image_path.stat().st_size, 4096)

    def test_publish_plan_records_rendered_image_path(self):
        if not _render_is_supported():
            self.skipTest("no renderer backend available (need Playwright or Pillow + CJK font)")
        with tempfile.TemporaryDirectory() as temp_dir:
            image_path = Path(temp_dir) / "card.png"
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
                    "--weather-fixture",
                    str(FIXTURES / "weather.json"),
                    "--render-image",
                    str(image_path),
                    "--prepare-artifact",
                    "--publish-plan",
                    "--json",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(result.stdout)
            plan = payload["publish_plan"]
            self.assertEqual(plan["rendered_image_path"], str(image_path.resolve()))
            step_names = [step["name"] for step in plan["steps"]]
            self.assertIn("render_card_image", step_names)

    def test_build_card_news_not_double_numbered(self):
        module = load_module()
        news_items = [
            module.AiNewsItem(
                title="OpenAI ships GPT-5.6",
                summary="Faster agents for startups.",
                title_zh="GPT-5.6 发布",
                summary_zh="面向初创公司的更快智能体。",
            ),
            module.AiNewsItem(title="社区共读招募", summary="本周六下午在客厅。"),
        ]
        card = module._build_card(
            date="2026-08-17",
            weekday_label="周一",
            weather=None,
            news_items=news_items,
            brief_blocks=[
                {"title": "问候", "body": "hi"},
                {"title": "今日天气", "body": "晴。"},
                {"title": "今天活动", "body": "无活动。"},
                {"title": "AI 新闻", "body": "1. 英文：OpenAI ships GPT-5.6：Faster agents.\n    中文：GPT-5.6 发布：面向初创公司的更快智能体。"},
            ],
            highlight=None,
        )
        # One card entry per news item, each carrying the bilingual lines but no
        # leading list index (the renderer owns the numbering).
        self.assertEqual(len(card.ai_news_items), 2)
        self.assertFalse(
            card.ai_news_items[0].startswith("1."),
            "card news entries must not carry a leading list index",
        )
        self.assertIn("GPT-5.6 发布", card.ai_news_items[0])
        self.assertIn("原题：", card.ai_news_items[0])
        self.assertIn("看点：", card.ai_news_items[0])
        self.assertIn("社区共读招募", card.ai_news_items[1])
        # The HTML renderer must number each item once, never "1. 1. 英文".
        renderer = self._load_renderer_module()
        html = renderer._render_html(card, 720)
        self.assertNotIn("1. 1. 英文", html)
        self.assertNotIn("由小满自动整理", html)
        self.assertIn('<span class="num">01</span>', html)
        self.assertIn('<span class="num">02</span>', html)
        self.assertNotIn('class="mark"', html)
        self.assertIn("ERHUA DAILY", html)

    def test_pillow_fallback_renders_full_height_without_truncation(self):
        if not _PIL_AVAILABLE:
            self.skipTest("Pillow not installed")
        sys.path.insert(0, str(WORKFLOW_DIR))
        renderer_spec = importlib.util.spec_from_file_location(
            "erhua_morning_brief_renderer", WORKFLOW_DIR / "morning_brief_renderer.py"
        )
        renderer = importlib.util.module_from_spec(renderer_spec)
        sys.modules[renderer_spec.name] = renderer
        renderer_spec.loader.exec_module(renderer)
        if not any(Path(p).exists() for p in renderer._font_candidates()):
            self.skipTest("no CJK-capable font installed")

        # A long activity plus five bilingual news items can push the real
        # content height past the old fixed 4000px Pillow canvas. The fallback
        # must grow the canvas instead of silently cropping at min(y, 4000).
        long_activity = "\n".join(
            "社区路跑训练营报名现已开启，请提前到栗峪口集合并带好补给。" * 3 for _ in range(20)
        )
        bilingual_news = [
            "OpenAI released a new long-context model.  OpenAI 发布新模型，主打长上下文与多模态推理能力，已向开发者开放。"
            for _ in range(5)
        ]
        card = renderer.MorningBriefCard(
            greeting="二花早报",
            date_label="2026-08-17 周一",
            activity_body=long_activity,
            ai_news_items=bilingual_news,
            highlight="今日氛围：社区共建日。",
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            image_path = Path(temp_dir) / "tall.png"
            renderer._render_with_pillow(card, image_path, 720, "png")
            self.assertTrue(image_path.exists())
            with _PILImage.open(image_path) as img:
                width, height = img.size
                bottom_pixel = img.convert("RGB").getpixel((width // 2, height - 10))
            self.assertGreater(height, 4000, "Pillow canvas must grow past the old 4000px cap")
            self.assertNotEqual(bottom_pixel, (0, 0, 0), "poster must not contain out-of-bounds black padding")

    def _load_renderer_module(self):
        sys.path.insert(0, str(WORKFLOW_DIR))
        renderer_spec = importlib.util.spec_from_file_location(
            "erhua_morning_brief_renderer", WORKFLOW_DIR / "morning_brief_renderer.py"
        )
        renderer = importlib.util.module_from_spec(renderer_spec)
        sys.modules[renderer_spec.name] = renderer
        renderer_spec.loader.exec_module(renderer)
        return renderer

    def test_pil_font_fails_closed_without_cjk_font(self):
        if not _PIL_AVAILABLE:
            self.skipTest("Pillow not installed")
        renderer = self._load_renderer_module()
        original = renderer._font_candidates
        # No real font exists at this stub path; the renderer must refuse to
        # fall back to a bitmap default that cannot render Chinese.
        renderer._font_candidates = lambda *a, **k: ["/nonexistent-cjk-font.ttf"]
        try:
            with self.assertRaises(RuntimeError):
                renderer._pil_font(20)
        finally:
            renderer._font_candidates = original

    def test_render_degrades_to_none_without_font(self):
        if not _PIL_AVAILABLE:
            self.skipTest("Pillow not installed")
        renderer = self._load_renderer_module()
        original_candidates = renderer._font_candidates
        original_playwright = renderer._render_with_playwright
        renderer._font_candidates = lambda *a, **k: ["/nonexistent-cjk-font.ttf"]
        # Force the Playwright path to fail so we exercise the Pillow fallback,
        # which must also fail closed when no CJK font is available.
        renderer._render_with_playwright = lambda *a, **k: next(
            _ for _ in ()
        ).throw(RuntimeError("playwright unavailable"))
        try:
            card = renderer.MorningBriefCard(
                greeting="二花早报",
                date_label="2026-08-17 周一",
                activity_body="社区路跑训练营报名现已开启。",
                ai_news_items=["OpenAI 发布新模型。", "Anthropic 更新企业安全评估。"],
                highlight="今日氛围：社区共建日。",
            )
            with tempfile.TemporaryDirectory() as temp_dir:
                out = Path(temp_dir) / "x.png"
                # render() must not raise; the image is a derived artifact.
                renderer.render(card, out)
                self.assertFalse(out.exists(), "fail-closed render must leave no file")
        finally:
            renderer._font_candidates = original_candidates
            renderer._render_with_playwright = original_playwright

    def test_pillow_render_raises_when_card_exceeds_height_cap(self):
        if not _PIL_AVAILABLE:
            self.skipTest("Pillow not installed")
        renderer = self._load_renderer_module()
        if not any(Path(p).exists() for p in renderer._font_candidates()):
            self.skipTest("no CJK-capable font installed")
        # Enough wrapped lines to push the measured canvas past MAX_HEIGHT.
        huge_activity = "\n".join(
            "社区路跑训练营报名现已开启，请提前到栗峪口集合并带好补给。" for _ in range(300)
        )
        card = renderer.MorningBriefCard(
            greeting="二花早报",
            date_label="2026-08-17 周一",
            activity_body=huge_activity,
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            out = Path(temp_dir) / "too-tall.png"
            with self.assertRaises(renderer.CardTooTallError):
                renderer._render_with_pillow(card, out, 720, "png")
            self.assertFalse(out.exists(), "oversized card must leave no image file")

    def test_render_degrades_to_none_when_card_exceeds_height_cap(self):
        renderer = self._load_renderer_module()
        original_playwright = renderer._render_with_playwright
        original_pillow = renderer._render_with_pillow
        calls = {"pillow": 0}

        def too_tall_playwright(*args, **kwargs):
            raise renderer.CardTooTallError("card height 9000px exceeds 8192px storage cap")

        def counting_pillow(*args, **kwargs):
            calls["pillow"] += 1
            raise AssertionError("Pillow fallback must not run after CardTooTallError")

        renderer._render_with_playwright = too_tall_playwright
        renderer._render_with_pillow = counting_pillow
        try:
            card = renderer.MorningBriefCard(
                greeting="二花早报",
                date_label="2026-08-17 周一",
                activity_body="社区路跑训练营报名现已开启。",
            )
            with tempfile.TemporaryDirectory() as temp_dir:
                out = Path(temp_dir) / "too-tall.png"
                # render() must not raise; the worker degrades to the text brief.
                renderer.render(card, out)
                self.assertFalse(out.exists(), "oversized card must leave no image file")
                self.assertEqual(calls["pillow"], 0)
        finally:
            renderer._render_with_playwright = original_playwright
            renderer._render_with_pillow = original_pillow

    def test_render_removes_stale_output_when_card_exceeds_height_cap(self):
        renderer = self._load_renderer_module()
        original_playwright = renderer._render_with_playwright
        def too_tall(*args, **kwargs):
            raise renderer.CardTooTallError("card height 9000px exceeds 8192px storage cap")

        renderer._render_with_playwright = too_tall
        try:
            card = renderer.MorningBriefCard(
                greeting="二花早报",
                date_label="2026-08-17 周一",
                activity_body="社区路跑训练营报名现已开启。",
            )
            with tempfile.TemporaryDirectory() as temp_dir:
                out = Path(temp_dir) / "card.png"
                out.write_bytes(b"stale image from a previous run")
                renderer.render(card, out)
                self.assertFalse(
                    out.exists(),
                    "render must remove a stale output so the worker never uploads it",
                )
        finally:
            renderer._render_with_playwright = original_playwright

    def test_prepare_activity_passes_actor_agent_xiaoman_under_erhua_profile(self):
        """Activity preview must explicitly declare actor_agent=xiaoman.

        The Erhua morning-brief worker runs under the Erhua Hermes profile,
        whose env sets QINTOPIA_PROFILE_ID=erhua. The xiaoman activity wrapper
        gates on actor_agent=xiaoman; without an explicit actor_agent the env
        fallback resolves to "erhua" and the wrapper rejects the preview with
        "actor_agent must be xiaoman", failing the whole worker (the
        run=failed regression that started 2026-08-15). The explicit arg must
        win regardless of the profile env.
        """
        module = load_module()
        captured: dict = {}

        def fake_prepare(args):
            captured.update(args)
            return json.dumps(
                {"success": True, "publishable_count": 0, "announcement_text": "今日无活动"}
            )

        fake_variant = mock.Mock()
        fake_variant.handle_qintopia_xiaoman_activity_announcement_prepare.side_effect = (
            fake_prepare
        )
        env_patch = mock.patch.dict(
            os.environ,
            {
                "QINTOPIA_PROFILE_ID": "erhua",
                "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE": "1",
                "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE": "1",
                "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE": "1",
            },
        )
        args = SimpleNamespace(activity_fixture=None, operator_name="op", audience="社区群成员")
        with env_patch, mock.patch.object(
            module, "_load_xiaoman_variant", return_value=fake_variant
        ):
            result = module._prepare_activity("2026-08-08", args)

        self.assertEqual(captured.get("actor_agent"), "xiaoman")
        self.assertIs(result.get("success"), True)


if __name__ == "__main__":
    unittest.main()
