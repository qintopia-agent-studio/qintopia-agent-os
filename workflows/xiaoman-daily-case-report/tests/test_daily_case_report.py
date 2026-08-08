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
            self.assertIn(str(deliverable_path), result["operator_review_message"])


if __name__ == "__main__":
    unittest.main()
