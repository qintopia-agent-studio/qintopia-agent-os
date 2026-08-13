#!/usr/bin/env python3
from __future__ import annotations

import argparse
import contextlib
import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
import types
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "daily_case_report.py"
SPEC = importlib.util.spec_from_file_location("daily_case_report", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("failed to load daily_case_report.py")
daily_case_report = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = daily_case_report
SPEC.loader.exec_module(daily_case_report)


class DailyCaseReportTest(unittest.TestCase):
    def test_report_date_without_date_uses_latest_rolling_24_hours(self) -> None:
        args = argparse.Namespace(date=None, timezone="Asia/Shanghai")

        start, end, display = daily_case_report._report_date_at(
            args,
            datetime(
                2026,
                8,
                8,
                18,
                30,
                45,
                123456,
                tzinfo=timezone(timedelta(hours=8)),
            ),
        )

        self.assertEqual(
            start.astimezone(timezone.utc).isoformat(),
            "2026-08-07T10:30:45+00:00",
        )
        self.assertEqual(
            end.astimezone(timezone.utc).isoformat(),
            "2026-08-08T10:30:45+00:00",
        )
        self.assertEqual(display, "过去24小时（截至 2026年08月08日 18:30）")

    def test_report_date_with_date_uses_requested_timezone_calendar_day(self) -> None:
        args = argparse.Namespace(date="2026-08-08", timezone="America/Los_Angeles")

        start, end, display = daily_case_report._report_date(args)

        self.assertEqual(display, "2026年08月08日")
        self.assertEqual(
            start.astimezone(timezone.utc).isoformat(),
            "2026-08-08T07:00:00+00:00",
        )
        self.assertEqual(
            end.astimezone(timezone.utc).isoformat(),
            "2026-08-09T07:00:00+00:00",
        )

    def test_normalize_message_times_renders_in_report_timezone(self) -> None:
        zone = daily_case_report._report_timezone("Asia/Shanghai")
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="alice",
                text="hello",
                sent_at=datetime(2026, 8, 7, 16, 30, tzinfo=timezone.utc),
                message_kind="text",
            )
        ]

        normalized = daily_case_report._normalize_message_times(messages, zone)

        self.assertEqual(
            normalized[0].sent_at.strftime("%Y-%m-%d %H:%M"),
            "2026-08-08 00:30",
        )

    def test_fetch_messages_uses_coalesced_report_time(self) -> None:
        report_time = datetime(2026, 8, 8, 9, 30, tzinfo=timezone.utc)
        captured: dict[str, object] = {}

        class FakeCursor:
            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc, tb):
                return False

            def execute(self, sql, params):
                captured["sql"] = sql
                captured["params"] = params

            def fetchall(self):
                return [("m1", "u1", "张三", "只有 received_at", "text", report_time)]

        class FakeConnection:
            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc, tb):
                return False

            def cursor(self):
                return FakeCursor()

        old_database_url = daily_case_report._database_url
        old_psycopg = sys.modules.get("psycopg")
        daily_case_report._database_url = lambda: "postgresql://unit"
        sys.modules["psycopg"] = types.SimpleNamespace(connect=lambda _url: FakeConnection())
        try:
            messages = daily_case_report._fetch_messages("chat-1", report_time, report_time)
        finally:
            daily_case_report._database_url = old_database_url
            if old_psycopg is None:
                sys.modules.pop("psycopg", None)
            else:
                sys.modules["psycopg"] = old_psycopg

        self.assertIn("COALESCE(m.sent_at, m.received_at) AS report_time", captured["sql"])
        self.assertIn("m.sender_person_id::text AS sender_person_id", captured["sql"])
        self.assertIn("m.message_kind = 'text'", captured["sql"])
        self.assertIn("NULLIF(BTRIM(m.text), '') IS NOT NULL", captured["sql"])
        self.assertEqual(messages[0].sent_at, report_time)

    def test_database_url_ignores_generic_database_url(self) -> None:
        old_message_store = os.environ.pop("QINTOPIA_MESSAGE_STORE_DATABASE_URL", None)
        old_sidecar = os.environ.pop("QINTOPIA_SIDECAR_DATABASE_URL", None)
        old_generic = os.environ.get("DATABASE_URL")
        os.environ["DATABASE_URL"] = "postgresql://wrong"
        try:
            self.assertIsNone(daily_case_report._database_url())
        finally:
            if old_message_store is not None:
                os.environ["QINTOPIA_MESSAGE_STORE_DATABASE_URL"] = old_message_store
            if old_sidecar is not None:
                os.environ["QINTOPIA_SIDECAR_DATABASE_URL"] = old_sidecar
            if old_generic is None:
                os.environ.pop("DATABASE_URL", None)
            else:
                os.environ["DATABASE_URL"] = old_generic

    def test_production_mode_requires_chat_id(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            env = os.environ.copy()
            env["PYTHONDONTWRITEBYTECODE"] = "1"
            env.pop(daily_case_report.CHAT_ID_ENV, None)
            completed = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--output-dir",
                    tmpdir,
                ],
                env=env,
                text=True,
                capture_output=True,
            )

        self.assertEqual(completed.returncode, 2)
        self.assertIn("requires --chat-id", completed.stderr)
        self.assertNotIn("database read-through is disabled", completed.stderr)

    def test_private_output_helpers_restrict_local_file_permissions(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            output_dir = daily_case_report._prepare_output_dir(str(Path(tmpdir) / "report"))
            html_path = output_dir / "report.html"

            daily_case_report._write_private_text(html_path, "private group text")

            self.assertEqual(output_dir.stat().st_mode & 0o777, 0o700)
            self.assertEqual(html_path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(html_path.read_text(encoding="utf-8"), "private group text")

    def test_prepare_output_dir_rejects_existing_non_private_directory(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            output_dir = Path(tmpdir) / "shared"
            output_dir.mkdir()
            output_dir.chmod(0o755)

            with self.assertRaisesRegex(RuntimeError, "already exists with mode 0755"):
                daily_case_report._prepare_output_dir(str(output_dir))

            self.assertEqual(output_dir.stat().st_mode & 0o777, 0o755)

    def test_production_mode_rejects_retained_html(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            env = os.environ.copy()
            env["PYTHONDONTWRITEBYTECODE"] = "1"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--render",
                    "html",
                    "--output-dir",
                    tmpdir,
                ],
                env=env,
                text=True,
                capture_output=True,
            )

        self.assertEqual(completed.returncode, 2)
        self.assertIn("cannot retain HTML", completed.stderr)

    def test_production_keep_html_is_rejected_before_reading_database(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            env = os.environ.copy()
            env["PYTHONDONTWRITEBYTECODE"] = "1"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--keep-html",
                    "--output-dir",
                    tmpdir,
                ],
                env=env,
                text=True,
                capture_output=True,
            )

        self.assertEqual(completed.returncode, 2)
        self.assertIn("cannot retain HTML", completed.stderr)
        self.assertNotIn("database read-through is disabled", completed.stderr)

    def test_production_auto_render_failure_removes_intermediate_html(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            args = argparse.Namespace(
                dry_run=False,
                fixture=None,
                keep_html=False,
                render="auto",
                output_dir=tmpdir,
                output_width=750,
                json=True,
                chat_id="chat-1",
            )
            report = daily_case_report.ReportData(
                group_name="group",
                report_title="case file",
                report_date="过去24小时",
                time_range="00:00-23:59",
                member_count=1,
                message_count=0,
                participant_count=0,
                case_count=0,
                suspect_count=0,
                hourly_counts=[0] * 24,
                cases=[],
                suspects=[],
                quote="done",
                highlight="done",
            )
            old_parse_args = daily_case_report._parse_args
            old_build_report = daily_case_report._build_report
            old_render_png = daily_case_report._render_png
            daily_case_report._parse_args = lambda: args
            daily_case_report._build_report = lambda _args: report
            daily_case_report._render_png = lambda *_args: (_ for _ in ()).throw(
                RuntimeError("missing browser")
            )
            try:
                stderr = io.StringIO()
                with contextlib.redirect_stderr(stderr):
                    code = daily_case_report.main()
            finally:
                daily_case_report._parse_args = old_parse_args
                daily_case_report._build_report = old_build_report
                daily_case_report._render_png = old_render_png

            self.assertEqual(code, 2)
            self.assertIn("PNG rendering skipped", stderr.getvalue())
            self.assertEqual(list(Path(tmpdir).glob("*.html")), [])

    def test_clean_text_removes_mention_token_without_dropping_body(self) -> None:
        self.assertEqual(
            daily_case_report._clean_text("@张三 今天活动几点开始"),
            "今天活动几点开始",
        )
        self.assertEqual(
            daily_case_report._clean_text("请 @zhangsan 看一下报名表"),
            "请 看一下报名表",
        )
        self.assertEqual(
            daily_case_report._clean_text("@张三今天活动几点开始"),
            "@张三今天活动几点开始",
        )

    def test_cluster_cases_sorts_missing_and_aware_times_without_crashing(self) -> None:
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="张三",
                text="活动讨论：开始",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u2",
                sender_name="李四",
                text="活动安排",
                sent_at=None,
                message_kind="text",
            ),
            daily_case_report.ReportMessage(
                id="m3",
                sender_id="u3",
                sender_name="王五",
                text="活动安排",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
            ),
        ]

        cases = daily_case_report._cluster_cases(messages)

        self.assertEqual(len(cases), 1)
        self.assertEqual(cases[0].message_count, 3)

    def test_render_html_has_no_external_font_fetch(self) -> None:
        report = daily_case_report.ReportData(
            group_name="group",
            report_title="case file",
            report_date="2026-08-08",
            time_range="00:00-23:59",
            member_count=1,
            message_count=0,
            participant_count=0,
            case_count=0,
            suspect_count=0,
            hourly_counts=[0] * 24,
            cases=[],
            suspects=[],
            quote="done",
            highlight="done",
        )

        rendered = daily_case_report._render_html(report, 750)

        self.assertNotIn("fonts.googleapis.com", rendered)
        self.assertNotIn("@import", rendered)
        self.assertNotIn("https://", rendered)
        self.assertNotIn("http://", rendered)
        self.assertIn("WX-CLI STYLE DAILY", rendered)

    def test_render_png_uses_absolute_file_uri_for_relative_html_path(self) -> None:
        captured: dict[str, object] = {}

        class FakePage:
            def route(self, *_args):
                pass

            def goto(self, url, wait_until):
                captured["url"] = url
                captured["wait_until"] = wait_until

            def evaluate(self, _script):
                return 120

            def set_viewport_size(self, size):
                captured["viewport"] = size

            def screenshot(self, path, full_page):
                captured["screenshot"] = (path, full_page)

        class FakeBrowser:
            def new_page(self, **_kwargs):
                return FakePage()

            def close(self):
                captured["closed"] = True

        class FakePlaywright:
            chromium = types.SimpleNamespace(launch=lambda: FakeBrowser())

        class FakeSyncPlaywright:
            def __enter__(self):
                return FakePlaywright()

            def __exit__(self, exc_type, exc, tb):
                return False

        old_cwd = Path.cwd()
        old_playwright = sys.modules.get("playwright")
        old_sync_api = sys.modules.get("playwright.sync_api")
        fake_playwright = types.ModuleType("playwright")
        fake_sync_api = types.ModuleType("playwright.sync_api")
        fake_sync_api.sync_playwright = lambda: FakeSyncPlaywright()
        sys.modules["playwright"] = fake_playwright
        sys.modules["playwright.sync_api"] = fake_sync_api
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                os.chdir(tmpdir)
                html_path = Path("reports") / "preview.html"
                html_path.parent.mkdir()
                html_path.write_text("<html></html>", encoding="utf-8")

                daily_case_report._render_png(html_path, Path("reports") / "preview.png", 750)
        finally:
            os.chdir(old_cwd)
            if old_playwright is None:
                sys.modules.pop("playwright", None)
            else:
                sys.modules["playwright"] = old_playwright
            if old_sync_api is None:
                sys.modules.pop("playwright.sync_api", None)
            else:
                sys.modules["playwright.sync_api"] = old_sync_api

        self.assertEqual(captured["wait_until"], "load")
        self.assertTrue(str(captured["url"]).startswith("file:///"))
        self.assertIn("/reports/preview.html", str(captured["url"]))
        self.assertNotIn("file://reports", str(captured["url"]))

    def test_render_png_falls_back_to_pillow_when_playwright_missing(self) -> None:
        calls: dict[str, object] = {}

        class FakeFont:
            pass

        class FakeImage:
            height = 16000

            @classmethod
            def new(cls, mode, size, color):
                calls["new"] = (mode, size, color)
                return cls()

            def paste(self, *_args):
                pass

            def crop(self, box):
                calls["crop"] = box
                return self

            def save(self, path, format):
                calls["save"] = (path, format)
                Path(path).write_bytes(b"fake-png")

        class FakeDraw:
            def __init__(self, _image):
                pass

            def textbbox(self, _pos, text, font=None):
                return (0, 0, len(str(text)) * 8, 18)

            def rectangle(self, *_args, **_kwargs):
                pass

            def text(self, *_args, **_kwargs):
                pass

        fake_pil = types.ModuleType("PIL")
        fake_image_module = types.ModuleType("PIL.Image")
        fake_image_draw = types.ModuleType("PIL.ImageDraw")
        fake_image_font = types.ModuleType("PIL.ImageFont")
        fake_image_module.new = FakeImage.new
        fake_image_draw.Draw = lambda image: FakeDraw(image)
        fake_image_font.truetype = lambda path, size: FakeFont()
        fake_image_font.load_default = lambda: FakeFont()

        old_playwright = sys.modules.get("playwright")
        old_sync_api = sys.modules.get("playwright.sync_api")
        old_pil = sys.modules.get("PIL")
        old_image = sys.modules.get("PIL.Image")
        old_draw = sys.modules.get("PIL.ImageDraw")
        old_font = sys.modules.get("PIL.ImageFont")
        sys.modules.pop("playwright", None)
        sys.modules.pop("playwright.sync_api", None)
        sys.modules["PIL"] = fake_pil
        sys.modules["PIL.Image"] = fake_image_module
        sys.modules["PIL.ImageDraw"] = fake_image_draw
        sys.modules["PIL.ImageFont"] = fake_image_font
        report = daily_case_report.ReportData(
            group_name="group",
            report_title="daily",
            report_date="2026-08-08",
            time_range="00:00-23:59",
            member_count=1,
            message_count=1,
            participant_count=1,
            case_count=0,
            suspect_count=0,
            hourly_counts=[0] * 24,
            cases=[],
            suspects=[],
            quote="done",
            highlight="done",
            headline="图片优先",
            subtitle="没有浏览器也要能走 Pillow 兜底",
            opening="",
            characters=[],
            quotes=[],
            tomorrow_clues=["继续观察"],
        )
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                output_path = Path(tmpdir) / "report.png"
                daily_case_report._render_png(Path("missing.html"), output_path, 750, report)
                self.assertEqual(output_path.read_bytes(), b"fake-png")
        finally:
            for name, old in [
                ("playwright", old_playwright),
                ("playwright.sync_api", old_sync_api),
                ("PIL", old_pil),
                ("PIL.Image", old_image),
                ("PIL.ImageDraw", old_draw),
                ("PIL.ImageFont", old_font),
            ]:
                if old is None:
                    sys.modules.pop(name, None)
                else:
                    sys.modules[name] = old

        self.assertEqual(calls["save"][1], "PNG")

    def test_render_html_mode_returns_existing_html_deliverable(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            env = os.environ.copy()
            env["PYTHONDONTWRITEBYTECODE"] = "1"
            completed = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--dry-run",
                    "--render",
                    "html",
                    "--json",
                    "--output-dir",
                    tmpdir,
                    "--date",
                    "2026-08-08",
                ],
                check=True,
                env=env,
                text=True,
                capture_output=True,
            )

            result = json.loads(completed.stdout)
            deliverable_path = Path(result["deliverable_path"])

            self.assertTrue(deliverable_path.is_file())
            self.assertEqual(result["html_path"], str(deliverable_path))
            self.assertIsNone(result["png_path"])
            self.assertTrue(Path(result["draft_bundle_path"]).is_file())
            self.assertTrue(result["public_output_style"]["image_first_delivery"])
            self.assertFalse(result["public_output_style"]["pdf_default_delivery"])
            self.assertTrue(result["public_output_style"]["storyline_first_output"])
            self.assertTrue(result["privacy_flags"]["stable_identity_grouping"])
            self.assertIn(str(deliverable_path), result["operator_review_message"])

    def test_character_sketches_group_by_person_id_before_display_name(self) -> None:
        person_a = "00000000-0000-0000-0000-0000000000aa"
        person_b = "00000000-0000-0000-0000-0000000000bb"
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_person_id=person_a,
                sender_name="同名",
                text="活动接龙：我来发起今晚讨论",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u2",
                sender_person_id=person_b,
                sender_name="同名",
                text="资源分享：我补一个完全不同的话题",
                sent_at=datetime(2026, 8, 8, 10, 0, tzinfo=timezone.utc),
                message_kind="text",
            ),
        ]
        memory = {
            person_a: [
                daily_case_report.CreativeMemorySignal(
                    person_id=person_a,
                    label="活动发起人",
                    fact_type="creative_profile",
                    count=3,
                    last_seen="2026-08-07",
                    public_safe=True,
                )
            ],
            person_b: [
                daily_case_report.CreativeMemorySignal(
                    person_id=person_b,
                    label="资料投喂者",
                    fact_type="creative_profile",
                    count=5,
                    last_seen="2026-08-07",
                    public_safe=True,
                )
            ],
        }

        sketches = daily_case_report._compute_characters(messages, memory)

        self.assertEqual(len(sketches), 2)
        self.assertEqual({item.message_count for item in sketches}, {1})
        self.assertTrue(any("活动发起人" in item.memory_line for item in sketches))
        self.assertTrue(any("资料投喂者" in item.memory_line for item in sketches))

    def test_private_memory_stays_out_of_public_image_line(self) -> None:
        person_id = "00000000-0000-0000-0000-0000000000cc"
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_person_id=person_id,
                sender_name="成员",
                text="今天我来补一个活动复盘",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
            )
        ]
        memory = {
            person_id: [
                daily_case_report.CreativeMemorySignal(
                    person_id=person_id,
                    label="不可公开标签",
                    fact_type="creative_profile",
                    count=7,
                    last_seen="2026-08-07",
                    public_safe=False,
                )
            ]
        }

        sketch = daily_case_report._compute_characters(messages, memory)[0]

        self.assertIn("私有人物画像候选 7 条", sketch.memory_line)
        self.assertNotIn("不可公开标签", sketch.memory_line)


if __name__ == "__main__":
    unittest.main()
