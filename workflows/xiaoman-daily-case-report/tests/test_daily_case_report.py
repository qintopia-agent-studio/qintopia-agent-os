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
                return [
                    (
                        "m1",
                        "u1",
                        "张三",
                        "只有 received_at",
                        "text",
                        report_time,
                        "11111111-1111-1111-1111-111111111111",
                    )
                ]

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
        self.assertEqual(messages[0].person_id, "11111111-1111-1111-1111-111111111111")

    def test_fetch_messages_psql_fallback_uses_fixed_psql_stdin_and_minimal_env(self) -> None:
        report_time = datetime(2026, 8, 8, 9, 30, tzinfo=timezone.utc)
        captured: dict[str, object] = {}
        old_psql_override = os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PSQL")
        os.environ["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PSQL"] = "/tmp/not-reviewed-psql"

        def fake_run(args, *, input, env, text, capture_output, timeout, check):
            captured["args"] = args
            captured["input"] = input
            captured["env"] = env
            self.assertTrue(text)
            self.assertTrue(capture_output)
            self.assertEqual(timeout, 30)
            self.assertFalse(check)
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
                            "sender_person_id": "11111111-1111-1111-1111-111111111111",
                        }
                    ]
                ),
                stderr="",
            )

        old_run = daily_case_report.subprocess.run
        daily_case_report.subprocess.run = fake_run
        try:
            messages = daily_case_report._fetch_messages_with_psql(
                "postgresql://user:p%40ss@db.example:5433/qintopia?sslmode=require",
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
        self.assertEqual(messages[0].person_id, "11111111-1111-1111-1111-111111111111")
        self.assertEqual(captured["args"][0], "/usr/bin/psql")
        self.assertEqual(captured["env"]["PATH"], "/usr/bin:/bin")
        self.assertEqual(captured["env"]["PGDATABASE"], "qintopia")
        self.assertEqual(captured["env"]["PGHOST"], "db.example")
        self.assertEqual(captured["env"]["PGPORT"], "5433")
        self.assertEqual(captured["env"]["PGPASSWORD"], "p@ss")
        self.assertEqual(captured["env"]["PGSSLMODE"], "require")
        self.assertNotIn("postgresql://", " ".join(captured["args"]))
        self.assertNotIn("postgresql://", " ".join(captured["env"].values()))
        self.assertNotIn("--command", captured["args"])
        self.assertIn(":'window_start'::timestamptz", captured["input"])
        self.assertIn(":'window_end'::timestamptz", captured["input"])
        self.assertIn("AND (:'chat_id' = '' OR m.chat_id = :'chat_id')", captured["input"])
        self.assertIn("--set", captured["args"])
        self.assertIn("chat_id=chat-1", captured["args"])

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

    def test_promotional_noise_is_excluded_from_digest_surfaces(self) -> None:
        promo = daily_case_report.ReportMessage(
            id="promo",
            sender_id="seller",
            sender_name="促销号",
            text=(
                "5L:/ 03/03 :9pm 我在抖音挑了喜欢的宝贝，订单在30分钟内有效，"
                "快帮我付个款吧～长按复制此条消息，打开抖音查看详情"
            ),
            sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
            message_kind="text",
        )
        discussion = [
            daily_case_report.ReportMessage(
                id=f"m{idx}",
                sender_id=f"u{idx}",
                sender_name=f"成员{idx}",
                text=f"套利策略复盘：资金分配和风险控制要先讲清楚，避免因为短线波动影响判断 {idx}",
                sent_at=datetime(2026, 8, 8, 10, idx, tzinfo=timezone.utc),
                message_kind="text",
            )
            for idx in range(3)
        ]

        messages = [promo, *discussion]
        filtered = daily_case_report._discussion_messages(messages)
        cases = daily_case_report._cluster_cases(filtered)
        suspects = daily_case_report._compute_suspects(filtered)
        characters = daily_case_report._compute_characters(filtered)
        highlight = daily_case_report._extract_highlight(filtered)

        self.assertNotIn(promo, filtered)
        self.assertNotIn("订单在30分钟内有效", highlight)
        self.assertTrue(cases)
        self.assertNotIn("促销号", {suspect.name for suspect in suspects})
        self.assertNotIn("促销号", {character.name for character in characters})

    def test_character_cards_use_public_daily_role_signals(self) -> None:
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="小雨",
                text="本周活动预告：周六晚 8 点有 AMA，我来收集大家的问题。",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id="11111111-1111-1111-1111-111111111111",
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u1",
                sender_name="小雨",
                text="我把报名表发群里了，大家填一下，明天提醒一次。",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id="11111111-1111-1111-1111-111111111111",
            ),
            daily_case_report.ReportMessage(
                id="m3",
                sender_id="u2",
                sender_name="阿杰",
                text="收到，我准备一个 RWA 合规边界的问题。",
                sent_at=datetime(2026, 8, 8, 9, 6, tzinfo=timezone.utc),
                message_kind="text",
            ),
        ]

        characters = daily_case_report._compute_characters(messages)

        self.assertTrue(characters)
        self.assertEqual(characters[0].name, "小雨")
        self.assertEqual(characters[0].role_label, "活动推进者")
        self.assertIn("活动预告", characters[0].evidence)
        self.assertGreaterEqual(characters[0].topic_count, 1)

    def test_character_cards_use_long_term_memory_counts_without_fact_text(self) -> None:
        person_id = "11111111-1111-1111-1111-111111111111"
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="小雨",
                text="今天活动我来提醒大家报名。",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id=person_id,
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u1",
                sender_name="小雨",
                text="活动表单也同步到群里了。",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id=person_id,
            ),
        ]
        memory = {
            person_id: daily_case_report.CharacterMemory(
                person_id=person_id,
                recent_fact_count=7,
                lifetime_fact_count=18,
                dominant_role_label="活动推进者",
            )
        }

        characters = daily_case_report._compute_characters(messages, memory)

        self.assertEqual(characters[0].memory_label, "近90天 7 次角色复现 · 长期偏「活动推进者」")
        self.assertEqual(characters[0].memory_weight_label, "近90天稳定复现 · 长期线索可用")
        self.assertIn("可作为「活动推进者」连续出场回调", characters[0].meme_seed)
        self.assertIn("稳定复现", characters[0].arc_label)
        self.assertEqual(characters[0].profile_upgrade_status, "eligible_for_review")
        self.assertGreaterEqual(characters[0].profile_evidence_count, 2)
        self.assertIn("daily_character_note:", characters[0].evidence_anchor)
        self.assertNotIn("fact_text", characters[0].memory_label)
        self.assertNotIn("fact_text", characters[0].memory_weight_label)

    def test_character_cards_reuse_reviewed_creative_profiles_for_callbacks(self) -> None:
        person_id = "11111111-1111-1111-1111-111111111111"
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="小雨",
                text="今天活动报名我来提醒，报名表也一起同步。",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id=person_id,
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u1",
                sender_name="小雨",
                text="活动问题收集放到表单里，晚上再提醒一次。",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id=person_id,
            ),
        ]
        creative_memory = {
            person_id: daily_case_report.CreativeProfileMemory(
                person_id=person_id,
                role_label="长期活动导演",
                story_function="把活动线串成连续剧",
                daily_arc="上周铺垫，这周继续推进活动主线",
                memory_weight_label="已审核跨日回调 · 长期线索可用",
                meme_seed="小雨又来收网了",
                callback_hint="可以回看她连续三次把活动从闲聊推到报名",
                recurrence_evidence_count=5,
            )
        }

        characters = daily_case_report._compute_characters(
            messages,
            {},
            creative_memory,
        )
        universe = daily_case_report._build_character_universe(
            [],
            [],
            characters,
            "2026年08月08日",
        )
        report = daily_case_report.ReportData(
            group_name="group",
            report_title="case file",
            report_date="2026-08-08",
            time_range="00:00-23:59",
            member_count=1,
            message_count=2,
            participant_count=1,
            case_count=0,
            suspect_count=0,
            hourly_counts=[0] * 24,
            cases=[],
            suspects=[],
            highlight=None,
            character_count=len(characters),
            characters=characters,
            character_universe=universe,
        )
        quote_map = daily_case_report._build_quote_map(report)
        wiki_bundle = daily_case_report._build_wiki_bundle(report, quote_map)
        draft_bundle = daily_case_report._build_draft_bundle(report, quote_map, wiki_bundle)
        run_manifest = daily_case_report._build_run_manifest(
            report,
            quote_map,
            wiki_bundle,
            draft_bundle,
        )

        self.assertEqual(characters[0].role_label, "活动推进者")
        self.assertEqual(characters[0].story_function, "把活动线串成连续剧")
        self.assertEqual(characters[0].arc_label, "上周铺垫，这周继续推进活动主线")
        self.assertEqual(characters[0].meme_seed, "小雨又来收网了")
        self.assertEqual(characters[0].expressive_label, "")
        self.assertEqual(characters[0].creative_profile_status, "active_reviewed")
        self.assertIn("已审核创意画像", characters[0].memory_label)
        self.assertFalse(characters[0].member_fact_memory_used)
        self.assertTrue(run_manifest["inputs"]["reviewed_creative_profiles_used"])
        self.assertFalse(run_manifest["inputs"]["long_term_member_facts_used"])
        self.assertEqual(draft_bundle["schema_version"], "xiaoman-daily-draft-bundle-v1")
        self.assertEqual(
            draft_bundle["roast_digest"]["status"],
            "candidate_requires_owner_review",
        )
        self.assertTrue(draft_bundle["storyline_memory"]["lookback_callbacks"])
        self.assertGreater(
            run_manifest["counts"]["draft_lookback_callback_count"],
            0,
        )
        self.assertEqual(universe["people"][0]["creative_profile_status"], "active_reviewed")
        self.assertNotIn(person_id, json.dumps(universe, ensure_ascii=False))
        self.assertNotIn("profile_text", json.dumps(universe, ensure_ascii=False))

    def test_creative_profile_rows_keep_only_safe_reviewed_fields(self) -> None:
        memory = daily_case_report._creative_profile_memory_from_rows(
            [
                (
                    "11111111-1111-1111-1111-111111111111",
                    {
                        "role_label": "长期活动导演",
                        "story_function": "串起活动线",
                    },
                    {
                        "daily_arc": "profile_text: should not leak",
                        "memory_weight_label": "已审核跨日回调",
                        "meme_seed": "小雨收网",
                        "callback_hint": "fact_text should not leak",
                        "public_expressive_labels": {
                            "public_surface_allowed": True,
                            "review_status": "reviewed",
                            "roast_label": "收网导演",
                        },
                        "evidence_anchor": "daily_character_note:person-safe-key",
                        "recurrence_evidence_count": 4,
                    },
                )
            ]
        )

        profile = memory["11111111-1111-1111-1111-111111111111"]
        self.assertEqual(profile.role_label, "长期活动导演")
        self.assertEqual(profile.story_function, "串起活动线")
        self.assertEqual(profile.daily_arc, "")
        self.assertEqual(profile.callback_hint, "")
        self.assertEqual(profile.meme_seed, "小雨收网")
        self.assertEqual(profile.expressive_label, "收网导演")
        self.assertEqual(profile.recurrence_evidence_count, 4)

    def test_unreviewed_expressive_labels_do_not_enter_public_memory(self) -> None:
        memory = daily_case_report._creative_profile_memory_from_rows(
            [
                (
                    "11111111-1111-1111-1111-111111111111",
                    {"role_label": "长期活动导演"},
                    {
                        "meme_seed": "小雨收网",
                        "public_expressive_labels": {
                            "public_surface_allowed": True,
                            "review_status": "candidate",
                            "roast_label": "未经审核外号",
                        },
                    },
                )
            ]
        )

        self.assertEqual(memory["11111111-1111-1111-1111-111111111111"].expressive_label, "")

    def test_character_profile_candidate_keeps_single_day_signal_as_daily_note(self) -> None:
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="小雨",
                text="活动报名我来提醒大家。",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id="11111111-1111-1111-1111-111111111111",
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u1",
                sender_name="小雨",
                text="活动表单同步一下。",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id="11111111-1111-1111-1111-111111111111",
            ),
        ]

        characters = daily_case_report._compute_characters(messages)
        universe = daily_case_report._build_character_universe(
            [],
            [],
            characters,
            "2026年08月08日",
        )
        candidate = universe["creative_profile_candidates"][0]

        self.assertEqual(characters[0].profile_upgrade_status, "daily_note_only")
        self.assertEqual(candidate["profile_upgrade_status"], "daily_note_only")
        self.assertFalse(candidate["minimum_recurrence_met"])
        self.assertEqual(candidate["recurrence_evidence_count"], 1)
        self.assertIn("不能升级为长期人物画像", candidate["blocked_reason"])
        self.assertFalse(candidate["public_surface_allowed"])

    def test_character_cards_do_not_merge_same_display_name_people(self) -> None:
        first_person_id = "11111111-1111-1111-1111-111111111111"
        second_person_id = "22222222-2222-2222-2222-222222222222"
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="小雨",
                text="今晚活动我来提醒大家报名。",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id=first_person_id,
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u1",
                sender_name="小雨",
                text="报名表和活动问题收集我都同步一下。",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id=first_person_id,
            ),
            daily_case_report.ReportMessage(
                id="m3",
                sender_id="u2",
                sender_name="小雨",
                text="我来整理今天的内容复盘和案例素材。",
                sent_at=datetime(2026, 8, 8, 10, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id=second_person_id,
            ),
            daily_case_report.ReportMessage(
                id="m4",
                sender_id="u2",
                sender_name="小雨",
                text="复盘里会把内容结构和故事线标出来。",
                sent_at=datetime(2026, 8, 8, 10, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id=second_person_id,
            ),
        ]
        memory = {
            first_person_id: daily_case_report.CharacterMemory(
                person_id=first_person_id,
                recent_fact_count=7,
                lifetime_fact_count=18,
                dominant_role_label="活动推进者",
            )
        }

        characters = daily_case_report._compute_characters(messages, memory)
        universe = daily_case_report._build_character_universe([], [], characters, "2026年08月08日")

        self.assertEqual([character.name for character in characters].count("小雨"), 2)
        self.assertEqual([character.message_count for character in characters], [2, 2])
        self.assertEqual(
            sum(1 for character in characters if "长期偏「活动推进者」" in character.memory_label),
            1,
        )
        people_keys = [person["key"] for person in universe["people"]]
        self.assertEqual(len(people_keys), len(set(people_keys)))
        self.assertNotIn(first_person_id, json.dumps(universe, ensure_ascii=False))
        self.assertNotIn(second_person_id, json.dumps(universe, ensure_ascii=False))

    def test_character_memory_rows_map_only_allowed_role_labels(self) -> None:
        memory = daily_case_report._character_memory_from_rows(
            [
                (
                    "11111111-1111-1111-1111-111111111111",
                    3,
                    12,
                    "content_story_lead",
                )
            ]
        )

        self.assertEqual(
            memory["11111111-1111-1111-1111-111111111111"].dominant_role_label,
            "故事线雷达",
        )
        self.assertEqual(
            memory["11111111-1111-1111-1111-111111111111"].memory_weight_label,
            "近90天偶发复现 · 长期线索可用",
        )
        self.assertIn(
            "轻量回看点",
            memory["11111111-1111-1111-1111-111111111111"].callback_seed,
        )

    def test_build_report_keeps_latest_messages_when_character_memory_fails(self) -> None:
        args = argparse.Namespace(
            dry_run=False,
            fixture=None,
            chat_id="chat-1",
            date="2026-08-08",
            timezone="Asia/Shanghai",
            group_name="group",
            report_title="case file",
        )
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="小雨",
                text="活动预告：今晚我来收集大家的问题。",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id="11111111-1111-1111-1111-111111111111",
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u1",
                sender_name="小雨",
                text="报名表我也同步到群里，明天再提醒一次。",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id="11111111-1111-1111-1111-111111111111",
            ),
        ]
        old_require = daily_case_report._require_read_through
        old_fetch_messages = daily_case_report._fetch_messages
        old_fetch_memory = daily_case_report._fetch_character_memory
        old_fetch_creative_memory = daily_case_report._fetch_creative_profile_memory
        daily_case_report._require_read_through = lambda: True
        daily_case_report._fetch_messages = lambda *_args: messages
        daily_case_report._fetch_character_memory = lambda *_args: (_ for _ in ()).throw(
            RuntimeError("member profile memory query failed")
        )
        daily_case_report._fetch_creative_profile_memory = lambda *_args: (_ for _ in ()).throw(
            RuntimeError("creative profile memory query failed")
        )
        try:
            report = daily_case_report._build_report(args)
        finally:
            daily_case_report._require_read_through = old_require
            daily_case_report._fetch_messages = old_fetch_messages
            daily_case_report._fetch_character_memory = old_fetch_memory
            daily_case_report._fetch_creative_profile_memory = old_fetch_creative_memory

        self.assertEqual(report.message_count, 2)
        self.assertEqual(report.character_count, 1)
        self.assertEqual(report.characters[0].name, "小雨")
        self.assertEqual(report.characters[0].memory_label, "")
        self.assertEqual(report.characters[0].creative_profile_status, "")

    def test_character_universe_uses_curated_second_pass_nodes(self) -> None:
        case = daily_case_report.CaseCard(
            case_no="CASE 01",
            title="活动讨论",
            time_label="10:00-11:00",
            summary="3 条消息，2 人参与",
            bullets=["活动报名节奏已经确认", "明天提醒一次"],
            message_count=3,
            participant_count=2,
            color_bg="#fff0a6",
            color_text="#111111",
            top_speaker="小雨",
        )
        topic = daily_case_report.HotTopic(
            rank=1,
            keyword="活动报名",
            message_count=3,
            participant_count=2,
        )
        character = daily_case_report.CharacterCard(
            rank=1,
            name="小雨",
            role_label="活动推进者",
            one_liner="把松散聊天推成下一步行动",
            evidence="活动报名节奏已经确认",
            message_count=2,
            topic_count=1,
            memory_label="近90天 7 次角色复现 · 长期偏「活动推进者」",
        )

        universe = daily_case_report._build_character_universe(
            [case],
            [topic],
            [character],
            "2026年08月08日",
        )
        report = daily_case_report.ReportData(
            group_name="group",
            report_title="case file",
            report_date="2026-08-08",
            time_range="00:00-23:59",
            member_count=2,
            message_count=3,
            participant_count=2,
            case_count=1,
            suspect_count=0,
            hourly_counts=[0] * 24,
            cases=[case],
            suspects=[],
            highlight=None,
            hot_topics=[topic],
            character_count=1,
            characters=[character],
            character_universe=universe,
        )

        self.assertEqual(universe["schema_version"], "xiaoman-character-universe-v1")
        self.assertEqual(universe["source"], "daily_case_report_second_pass")
        self.assertFalse(universe["raw_messages_included"])
        self.assertFalse(universe["profile_fact_text_included"])
        self.assertEqual(universe["people"][0]["label"], "小雨")
        self.assertEqual(
            universe["creative_profile_candidate_policy"]["profile_kind"],
            "creative_profile",
        )
        self.assertFalse(
            universe["creative_profile_candidate_policy"]["writes_member_profile_snapshots"]
        )
        self.assertFalse(
            universe["creative_profile_candidate_policy"]["public_surface_allowed"]
        )
        self.assertEqual(
            universe["creative_profile_candidates"][0]["profile_kind"],
            "creative_profile",
        )
        self.assertFalse(
            universe["creative_profile_candidates"][0]["public_surface_allowed"]
        )
        self.assertEqual(
            universe["creative_profile_candidates"][0]["evidence_anchor"],
            "daily_character_note:小雨",
        )
        self.assertEqual(
            universe["creative_profile_candidates"][0]["profile_upgrade_status"],
            "daily_note_only",
        )
        self.assertFalse(
            universe["creative_profile_candidates"][0]["minimum_recurrence_met"]
        )
        self.assertEqual(universe["topics"][0]["label"], "活动报名")
        self.assertEqual(universe["events"][0]["case_no"], "CASE 01")
        self.assertEqual(universe["storyline_candidates"][0]["label"], "活动讨论")
        self.assertTrue(any(edge["relation"] == "appears_in" for edge in universe["edges"]))

        markdown = daily_case_report._render_daily_markdown(report)
        self.assertIn("## 可沉淀故事线", markdown)
        self.assertIn("[[活动讨论]]：3 条消息，2 人参与", markdown)

    def test_character_universe_exports_public_safe_memes_relationships_and_callbacks(self) -> None:
        first_person_id = "11111111-1111-1111-1111-111111111111"
        second_person_id = "22222222-2222-2222-2222-222222222222"
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="小雨",
                text="活动报名我来提醒，活动报名表今晚同步。",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
                person_id=first_person_id,
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u1",
                sender_name="小雨",
                text="活动问题收集也放到报名表里。",
                sent_at=datetime(2026, 8, 8, 9, 5, tzinfo=timezone.utc),
                message_kind="text",
                person_id=first_person_id,
            ),
            daily_case_report.ReportMessage(
                id="m3",
                sender_id="u2",
                sender_name="阿杰",
                text="活动报名我看到了，我补一个 RWA 问题。",
                sent_at=datetime(2026, 8, 8, 9, 6, tzinfo=timezone.utc),
                message_kind="text",
                person_id=second_person_id,
            ),
            daily_case_report.ReportMessage(
                id="m4",
                sender_id="u2",
                sender_name="阿杰",
                text="报名问题可以先按活动主题分组。",
                sent_at=datetime(2026, 8, 8, 9, 7, tzinfo=timezone.utc),
                message_kind="text",
                person_id=second_person_id,
            ),
        ]
        memory = {
            first_person_id: daily_case_report.CharacterMemory(
                person_id=first_person_id,
                recent_fact_count=8,
                lifetime_fact_count=20,
                dominant_role_label="活动推进者",
                recurrence_label="近90天稳定复现",
                depth_label="长期线索可用",
                memory_weight_label="近90天稳定复现 · 长期线索可用",
                callback_seed="可作为「活动推进者」连续出场回调",
            )
        }

        characters = daily_case_report._compute_characters(messages, memory)
        universe = daily_case_report._build_character_universe([], [], characters, "2026年08月08日")
        report = daily_case_report.ReportData(
            group_name="group",
            report_title="case file",
            report_date="2026-08-08",
            time_range="00:00-23:59",
            member_count=2,
            message_count=4,
            participant_count=2,
            case_count=0,
            suspect_count=0,
            hourly_counts=[0] * 24,
            cases=[],
            suspects=[],
            highlight=None,
            character_count=len(characters),
            characters=characters,
            character_universe=universe,
        )

        self.assertTrue(universe["memes"])
        self.assertTrue(universe["callbacks"])
        self.assertTrue(universe["relationships"])
        self.assertTrue(universe["creative_profile_candidates"])
        self.assertTrue(universe["expressive_label_candidates"])
        self.assertFalse(
            any(
                candidate["public_surface_allowed"]
                and candidate["review_status"] != "reviewed"
                for candidate in universe["expressive_label_candidates"]
            )
        )
        self.assertEqual(
            universe["creative_universe_candidates"]["schema_version"],
            "xiaoman-daily-creative-universe-candidates-v1",
        )
        self.assertFalse(universe["creative_universe_candidates"]["public_surface_allowed"])
        self.assertFalse(
            universe["creative_universe_candidates"]["writes_member_profile_snapshots"]
        )
        self.assertGreaterEqual(
            universe["creative_universe_candidates"]["candidate_count"],
            1,
        )
        self.assertTrue(
            universe["creative_universe_candidates"]["candidate_sets"]["cross_day_memes"]
        )
        self.assertTrue(
            universe["creative_universe_candidates"]["candidate_sets"]["relationship_labels"]
        )
        self.assertTrue(
            any(
                candidate["profile_upgrade_status"] == "eligible_for_review"
                for candidate in universe["creative_profile_candidates"]
            )
        )
        self.assertTrue(any(edge["relation"] == "co_discusses_topic" for edge in universe["edges"]))
        self.assertIn("同场关系", daily_case_report._render_html(report, 750))
        markdown = daily_case_report._render_daily_markdown(report)
        self.assertIn("## 同场关系", markdown)
        self.assertIn("## 可审核人物画像候选", markdown)
        self.assertIn("公开话题", markdown)
        serialized = json.dumps(universe, ensure_ascii=False)
        self.assertNotIn(first_person_id, serialized)
        self.assertNotIn(second_person_id, serialized)
        self.assertNotIn("raw profile fact text", serialized)

    def test_private_review_bundle_exports_quote_map_wiki_and_run_manifest(self) -> None:
        person_id = "11111111-1111-1111-1111-111111111111"
        case = daily_case_report.CaseCard(
            case_no="CASE 01",
            title="活动讨论",
            time_label="10:00-11:00",
            summary="3 条消息，2 人参与",
            bullets=["活动报名节奏已经确认", "明天提醒一次"],
            message_count=3,
            participant_count=2,
            color_bg="#fff0a6",
            color_text="#111111",
            top_speaker="小雨",
        )
        topic = daily_case_report.HotTopic(
            rank=1,
            keyword="活动报名",
            message_count=3,
            participant_count=2,
        )
        character = daily_case_report.CharacterCard(
            rank=1,
            name="小雨",
            role_label="活动推进者",
            one_liner="把松散聊天推成下一步行动",
            evidence="活动报名节奏已经确认",
            message_count=2,
            topic_count=1,
            node_key="person-safe-key",
            memory_label="近90天 7 次角色复现 · 长期偏「活动推进者」",
            member_fact_memory_used=True,
            story_function="推进剧情",
            callback_hint="今天不是孤例，可以回看「活动推进者」的长期复现",
            arc_label="长期线索可用，今日再次露出「活动推进者」信号",
            meme_seed="可作为「活动推进者」连续出场回调",
            memory_weight_label="近90天稳定复现 · 长期线索可用",
        )
        universe = daily_case_report._build_character_universe(
            [case],
            [topic],
            [character],
            "2026年08月08日",
        )
        report = daily_case_report.ReportData(
            group_name="group",
            report_title="case file",
            report_date="2026-08-08",
            time_range="00:00-23:59",
            member_count=2,
            message_count=3,
            participant_count=2,
            case_count=1,
            suspect_count=0,
            hourly_counts=[0] * 24,
            cases=[case],
            suspects=[],
            highlight="活动报名节奏已经确认，明天提醒一次",
            hot_topics=[topic],
            character_count=1,
            characters=[character],
            character_universe=universe,
            window_start="2026-08-08T00:00:00+08:00",
            window_end="2026-08-09T00:00:00+08:00",
        )

        quote_map = daily_case_report._build_quote_map(report)
        wiki_bundle = daily_case_report._build_wiki_bundle(report, quote_map)
        draft_bundle = daily_case_report._build_draft_bundle(report, quote_map, wiki_bundle)
        run_manifest = daily_case_report._build_run_manifest(
            report,
            quote_map,
            wiki_bundle,
            draft_bundle,
            source_chat_id="chat-1",
        )
        review_report = daily_case_report._render_review_report(
            report,
            quote_map,
            wiki_bundle,
            draft_bundle,
            run_manifest,
        )

        self.assertEqual(quote_map["schema_version"], "xiaoman-daily-quote-map-v1")
        self.assertGreaterEqual(quote_map["entry_count"], 3)
        self.assertFalse(quote_map["raw_message_rows_included"])
        self.assertFalse(quote_map["profile_fact_text_included"])
        self.assertFalse(quote_map["public_surface_allowed"])
        self.assertTrue(
            any(entry["source_kind"] == "daily_character_note" for entry in quote_map["entries"])
        )
        self.assertEqual(wiki_bundle["schema_version"], "xiaoman-daily-wiki-bundle-v1")
        self.assertEqual(draft_bundle["schema_version"], "xiaoman-daily-draft-bundle-v1")
        self.assertEqual(
            draft_bundle["roast_digest"]["status"],
            "candidate_requires_owner_review",
        )
        ordinary_digest = draft_bundle["ordinary_digest"]
        self.assertEqual(
            ordinary_digest["weather_context"]["status"],
            "omitted_no_reviewed_weather_source",
        )
        self.assertTrue(ordinary_digest["one_sentence_summary"])
        self.assertEqual(ordinary_digest["main_topics"][0]["title"], "活动讨论")
        self.assertEqual(ordinary_digest["main_topics"][0]["message_ids"], [])
        self.assertEqual(ordinary_digest["main_topics"][0]["attachment_pointers"], [])
        self.assertEqual(ordinary_digest["main_topics"][0]["media_links"], [])
        self.assertEqual(
            ordinary_digest["main_topics"][0]["media_notes"]["status"],
            "omitted_no_reviewed_attachment_source",
        )
        self.assertFalse(
            ordinary_digest["main_topics"][0]["media_notes"]["raw_message_payload_read"]
        )
        self.assertEqual(ordinary_digest["people_notes"][0]["role_label"], "活动推进者")
        self.assertGreaterEqual(len(ordinary_digest["local_life_notes"]), 1)
        self.assertEqual(ordinary_digest["local_life_notes"][0]["source"], "CASE 01")
        self.assertTrue(ordinary_digest["risk_items"])
        self.assertGreaterEqual(len(ordinary_digest["candidate_public_topics"]), 1)
        self.assertIn("主要话题", ordinary_digest["section_keys"])
        self.assertIn("候选公众号选题", ordinary_digest["section_keys"])
        self.assertGreaterEqual(
            draft_bundle["counts"]["ordinary_digest_topic_count"],
            1,
        )
        self.assertGreaterEqual(
            draft_bundle["counts"]["ordinary_digest_people_note_count"],
            1,
        )
        self.assertGreaterEqual(
            draft_bundle["counts"]["ordinary_digest_local_life_note_count"],
            1,
        )
        self.assertGreaterEqual(
            draft_bundle["counts"]["ordinary_digest_candidate_public_topic_count"],
            1,
        )
        self.assertGreaterEqual(
            draft_bundle["counts"]["lookback_callback_count"],
            1,
        )
        self.assertEqual(wiki_bundle["counts"]["people"], 1)
        self.assertEqual(wiki_bundle["counts"]["events"], 1)
        self.assertEqual(wiki_bundle["counts"]["storylines"], 1)
        self.assertFalse(wiki_bundle["public_surface_allowed"])
        self.assertEqual(run_manifest["schema_version"], "xiaoman-daily-run-manifest-v1")
        self.assertTrue(run_manifest["inputs"]["latest_chat_records_preserved"])
        self.assertTrue(run_manifest["inputs"]["long_term_member_facts_used"])
        self.assertFalse(run_manifest["inputs"]["long_term_member_fact_text_included"])
        self.assertEqual(
            run_manifest["reference_workshop_steps"]["attachment_index"],
            "omitted_no_reviewed_attachment_source",
        )
        self.assertEqual(
            run_manifest["reference_workshop_steps"]["media_prepare"],
            "omitted_no_reviewed_attachment_source",
        )
        self.assertFalse(run_manifest["reference_workshop_steps"]["raw_message_payload_read"])
        self.assertFalse(
            run_manifest["reference_workshop_steps"]["attachment_public_surface_allowed"]
        )
        self.assertGreaterEqual(run_manifest["counts"]["creative_universe_candidate_count"], 1)
        self.assertGreaterEqual(run_manifest["counts"]["expressive_label_candidate_count"], 1)
        self.assertFalse(run_manifest["privacy"]["profile_fact_text_included"])
        self.assertFalse(run_manifest["privacy"]["creative_profile_public_surface_allowed"])
        self.assertFalse(run_manifest["privacy"]["creative_universe_public_surface_allowed"])
        self.assertFalse(
            run_manifest["privacy"]["unreviewed_expressive_labels_public_surface_allowed"]
        )
        self.assertFalse(run_manifest["privacy"]["raw_message_payload_read"])
        self.assertFalse(run_manifest["privacy"]["attachment_public_surface_allowed"])
        self.assertIn("审核清单", review_report)
        self.assertIn("eligible_for_review", review_report)
        self.assertIn("创作资产候选", review_report)
        self.assertIn("creative_universe_public_surface_allowed=false", review_report)
        self.assertIn("unreviewed_expressive_labels_public_surface_allowed=false", review_report)
        self.assertIn("raw_message_payload_read=false", review_report)
        self.assertIn("attachment_public_surface_allowed=false", review_report)
        self.assertIn("未读取 raw payload", review_report)
        self.assertIn("evidence_count=", review_report)
        self.assertIn("worker-run evidence 只能保留 presence/count/privacy flags", review_report)
        serialized = json.dumps(
            {
                "quote_map": quote_map,
                "wiki_bundle": wiki_bundle,
                "run_manifest": run_manifest,
                "review_report": review_report,
            },
            ensure_ascii=False,
        )
        self.assertNotIn(person_id, serialized)
        self.assertNotIn("raw profile fact text", serialized)

    def test_missing_highlight_is_omitted_instead_of_synthesized(self) -> None:
        self.assertIsNone(
            daily_case_report._extract_highlight([
                daily_case_report.ReportMessage(
                    id="short",
                    sender_id="u1",
                    sender_name="成员",
                    text="收到",
                    sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                    message_kind="text",
                )
            ])
        )

    def test_hot_topics_are_ranked_from_repeated_source_tokens(self) -> None:
        messages = [
            daily_case_report.ReportMessage(
                id=f"m{index}",
                sender_id=f"u{index}",
                sender_name=f"成员{index}",
                text=text,
                sent_at=datetime(2026, 8, 8, 9, index, tzinfo=timezone.utc),
                message_kind="text",
            )
            for index, text in enumerate([
                "我在整理自动化工作流的步骤。",
                "自动化工作流怎么做更稳定？",
                "先复盘自动化工作流，再补充模板。",
                "内容创作遇到卡点时，先做最小版本。",
                "我也想聊内容创作的实际用法。",
            ])
        ]

        topics = daily_case_report._hot_topics(messages)

        self.assertTrue(topics)
        self.assertEqual(topics[0].keyword, "自动化工作流")
        self.assertEqual(topics[0].message_count, 3)
        self.assertEqual(topics[0].participant_count, 3)
        content_topic = next(topic for topic in topics if topic.keyword == "内容创作")
        self.assertEqual(content_topic.message_count, 2)
        self.assertEqual(content_topic.participant_count, 2)

    def test_hot_topics_exclude_repeated_sentence_fragments(self) -> None:
        messages = [
            daily_case_report.ReportMessage(
                id=f"m{index}",
                sender_id=f"u{index}",
                sender_name=f"成员{index}",
                text=text,
                sent_at=datetime(2026, 8, 8, 9, index, tzinfo=timezone.utc),
                message_kind="text",
            )
            for index, text in enumerate([
                "资料发群里了，大家按需查看。",
                "问题表发群里了，欢迎补充。",
            ])
        ]

        topics = daily_case_report._hot_topics(messages)

        self.assertFalse(any("群里" in topic.keyword for topic in topics))

    def test_hot_topics_include_case_storylines_as_wiki_topics(self) -> None:
        messages = daily_case_report._sample_messages(
            datetime(2026, 8, 8, tzinfo=timezone.utc)
        )
        cases = daily_case_report._cluster_cases(messages)

        topics = daily_case_report._hot_topics(messages, cases)
        topic_names = {topic.keyword for topic in topics}

        self.assertIn("资源分享", topic_names)
        self.assertIn("技术求助", topic_names)
        self.assertFalse(any(topic.keyword.startswith("早场") for topic in topics))

    def test_weak_colon_sentence_does_not_capture_later_messages(self) -> None:
        messages = [
            daily_case_report.ReportMessage(
                id="m1",
                sender_id="u1",
                sender_name="张三",
                text="活动讨论：今天报名节奏先对齐一下",
                sent_at=datetime(2026, 8, 8, 9, 0, tzinfo=timezone.utc),
                message_kind="text",
            ),
            daily_case_report.ReportMessage(
                id="m2",
                sender_id="u2",
                sender_name="李四",
                text="我可以负责统计人数",
                sent_at=datetime(2026, 8, 8, 9, 1, tzinfo=timezone.utc),
                message_kind="text",
            ),
            daily_case_report.ReportMessage(
                id="m3",
                sender_id="u3",
                sender_name="王五",
                text="国家现在规定叫：词元，哇喔，这把名字好帅",
                sent_at=datetime(2026, 8, 8, 9, 2, tzinfo=timezone.utc),
                message_kind="text",
            ),
            daily_case_report.ReportMessage(
                id="m4",
                sender_id="u4",
                sender_name="赵六",
                text="后面这句不应该继续算进活动讨论",
                sent_at=datetime(2026, 8, 8, 9, 3, tzinfo=timezone.utc),
                message_kind="text",
            ),
        ]

        clusters = daily_case_report._detect_topic_markers(messages)

        self.assertEqual(list(clusters), ["活动讨论"])
        self.assertEqual([msg.id for msg in clusters["活动讨论"]], ["m1", "m2"])

    def test_weak_keywords_do_not_block_time_bucket_cards(self) -> None:
        messages = []
        for hour in (13, 19):
            for idx in range(3):
                messages.append(
                    daily_case_report.ReportMessage(
                        id=f"{hour}-{idx}",
                        sender_id=f"u{idx}",
                        sender_name=f"成员{idx}",
                        text=f"呲牙 哈哈哈 收到 好的 {hour}-{idx}",
                        sent_at=datetime(2026, 8, 8, hour, idx, tzinfo=timezone.utc),
                        message_kind="text",
                    )
                )

        cases = daily_case_report._cluster_cases(messages)

        self.assertEqual(len(cases), 2)
        self.assertTrue(all("呲牙" not in case.title for case in cases))
        self.assertTrue(all("哈哈" not in case.title for case in cases))
        self.assertTrue(all("呲牙" not in " ".join(case.bullets) for case in cases))
        self.assertTrue(all("哈哈" not in " ".join(case.bullets) for case in cases))

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
            highlight=None,
        )

        rendered = daily_case_report._render_html(report, 750)

        self.assertNotIn("fonts.googleapis.com", rendered)
        self.assertNotIn("@import", rendered)
        self.assertNotIn("https://", rendered)
        self.assertNotIn("http://", rendered)
        self.assertNotIn("今日高亮", rendered)
        self.assertNotIn("COACH'S TIMEOUT", rendered)
        self.assertNotIn("群聊热榜", rendered)

    def test_render_html_uses_character_daily_story_structure(self) -> None:
        report = daily_case_report.ReportData(
            group_name="group",
            report_title="群聊战报",
            report_date="2026-08-08",
            time_range="00:00-23:59",
            member_count=2,
            message_count=3,
            participant_count=2,
            case_count=1,
            suspect_count=1,
            hourly_counts=[0] * 24,
            cases=[daily_case_report.CaseCard(
                case_no="CASE 01",
                title="讨论主题",
                time_label="10:00-11:00",
                summary="2 条消息，2 人参与",
                bullets=["社区活动安排已经确认", "有没有人明天提醒一次？"],
                message_count=2,
                participant_count=2,
                color_bg="#fff0a6",
                color_text="#111111",
                top_speaker="成员",
            )],
            suspects=[daily_case_report.Suspect(
                rank=1,
                name="成员",
                message_count=2,
                word_count=12,
                avatar_emoji="*",
            )],
            highlight="原始群消息",
            hot_topics=[daily_case_report.HotTopic(
                rank=1,
                keyword="讨论主题",
                message_count=2,
                participant_count=2,
            )],
            character_count=1,
            characters=[daily_case_report.CharacterCard(
                rank=1,
                name="成员",
                role_label="活动推进者",
                one_liner="把松散聊天推成下一步行动",
                evidence="活动安排已经确认",
                message_count=2,
                topic_count=1,
            )],
        )

        rendered = daily_case_report._render_html(report, 750)

        self.assertIn("XIAOMAN CHARACTER DAILY", rendered)
        self.assertIn("小满群聊日报", rendered)
        self.assertIn("今日主线", rendered)
        self.assertIn("人物出场表", rendered)
        self.assertIn("DAILY WORKSHOP INDEX", rendered)
        self.assertIn("梗和回调候选", rendered)
        self.assertIn("地点 / 本地生活线索", rendered)
        self.assertIn("待解决问题", rendered)
        self.assertIn("故事线候选", rendered)
        self.assertIn("发言出场榜", rendered)
        self.assertNotIn("XIAOMAN COMMUNITY SCOREBOARD", rendered)
        self.assertNotIn("群聊热榜", rendered)
        self.assertNotIn("今日局势", rendered)
        self.assertNotIn("今日 MVP", rendered)
        self.assertIn("background: #ffd92e", rendered)
        self.assertLess(rendered.index("今日主线"), rendered.index("人物出场表"))
        self.assertLess(rendered.index("DAILY WORKSHOP INDEX"), rendered.index("人物出场表"))
        self.assertLess(rendered.index("人物出场表"), rendered.index("今日台词"))
        self.assertLess(rendered.index("今日台词"), rendered.index("梗和回调候选"))
        self.assertLess(rendered.index("梗和回调候选"), rendered.index("故事线候选"))
        self.assertLess(rendered.index("地点 / 本地生活线索"), rendered.index("故事线候选"))
        self.assertLess(rendered.index("待解决问题"), rendered.index("故事线候选"))
        self.assertLess(rendered.index("故事线候选"), rendered.index("当日素材"))
        self.assertLess(rendered.index("故事线候选"), rendered.index("24H 活跃节奏"))
        self.assertLess(rendered.index("故事线候选"), rendered.index("发言出场榜"))

        markdown = daily_case_report._render_daily_markdown(report)
        self.assertIn("# 小满群聊日报｜2026-08-08｜讨论主题", markdown)
        self.assertIn("## 天气背景", markdown)
        self.assertIn("## 主要话题", markdown)
        self.assertIn("## 今日剧中人", markdown)
        self.assertIn("**成员（活动推进者）**", markdown)
        self.assertIn("## 梗和回调候选", markdown)
        self.assertIn("## 地点 / 本地生活线索", markdown)
        self.assertIn("## 待解决问题", markdown)
        self.assertIn("## 候选公众号选题", markdown)
        self.assertIn("raw_messages_included=false", markdown)
        self.assertIn("profile_fact_text_included=false", markdown)
        style = daily_case_report._public_output_style_contract()
        self.assertEqual(style["schema_version"], "xiaoman-daily-public-output-style-v1")
        self.assertTrue(style["character_daily_layout"])
        self.assertTrue(style["storyline_first"])
        self.assertTrue(style["image_first_delivery"])
        self.assertFalse(style["pdf_default_delivery"])
        self.assertTrue(style["roast_review_boundary"])
        self.assertTrue(style["private_draft_only"])
        self.assertFalse(style["public_surface_contains_private_draft"])

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

    def test_pillow_renderer_keeps_character_daily_story_order(self) -> None:
        captured_text: list[str] = []

        class FakeFont:
            def __init__(self, size: int):
                self.size = size

        class FakeImageObject:
            height = 20000

            def crop(self, _box):
                return self

            def save(self, output_path, format=None, **_kwargs):
                Path(output_path).write_bytes(str(format or "").encode("utf-8"))

        class FakeImageModule(types.SimpleNamespace):
            def new(self, *_args, **_kwargs):
                return FakeImageObject()

        class FakeDraw:
            def text(self, _xy, text, **_kwargs):
                captured_text.append(str(text))

            def textbbox(self, _xy, text, **_kwargs):
                return (0, 0, len(str(text)) * 8, 16)

            def textlength(self, text, **_kwargs):
                return len(str(text)) * 8

            def rectangle(self, *_args, **_kwargs):
                pass

            def rounded_rectangle(self, *_args, **_kwargs):
                pass

            def ellipse(self, *_args, **_kwargs):
                pass

            def line(self, *_args, **_kwargs):
                pass

        fake_pil = types.ModuleType("PIL")
        fake_image = FakeImageModule()
        fake_image_draw = types.SimpleNamespace(Draw=lambda _image: FakeDraw())
        old_pil = sys.modules.get("PIL")
        old_image = sys.modules.get("PIL.Image")
        old_image_draw = sys.modules.get("PIL.ImageDraw")
        old_pil_font = daily_case_report._pil_font
        sys.modules["PIL"] = fake_pil
        sys.modules["PIL.Image"] = fake_image
        sys.modules["PIL.ImageDraw"] = fake_image_draw
        daily_case_report._pil_font = lambda size, **_kwargs: FakeFont(size)
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                report = daily_case_report.ReportData(
                    group_name="group",
                    report_title="群聊战报",
                    report_date="2026-08-08",
                    time_range="00:00-23:59",
                    member_count=2,
                    message_count=3,
                    participant_count=2,
                    case_count=1,
                    suspect_count=1,
                    hourly_counts=[0] * 24,
                    cases=[
                        daily_case_report.CaseCard(
                            case_no="CASE 01",
                            title="讨论主题",
                            time_label="10:00-11:00",
                            summary="3 条消息，2 人参与",
                            bullets=["活动安排已经确认", "有没有人明天提醒一次？"],
                            message_count=3,
                            participant_count=2,
                            color_bg="#fff0a6",
                            color_text="#111111",
                            top_speaker="成员",
                        )
                    ],
                    suspects=[
                        daily_case_report.Suspect(
                            rank=1,
                            name="成员",
                            message_count=2,
                            word_count=12,
                            avatar_emoji="*",
                        )
                    ],
                    highlight="活动安排已经确认",
                    hot_topics=[
                        daily_case_report.HotTopic(
                            rank=1,
                            keyword="讨论主题",
                            message_count=2,
                            participant_count=2,
                        )
                    ],
                    character_count=1,
                    characters=[
                        daily_case_report.CharacterCard(
                            rank=1,
                            name="成员",
                            role_label="活动推进者",
                            one_liner="把松散聊天推成下一步行动",
                            evidence="活动安排已经确认",
                            message_count=2,
                            topic_count=1,
                        )
                    ],
                )
                daily_case_report._render_image_with_pillow(
                    report,
                    Path(tmpdir) / "preview.jpg",
                    750,
                    "jpeg",
                )
        finally:
            daily_case_report._pil_font = old_pil_font
            if old_pil is None:
                sys.modules.pop("PIL", None)
            else:
                sys.modules["PIL"] = old_pil
            if old_image is None:
                sys.modules.pop("PIL.Image", None)
            else:
                sys.modules["PIL.Image"] = old_image
            if old_image_draw is None:
                sys.modules.pop("PIL.ImageDraw", None)
            else:
                sys.modules["PIL.ImageDraw"] = old_image_draw

        rendered = "\n".join(captured_text)
        self.assertIn("人物出场表", rendered)
        self.assertIn("今日台词", rendered)
        self.assertIn("梗和回调候选", rendered)
        self.assertIn("地点 / 本地生活线索", rendered)
        self.assertIn("待解决问题", rendered)
        self.assertIn("故事线候选", rendered)
        self.assertIn("24H 活跃节奏", rendered)
        self.assertLess(rendered.index("人物出场表"), rendered.index("今日台词"))
        self.assertLess(rendered.index("今日台词"), rendered.index("梗和回调候选"))
        self.assertLess(rendered.index("梗和回调候选"), rendered.index("故事线候选"))
        self.assertLess(rendered.index("地点 / 本地生活线索"), rendered.index("故事线候选"))
        self.assertLess(rendered.index("待解决问题"), rendered.index("故事线候选"))
        self.assertLess(rendered.index("故事线候选"), rendered.index("24H 活跃节奏"))

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
            self.assertTrue(Path(result["daily_report_markdown_path"]).is_file())
            self.assertTrue(Path(result["character_universe_path"]).is_file())
            self.assertTrue(Path(result["quote_map_path"]).is_file())
            self.assertTrue(Path(result["wiki_bundle_path"]).is_file())
            self.assertTrue(Path(result["draft_bundle_path"]).is_file())
            self.assertTrue(Path(result["run_manifest_path"]).is_file())
            self.assertTrue(Path(result["review_report_path"]).is_file())
            self.assertTrue(Path(result["creative_profile_review_payload_path"]).is_file())
            universe = json.loads(Path(result["character_universe_path"]).read_text(encoding="utf-8"))
            self.assertEqual(universe, result["character_universe"])
            quote_map = json.loads(Path(result["quote_map_path"]).read_text(encoding="utf-8"))
            wiki_bundle = json.loads(Path(result["wiki_bundle_path"]).read_text(encoding="utf-8"))
            draft_bundle = json.loads(Path(result["draft_bundle_path"]).read_text(encoding="utf-8"))
            run_manifest = json.loads(Path(result["run_manifest_path"]).read_text(encoding="utf-8"))
            review_payload = json.loads(
                Path(result["creative_profile_review_payload_path"]).read_text(encoding="utf-8")
            )
            self.assertEqual(quote_map, result["quote_map"])
            self.assertEqual(wiki_bundle, result["wiki_bundle"])
            self.assertEqual(draft_bundle, result["draft_bundle"])
            self.assertEqual(run_manifest, result["run_manifest"])
            self.assertEqual(draft_bundle["schema_version"], "xiaoman-daily-draft-bundle-v1")
            self.assertFalse(draft_bundle["public_surface_allowed"])
            self.assertFalse(draft_bundle["raw_message_rows_included"])
            self.assertFalse(draft_bundle["profile_fact_text_included"])
            self.assertEqual(
                draft_bundle["roast_digest"]["status"],
                "candidate_requires_owner_review",
            )
            self.assertIn("public_draft", draft_bundle)
            self.assertIn("lookback_callbacks", draft_bundle["storyline_memory"])
            self.assertEqual(review_payload["source"], "xiaoman-daily-creative-profile-review-v1")
            self.assertTrue(review_payload["review_notes"]["person_id_required"])
            self.assertFalse(review_payload["review_notes"]["display_name_binding_allowed"])
            self.assertTrue(
                all(candidate["person_id"] == "" for candidate in review_payload["candidates"])
            )
            self.assertTrue(
                any(
                    candidate["review_decision"] == "pending_review"
                    for candidate in review_payload["candidates"]
                )
            )
            self.assertFalse(universe["raw_messages_included"])
            self.assertFalse(universe["profile_fact_text_included"])
            self.assertFalse(quote_map["raw_message_rows_included"])
            self.assertFalse(quote_map["profile_fact_text_included"])
            self.assertFalse(wiki_bundle["public_surface_allowed"])
            self.assertFalse(run_manifest["privacy"]["profile_fact_text_included"])
            self.assertFalse(run_manifest["inputs"]["long_term_member_facts_used"])
            self.assertFalse(result["private_review_bundle"]["public_surface_allowed"])
            self.assertTrue(result["private_review_bundle"]["review_required"])
            self.assertGreater(result["private_review_bundle"]["quote_map_entry_count"], 0)
            self.assertGreater(
                result["private_review_bundle"]["draft_counts"][
                    "roast_profile_candidate_count"
                ],
                0,
            )
            self.assertEqual(
                result["private_review_bundle"]["creative_profile_review_payload"]["candidate_count"],
                len(review_payload["candidates"]),
            )
            self.assertGreater(
                result["private_review_bundle"]["creative_profile_review_payload"][
                    "pending_review_count"
                ],
                0,
            )
            self.assertEqual(
                result["private_review_bundle"]["creative_profile_review_payload"][
                    "approved_candidate_count"
                ],
                0,
            )
            self.assertFalse(
                result["private_review_bundle"]["creative_profile_review_payload"][
                    "display_name_binding_allowed"
                ]
            )
            self.assertIn("people", universe)
            self.assertIn("events", universe)
            self.assertIn("storyline_candidates", universe)
            self.assertIn("timeline", wiki_bundle)
            style = result["public_output_style"]
            self.assertEqual(style["schema_version"], "xiaoman-daily-public-output-style-v1")
            self.assertTrue(style["character_daily_layout"])
            self.assertTrue(style["storyline_first"])
            self.assertTrue(style["cast_notes_enabled"])
            self.assertTrue(style["meme_callback_section_enabled"])
            self.assertTrue(style["relationship_section_enabled"])
            self.assertTrue(style["image_first_delivery"])
            self.assertFalse(style["pdf_default_delivery"])
            self.assertTrue(style["roast_review_boundary"])
            self.assertTrue(style["private_draft_only"])
            self.assertFalse(style["public_surface_contains_private_draft"])
            self.assertIn("## 今日剧中人", result["daily_report_markdown"])
            self.assertIn("## 梗和回调候选", result["daily_report_markdown"])
            summary = daily_case_report._summary_result_json(result)
            self.assertIn("character_universe_summary", summary)
            self.assertEqual(
                summary["character_universe_summary"]["people_count"],
                len(result["character_universe"]["people"]),
            )
            self.assertNotIn("daily_report_markdown", summary)
            self.assertNotIn("operator_review_message", summary)
            self.assertNotIn("character_universe", summary)
            self.assertNotIn("quote_map", summary)
            self.assertNotIn("wiki_bundle", summary)
            self.assertNotIn("draft_bundle", summary)
            self.assertNotIn("run_manifest", summary)
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
            self.assertEqual(artifact["template_version"], "xiaoman-daily-case-report-v3")
            self.assertEqual(artifact["render"]["image_format"], "jpeg")
            self.assertEqual(artifact["render"]["width"], 750)
            self.assertEqual(artifact["render"]["jpeg_quality"], 92)
            self.assertEqual(artifact["content_metrics"]["character_count"], 0)
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
            self.assertTrue(Path(result["daily_report_markdown_path"]).is_file())
            self.assertTrue(Path(result["character_universe_path"]).is_file())
            self.assertTrue(Path(result["quote_map_path"]).is_file())
            self.assertTrue(Path(result["wiki_bundle_path"]).is_file())
            self.assertTrue(Path(result["draft_bundle_path"]).is_file())
            self.assertTrue(Path(result["run_manifest_path"]).is_file())
            self.assertTrue(Path(result["review_report_path"]).is_file())
            self.assertTrue(Path(result["creative_profile_review_payload_path"]).is_file())
            self.assertIn("character_universe_summary", result)
            self.assertEqual(
                result["character_universe_summary"]["people_count"],
                len(result["character_universe"]["people"]),
            )
            self.assertFalse(result["character_universe"]["raw_messages_included"])
            self.assertFalse(result["quote_map"]["public_surface_allowed"])
            self.assertFalse(result["wiki_bundle"]["public_surface_allowed"])
            self.assertFalse(result["run_manifest"]["privacy"]["profile_fact_text_included"])
            self.assertFalse(result["run_manifest"]["inputs"]["long_term_member_facts_used"])
            self.assertTrue(result["private_review_bundle"]["review_required"])
            self.assertEqual(
                result["private_review_bundle"]["creative_profile_review_payload"][
                    "approved_candidate_count"
                ],
                0,
            )
            self.assertFalse(result["requires_human_confirmation"])
            self.assertFalse(result["auto_publish_ready"])
            self.assertEqual(result["artifact_candidate"]["workflow_type"], "daily_case_report")
            self.assertEqual(result["artifact_candidate"]["render"]["width"], 750)
            self.assertEqual(
                result["artifact_candidate"]["report_window"]["start"],
                "2026-08-08T00:00:00+08:00",
            )
            self.assertNotIn("回复「发」", result["operator_review_message"])

    def test_main_summary_json_omits_private_rendered_bodies_for_auto_publish(self) -> None:
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
                json_summary_only=True,
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
            self.assertTrue(result["success"])
            self.assertEqual(result["image_mime_type"], "image/jpeg")
            self.assertIn("character_universe_summary", result)
            self.assertGreater(result["character_universe_summary"]["people_count"], 0)
            self.assertFalse(result["character_universe_summary"]["raw_messages_included"])
            self.assertFalse(result["private_review_bundle"]["raw_message_payload_read"])
            self.assertNotIn("daily_report_markdown", result)
            self.assertNotIn("operator_review_message", result)
            self.assertNotIn("character_universe", result)
            self.assertNotIn("quote_map", result)
            self.assertNotIn("wiki_bundle", result)
            self.assertNotIn("draft_bundle", result)
            self.assertNotIn("run_manifest", result)


if __name__ == "__main__":
    unittest.main()
