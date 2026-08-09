#!/usr/bin/env python3
from __future__ import annotations

import argparse
import builtins
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
        self.assertIn("m.message_kind = 'text'", captured["sql"])
        self.assertIn("NULLIF(BTRIM(m.text), '') IS NOT NULL", captured["sql"])
        self.assertEqual(messages[0].sent_at, report_time)

    def test_fetch_messages_psql_fallback_uses_pgdatabase_not_command_args(self) -> None:
        report_time = datetime(2026, 8, 8, 9, 30, tzinfo=timezone.utc)
        captured: dict[str, object] = {}
        old_psql_override = os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PSQL")
        os.environ["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PSQL"] = "/tmp/not-reviewed-psql"

        def fake_run(args, *, env, **_kwargs):
            captured["args"] = args
            captured["env"] = env
            return types.SimpleNamespace(
                returncode=0,
                stdout=json.dumps(
                    [
                        {
                            "id": "m1",
                            "sender_id": "u1",
                            "sender_name": "张三",
                            "text": "活动安排",
                            "message_kind": "text",
                            "report_time": report_time.isoformat(),
                        }
                    ]
                ),
                stderr="",
            )

        old_run = daily_case_report.subprocess.run
        daily_case_report.subprocess.run = fake_run
        try:
            messages = daily_case_report._fetch_messages_with_psql(
                "postgresql://user:pass@db.example/qintopia",
                "chat-1",
                report_time,
                report_time + timedelta(hours=1),
            )
        finally:
            daily_case_report.subprocess.run = old_run
            if old_psql_override is None:
                os.environ.pop("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PSQL", None)
            else:
                os.environ["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PSQL"] = old_psql_override

        self.assertEqual(messages[0].sent_at, report_time)
        self.assertEqual(captured["args"][0], "/usr/bin/psql")
        self.assertEqual(
            captured["env"]["PGDATABASE"],
            "postgresql://user:pass@db.example/qintopia",
        )
        self.assertNotIn("postgresql://user:pass@db.example/qintopia", captured["args"])
        self.assertIn("--set", captured["args"])

    def test_fetch_messages_psql_failure_does_not_echo_database_url(self) -> None:
        def fake_run(*_args, **_kwargs):
            return types.SimpleNamespace(
                returncode=1,
                stdout="",
                stderr="psql: error: postgresql://user:pass@db.example/qintopia failed",
            )

        old_run = daily_case_report.subprocess.run
        daily_case_report.subprocess.run = fake_run
        try:
            with self.assertRaisesRegex(RuntimeError, "^message store query failed$"):
                daily_case_report._fetch_messages_with_psql(
                    "postgresql://user:pass@db.example/qintopia",
                    "chat-1",
                    datetime(2026, 8, 8, tzinfo=timezone.utc),
                    datetime(2026, 8, 9, tzinfo=timezone.utc),
                )
        finally:
            daily_case_report.subprocess.run = old_run

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

    def test_render_png_defaults_to_png_image_format_without_overriding_explicit_format(
        self,
    ) -> None:
        old_argv = sys.argv
        try:
            sys.argv = [str(SCRIPT), "--dry-run", "--render", "png"]
            args = daily_case_report._parse_args()
            self.assertEqual(args.image_format, "png")

            sys.argv = [
                str(SCRIPT),
                "--dry-run",
                "--render",
                "png",
                "--image-format",
                "jpeg",
            ]
            args = daily_case_report._parse_args()
            self.assertEqual(args.image_format, "jpeg")
        finally:
            sys.argv = old_argv

    def test_main_legacy_render_png_defaults_to_png_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            args = argparse.Namespace(
                dry_run=True,
                fixture=None,
                keep_html=False,
                render="png",
                output_dir=tmpdir,
                output_width=750,
                image_format=None,
                json=True,
                chat_id="chat-1",
                date="2026-08-08",
                timezone="Asia/Shanghai",
                group_name="group",
                report_title="case file",
            )
            old_parse_args = daily_case_report._parse_args
            old_render_image = daily_case_report._render_image

            def fake_render_image(_html_path, output_path, _width, image_format, *_args):
                self.assertEqual(image_format, "png")
                Path(output_path).write_bytes(b"main-fixture-png")

            daily_case_report._parse_args = lambda: args
            daily_case_report._render_image = fake_render_image
            try:
                stdout = io.StringIO()
                with contextlib.redirect_stdout(stdout):
                    code = daily_case_report.main()
            finally:
                daily_case_report._parse_args = old_parse_args
                daily_case_report._render_image = old_render_image

            self.assertEqual(code, 0)
            result = json.loads(stdout.getvalue())
            self.assertEqual(result["image_format"], "png")
            self.assertEqual(result["image_mime_type"], "image/png")
            self.assertTrue(result["image_path"].endswith(".png"))
            self.assertEqual(result["png_path"], result["image_path"])
            self.assertEqual(result["artifact_candidate"]["mime_type"], "image/png")
            self.assertIsNone(result["artifact_candidate"]["render"]["jpeg_quality"])

    def test_production_auto_render_failure_removes_intermediate_html(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            args = argparse.Namespace(
                dry_run=False,
                fixture=None,
                keep_html=False,
                render="auto",
                output_dir=tmpdir,
                output_width=750,
                image_format="jpeg",
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
            old_render_image = daily_case_report._render_image
            daily_case_report._parse_args = lambda: args
            daily_case_report._build_report = lambda _args: report
            daily_case_report._render_image = lambda *_args: (_ for _ in ()).throw(
                RuntimeError("missing browser")
            )
            try:
                stderr = io.StringIO()
                with contextlib.redirect_stderr(stderr):
                    code = daily_case_report.main()
            finally:
                daily_case_report._parse_args = old_parse_args
                daily_case_report._build_report = old_build_report
                daily_case_report._render_image = old_render_image

            self.assertEqual(code, 2)
            self.assertIn("image rendering skipped", stderr.getvalue())
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

    def test_render_image_uses_absolute_file_uri_for_relative_html_path(self) -> None:
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

            def screenshot(self, **kwargs):
                captured["screenshot"] = kwargs

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

                daily_case_report._render_image(
                    html_path,
                    Path("reports") / "preview.jpg",
                    750,
                    "jpeg",
                )
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
        self.assertEqual(captured["screenshot"]["type"], "jpeg")
        self.assertEqual(captured["screenshot"]["quality"], 92)

    def test_render_image_falls_back_to_pillow_when_playwright_is_missing(self) -> None:
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
        old_import = builtins.__import__
        old_pillow = daily_case_report._render_image_with_pillow

        def fake_import(name, *args, **kwargs):
            if name.startswith("playwright"):
                raise ImportError(name)
            return old_import(name, *args, **kwargs)

        def fake_pillow(_report, output_path, _width, image_format):
            self.assertEqual(image_format, "jpeg")
            Path(output_path).write_bytes(b"pillow-jpeg")

        builtins.__import__ = fake_import
        daily_case_report._render_image_with_pillow = fake_pillow
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                html_path = Path(tmpdir) / "preview.html"
                output_path = Path(tmpdir) / "preview.jpg"
                html_path.write_text("<html></html>", encoding="utf-8")

                daily_case_report._render_image(html_path, output_path, 750, "jpeg", report)

                self.assertEqual(output_path.read_bytes(), b"pillow-jpeg")
        finally:
            builtins.__import__ = old_import
            daily_case_report._render_image_with_pillow = old_pillow

    def test_render_image_falls_back_to_pillow_when_browser_is_missing(self) -> None:
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

        class FakePlaywright:
            chromium = types.SimpleNamespace(launch=lambda: (_ for _ in ()).throw(RuntimeError("no browser")))

        class FakeSyncPlaywright:
            def __enter__(self):
                return FakePlaywright()

            def __exit__(self, exc_type, exc, tb):
                return False

        old_playwright = sys.modules.get("playwright")
        old_sync_api = sys.modules.get("playwright.sync_api")
        old_pillow = daily_case_report._render_image_with_pillow
        fake_playwright = types.ModuleType("playwright")
        fake_sync_api = types.ModuleType("playwright.sync_api")
        fake_sync_api.sync_playwright = lambda: FakeSyncPlaywright()
        sys.modules["playwright"] = fake_playwright
        sys.modules["playwright.sync_api"] = fake_sync_api

        def fake_pillow(_report, output_path, _width, image_format):
            self.assertEqual(image_format, "jpeg")
            Path(output_path).write_bytes(b"pillow-jpeg")

        daily_case_report._render_image_with_pillow = fake_pillow
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                html_path = Path(tmpdir) / "preview.html"
                output_path = Path(tmpdir) / "preview.jpg"
                html_path.write_text("<html></html>", encoding="utf-8")

                daily_case_report._render_image(html_path, output_path, 750, "jpeg", report)

                self.assertEqual(output_path.read_bytes(), b"pillow-jpeg")
        finally:
            daily_case_report._render_image_with_pillow = old_pillow
            if old_playwright is None:
                sys.modules.pop("playwright", None)
            else:
                sys.modules["playwright"] = old_playwright
            if old_sync_api is None:
                sys.modules.pop("playwright.sync_api", None)
            else:
                sys.modules["playwright.sync_api"] = old_sync_api

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
            self.assertIsNone(result["image_path"])
            self.assertIsNone(result["artifact_candidate"])
            self.assertFalse(result["requires_human_confirmation"])
            self.assertFalse(result["auto_publish_ready"])
            self.assertNotIn("回复「发」", result["operator_review_message"])
            self.assertIn(str(deliverable_path), result["operator_review_message"])

    def test_result_json_reports_image_artifact_candidate_identity(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            image_path = Path(tmpdir) / "daily-report.jpg"
            image_path.write_bytes(b"fixture-jpeg-bytes")
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
                window_start="2026-08-07T07:45:00+08:00",
                window_end="2026-08-08T07:45:00+08:00",
                timezone="Asia/Shanghai",
            )

            result = daily_case_report._result_json(
                report,
                image_path,
                image_path,
                "jpeg",
                output_width=750,
                source_chat_id="chat-1",
            )

            self.assertEqual(result["image_path"], str(image_path))
            self.assertEqual(result["image_format"], "jpeg")
            self.assertEqual(result["image_mime_type"], "image/jpeg")
            self.assertIsNone(result["png_path"])
            artifact = result["artifact_candidate"]
            self.assertEqual(artifact["artifact_type"], "generated_image")
            self.assertEqual(artifact["workflow_type"], "daily_case_report")
            self.assertEqual(artifact["mime_type"], "image/jpeg")
            self.assertEqual(artifact["filename"], "daily-report.jpg")
            self.assertEqual(artifact["byte_size"], len(b"fixture-jpeg-bytes"))
            self.assertEqual(artifact["template_version"], "xiaoman-daily-case-report-v1")
            self.assertEqual(artifact["render"]["image_format"], "jpeg")
            self.assertEqual(artifact["render"]["width"], 750)
            self.assertEqual(artifact["render"]["jpeg_quality"], 92)
            self.assertEqual(artifact["report_window"]["start"], "2026-08-07T07:45:00+08:00")
            self.assertEqual(artifact["report_window"]["end"], "2026-08-08T07:45:00+08:00")
            self.assertEqual(artifact["report_window"]["timezone"], "Asia/Shanghai")
            self.assertEqual(artifact["source_chat_ref"]["kind"], "sha256")
            self.assertRegex(artifact["source_chat_ref"]["value"], r"^sha256:[0-9a-f]{64}$")
            self.assertRegex(artifact["content_hash"], r"^sha256:[0-9a-f]{64}$")
            self.assertRegex(artifact["file_md5"], r"^[0-9a-f]{32}$")

    def test_main_reports_jpeg_artifact_candidate_after_successful_render(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            args = argparse.Namespace(
                dry_run=True,
                fixture=None,
                keep_html=False,
                render="image",
                output_dir=tmpdir,
                output_width=750,
                image_format="jpeg",
                json=True,
                chat_id="chat-1",
                date="2026-08-08",
                timezone="Asia/Shanghai",
                group_name="group",
                report_title="case file",
            )
            old_parse_args = daily_case_report._parse_args
            old_render_image = daily_case_report._render_image

            def fake_render_image(_html_path, output_path, _width, image_format, *_args):
                self.assertEqual(image_format, "jpeg")
                Path(output_path).write_bytes(b"main-fixture-jpeg")

            daily_case_report._parse_args = lambda: args
            daily_case_report._render_image = fake_render_image
            try:
                stdout = io.StringIO()
                with contextlib.redirect_stdout(stdout):
                    code = daily_case_report.main()
            finally:
                daily_case_report._parse_args = old_parse_args
                daily_case_report._render_image = old_render_image

            self.assertEqual(code, 0)
            result = json.loads(stdout.getvalue())
            self.assertEqual(result["image_format"], "jpeg")
            self.assertEqual(result["image_mime_type"], "image/jpeg")
            self.assertTrue(result["image_path"].endswith(".jpg"))
            self.assertIsNone(result["png_path"])
            self.assertFalse(result["requires_human_confirmation"])
            self.assertFalse(result["auto_publish_ready"])
            self.assertEqual(result["artifact_candidate"]["workflow_type"], "daily_case_report")
            self.assertEqual(result["artifact_candidate"]["render"]["width"], 750)
            self.assertEqual(
                result["artifact_candidate"]["report_window"]["start"],
                "2026-08-08T00:00:00+08:00",
            )
            self.assertNotIn("回复「发」", result["operator_review_message"])


if __name__ == "__main__":
    unittest.main()
