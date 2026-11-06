import asyncio
import copy
import json
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from adapter import QiWeAdapter
from solitaire.activity_service import ActivityService
from solitaire.feishu_writer import FeishuActivityMapping, FeishuActivityWriter
from solitaire.llm_parser import HermesSolitaireContentParser
from solitaire.parser import build_activity_record_from_fields
from solitaire.reminder import ReminderWorker, ReminderWorkerConfig
from solitaire.repository import ActivityRepository
from solitaire.time_resolution import resolve_time

ANCHOR = datetime(2026, 9, 17, 0, 0, tzinfo=timezone.utc)
DUE = datetime(2026, 9, 17, 10, 30, tzinfo=timezone.utc)


def event(text="今天晚上七点", timestamp=ANCHOR):
    return SimpleNamespace(message_kind="solitaire", text=text, timestamp=timestamp,
                           group_id="synthetic-room", event_id="synthetic-event", sender_id="synthetic-author",
                           raw_event_ref={"msgData": {"solitaireInfo": {
                               "authorId": "synthetic-author", "timestamp": ANCHOR.timestamp()}}})


def activity(text="今天晚上七点", timestamp=ANCHOR):
    return build_activity_record_from_fields(event(text, timestamp), "测试活动\n" + text,
                                            activity_subject="测试活动", activity_identity="测试活动",
                                            start_time=text, participant_names=["测试参与人"])


class TimeResolutionTests(unittest.TestCase):
    def resolve(self, text, **kwargs):
        return resolve_time(text, anchor=ANCHOR, timezone="Asia/Shanghai", **kwargs)

    def test_supported_dates_and_single_ranges(self):
        for text, expected in [
            ("今天晚上七点", "2026-09-17 19:00"),
            ("明天下午三点半", "2026-09-18 15:30"),
            ("2026.9.18 19:30", "2026-09-18 19:30"),
            ("9月18号（周五）19:30", "2026-09-18 19:30"),
            ("9月18号14:30‑16:30", "2026-09-18 14:30"),
            ("9月18号14:30-16:30", "2026-09-18 14:30"),
            ("2026/9/18 上午9点", "2026-09-18 09:00"),
            ("下周三 18:00", "2026-09-23 18:00"),
            ("今晚8点", "2026-09-17 20:00"),
        ]:
            with self.subTest(text=text):
                result = self.resolve(text)
                self.assertEqual(result.state, "resolved")
                self.assertEqual(result.start_time, expected)

    def test_ambiguous_or_invalid_never_guesses(self):
        for text, reason in [("七点", "ambiguous_daypart"), ("9点", "ambiguous_daypart"),
                             ("今天七点", "ambiguous_daypart"), ("2026-09-18", "missing_clock"),
                             ("9月15到17号", "multiple_occurrences"),
                             ("2026-02-30 19:00", "invalid_date"),
                             ("9月18号（周六）19:30", "conflicting_dates"),
                             ("明天19:00 后天20:00", "multiple_dates")]:
            with self.subTest(text=text):
                result = self.resolve(text)
                self.assertEqual(result.state, "needs_confirmation")
                self.assertEqual(result.reason, reason)
                self.assertFalse(result.start_time)

    def test_clock_without_date_uses_creation_day_and_preserves_source_facts(self):
        for text, expected in [
            ("下午7.30", "2026-09-17 19:30"),
            ("下午七点半", "2026-09-17 19:30"),
            ("晚上7.30", "2026-09-17 19:30"),
            ("19:30", "2026-09-17 19:30"),
            ("15点", "2026-09-17 15:00"),
            ("9:30-11:30", "2026-09-17 09:30"),
            ("19:30-20:30", "2026-09-17 19:30"),
        ]:
            with self.subTest(text=text):
                result = self.resolve(text, source=text, facts={"date_text": "", "time_text": text})
                self.assertEqual(result.state, "resolved")
                self.assertEqual(result.reason, "resolved_from_creation_day")
                self.assertEqual(result.date_text, "")
                self.assertEqual(result.start_time, expected)

    def test_clock_creation_day_uses_configured_timezone(self):
        result = resolve_time("19:30", anchor=datetime(2026, 9, 16, 17, tzinfo=timezone.utc),
                              timezone="Asia/Shanghai")
        self.assertEqual(result.start_time, "2026-09-17 19:30")
        self.assertEqual(result.reason, "resolved_from_creation_day")

    def test_clock_without_date_requires_creation_anchor(self):
        result = resolve_time("下午7.30", anchor=None, timezone="Asia/Shanghai",
                              source="下午7.30模拟练习", facts={"date_text": "", "time_text": "下午7.30"})
        self.assertEqual(result.state, "needs_confirmation")
        self.assertEqual(result.reason, "missing_anchor")
        self.assertFalse(result.start_time)

    def test_source_date_omitted_by_model_precedes_creation_day_default(self):
        for source, expected in [
            ("9月18日，下午7.30模拟练习", "2026-09-18 19:30"),
            ("日期：2026-09-18\n下午7.30模拟练习", "2026-09-18 19:30"),
            ("日期：2027-09-18\n下午7.30模拟练习", "2027-09-18 19:30"),
            ("日期:2027-09-18\n下午7.30模拟练习", "2027-09-18 19:30"),
            ("明天下午7.30模拟练习", "2026-09-18 19:30"),
        ]:
            with self.subTest(source=source):
                result = self.resolve("下午7.30", source=source,
                                      facts={"date_text": "", "time_text": "下午7.30"})
                self.assertEqual(result.state, "resolved")
                self.assertEqual(result.reason, "resolved")
                self.assertTrue(result.date_text)
                self.assertEqual(result.start_time, expected)

    def test_source_date_conflicts_cannot_be_omitted_for_clock_default(self):
        for source in ("2026-09-24（周四）今天下午7.30", "9月18日（周六）下午7.30",
                       "9月18日和9月19日下午7.30", "2026-02-30下午7.30"):
            with self.subTest(source=source):
                result = self.resolve("下午7.30", source=source, facts={})
                self.assertEqual(result.state, "needs_confirmation")
                self.assertFalse(result.start_time)

    def test_unsupported_source_date_prevents_creation_day_default(self):
        for date_text in ("周末", "下个月", "昨天", "月底", "下周", "这周", "明年9月18日", "昨天（周三）"):
            for expression in ("下午7.30", date_text + "下午7.30"):
                with self.subTest(date_text=date_text, expression=expression):
                    result = self.resolve(expression, source=date_text + "下午7.30模拟练习", facts={})
                    self.assertEqual(result.state, "needs_confirmation")
                    self.assertFalse(result.start_time)

    def test_clock_fragment_keeps_source_daypart_before_defaulting_date(self):
        result = self.resolve("7.30", source="下午7.30进行模拟练习",
                              facts={"date_text": "", "time_text": "7.30"})
        self.assertEqual(result.state, "resolved")
        self.assertEqual(result.reason, "resolved_from_creation_day")
        self.assertEqual(result.date_text, "")
        self.assertEqual(result.time_text, "下午7.30")
        self.assertEqual(result.start_time, "2026-09-17 19:30")

    def test_invalid_or_multiple_clocks_do_not_gain_a_creation_day(self):
        for text in ("下午7.90", "25:00", "下午7.30或晚上8.30", "19:30/20:30"):
            with self.subTest(text=text):
                result = self.resolve(text, source=text, facts={})
                self.assertEqual(result.state, "needs_confirmation")
                self.assertFalse(result.start_time)

    def test_clock_default_cannot_omit_an_alternative_source_clock(self):
        source = "下午7.30或晚上8.30进行模拟练习"
        result = self.resolve("下午7.30", source=source,
                              facts={"date_text": "", "time_text": "下午7.30"})
        self.assertEqual(result.state, "needs_confirmation")
        self.assertFalse(result.start_time)

    def test_clock_default_recovers_range_start_omitted_by_model(self):
        result = self.resolve("11:30", source="9:30-11:30进行模拟练习",
                              facts={"date_text": "", "time_text": "11:30"})
        self.assertEqual(result.state, "resolved")
        self.assertEqual(result.reason, "resolved_from_creation_day")
        self.assertEqual(result.date_text, "")
        self.assertEqual(result.time_text, "9:30")
        self.assertEqual(result.start_time, "2026-09-17 09:30")

    def test_period_clock_with_explicit_daypart(self):
        for text, expected in [
            ("今晚7.30", "2026-09-17 19:30"),
            ("今晚 7.30", "2026-09-17 19:30"),
            ("明晚7.30", "2026-09-18 19:30"),
            ("明晚 7.30", "2026-09-18 19:30"),
            ("今天晚上7.30", "2026-09-17 19:30"),
            ("今晚7．30", "2026-09-17 19:30"),
        ]:
            with self.subTest(text=text):
                result = self.resolve(text)
                self.assertEqual(result.state, "resolved")
                self.assertEqual(result.start_time, expected)

    def test_same_day_words_in_source_do_not_create_multiple_dates(self):
        source = "今晚7:30，今天一起学习，今日欢迎参与，今晚见。"
        result = self.resolve("今晚7:30", source=source,
                              facts={"date_text": "今晚", "time_text": "7:30"})
        self.assertEqual(result.state, "resolved")
        self.assertEqual(result.start_time, "2026-09-17 19:30")

    def test_period_clock_combines_verbatim_date_and_clock_facts(self):
        result = self.resolve("7.30", source="今晚7.30，今天一起学习。",
                              facts={"date_text": "今晚", "time_text": "7.30"})
        self.assertEqual(result.state, "resolved")
        self.assertEqual(result.start_time, "2026-09-17 19:30")

    def test_period_dates_without_daypart_remain_dates(self):
        for text in ("7.30", "2026.7.30"):
            for kwargs in ({}, {"source": text, "facts": {}}):
                with self.subTest(text=text, source_facts=bool(kwargs)):
                    result = self.resolve(text, **kwargs)
                    self.assertEqual(result.state, "needs_confirmation")
                    self.assertEqual(result.reason, "missing_clock")
                    self.assertEqual(result.date_text, text)
                    self.assertFalse(result.start_time)
        for text in ("7.30 19:30", "2026.7.30 19:30"):
            with self.subTest(text=text):
                self.assertEqual(self.resolve(text).start_time, "2026-07-30 19:30")

    def test_model_cannot_omit_conflicting_source_date_after_same_day_grouping(self):
        source = "2026-09-24（周四）今天19:30"
        for expression, facts in [
            ("2026-09-24（周四）", {"date_text": "2026-09-24", "time_text": "19:30"}),
            ("今天19:30", {}),
        ]:
            with self.subTest(expression=expression):
                result = self.resolve(expression, source=source, facts=facts)
                self.assertEqual(result.state, "needs_confirmation")
                self.assertEqual(result.reason, "conflicting_dates")
                self.assertFalse(result.start_time)

    def test_explicit_date_must_agree_with_every_relative_word(self):
        conflicting = "2026-09-24（周四）今天19:30"
        result = self.resolve(conflicting, source=conflicting, facts={})
        self.assertEqual(result.state, "needs_confirmation")
        self.assertEqual(result.reason, "conflicting_dates")
        self.assertFalse(result.start_time)

        agreeing = "2026-09-17（周四）今天19:30"
        result = self.resolve(agreeing, source=agreeing, facts={})
        self.assertEqual(result.state, "resolved")
        self.assertEqual(result.start_time, "2026-09-17 19:30")

    def test_invalid_period_clock_or_date_suffix_never_schedules(self):
        for text in ("今晚7.90", "今晚7.30日"):
            with self.subTest(text=text):
                result = self.resolve(text)
                self.assertEqual(result.state, "needs_confirmation")
                self.assertFalse(result.start_time)

    def test_facts_combine_source_lines_without_inventing(self):
        facts = {"date_text": "9月18号", "time_text": "晚上七点"}
        result = self.resolve("晚上七点", source="日期：9月18号\n开场：晚上七点", facts=facts)
        self.assertEqual(result.start_time, "2026-09-18 19:00")
        self.assertEqual(self.resolve("七点", source="七点", facts=facts).state, "needs_confirmation")

    def test_model_cannot_select_one_date_from_multiple_occurrences(self):
        result = self.resolve("9月18号 19:00", source="9月18号 19:00 和9月19号 20:00",
                              facts={"date_text": "9月18号", "time_text": "19:00"})
        self.assertEqual(result.reason, "multiple_dates")

    def test_past_dates_and_year_boundary(self):
        self.assertEqual(self.resolve("2024-04-15 14:30").start_time, "2024-04-15 14:30")
        result = resolve_time("明天 01:00", anchor=datetime(2026, 12, 31, 15, tzinfo=timezone.utc), timezone="Asia/Shanghai")
        self.assertEqual(result.start_time, "2027-01-01 01:00")


class ReminderLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = ActivityRepository(self.temp.name)
        self.service = ActivityService(self.repo, FeishuActivityWriter(FeishuActivityMapping()))

    def ledger(self):
        return json.loads((Path(self.temp.name) / "solitaire/reminders.json").read_text())

    def worker(self, send, dry_run=False, repository=None):
        service = self.service if repository is None else ActivityService(repository, self.service.writer)
        return ReminderWorker(ReminderWorkerConfig(enabled=True, dry_run=dry_run, allowed_groups=["synthetic-room"]), service, send)

    def parse_scripted_activity(self, text, start_time, time_facts, source_event=None):
        fields = {"is_activity": True, "activity_subject": "模拟学习活动", "activity_type": "社区活动",
                  "activity_detail": "活动室", "start_time": start_time,
                  "time_facts": time_facts, "participant_names": ["模拟参与人"], "promo_text": "欢迎参与"}
        calls = []

        async def complete(**kwargs):
            self.assertEqual(kwargs["purpose"], "qiwe_solitaire_activity_parse")
            self.assertIn(text, kwargs["messages"][-1]["content"])
            calls.append(kwargs)
            return SimpleNamespace(text=json.dumps(fields))

        parser = HermesSolitaireContentParser(SimpleNamespace(acomplete=complete))
        service = ActivityService(self.repo, self.service.writer, parser)
        with patch.object(service, "schedule_feishu_sync"):
            result = asyncio.run(service.upsert_from_solitaire(source_event or event(text)))
        self.assertEqual(len(calls), 1)
        return result

    def live_ack(self, result):
        holder = SimpleNamespace(qiwe=SimpleNamespace(activity_reminder_enabled=True,
                                 activity_reminder_dry_run=False, activity_reminder_allowed_groups=["synthetic-room"]))
        return QiWeAdapter._passive_ack_text(holder, result, group_id="synthetic-room")

    def test_period_clock_and_same_day_words_schedule_one_correct_reminder(self):
        text = "今晚7.30，活动室见。今天一起学习，今日欢迎参与，今晚见。\n\n1. 模拟参与人"
        result = self.parse_scripted_activity(text, "今晚7.30", {"date_text": "今晚", "time_text": "7.30"})
        self.assertEqual(result.start_time, "2026-09-17 19:30")
        self.assertEqual(result.reminder_plan["state"], "scheduled")
        self.assertEqual(result.reminder_plan["due_at"], ["2026-09-17T11:00:00+00:00"])
        self.assertEqual(len(self.ledger()), 1)
        stored = self.repo.get_activity(result.activity_id)
        self.assertEqual(stored["time_resolution"]["state"], "resolved")
        self.assertEqual(stored["start_time"], "2026-09-17 19:30")
        ack = self.live_ack(result)
        self.assertIn("时间二花也记下了：2026-09-17 19:30。", ack)
        self.assertIn("已安排活动开始前提醒", ack)
        self.assertNotIn("待确认", ack)
        self.assertNotIn("不明确", ack)

    def test_clock_only_activity_schedules_and_explains_creation_day(self):
        text = "下午7.30活动室模拟练习。\n\n1. 模拟参与人"
        result = self.parse_scripted_activity(text, "下午7.30", {"date_text": "", "time_text": "下午7.30"})
        self.assertEqual(result.start_time, "2026-09-17 19:30")
        self.assertEqual(result.reminder_plan["state"], "scheduled")
        self.assertEqual(result.reminder_plan["due_at"], ["2026-09-17T11:00:00+00:00"])
        self.assertEqual(len(self.ledger()), 1)
        stored = self.repo.get_activity(result.activity_id)
        self.assertEqual(stored["time_resolution"]["reason"], "resolved_from_creation_day")
        self.assertEqual(stored["time_resolution"]["date_text"], "")
        ack = self.live_ack(result)
        self.assertIn("时间二花也记下了：2026-09-17 19:30。", ack)
        self.assertIn("首次发起当天", ack)
        self.assertIn("已安排活动开始前提醒", ack)
        self.assertNotIn("待确认", ack)

    def test_clock_only_forward_keeps_original_day_without_new_reminder(self):
        text = "下午7.30活动室模拟练习。\n\n1. 模拟参与人"
        facts = {"date_text": "", "time_text": "下午7.30"}
        first = self.parse_scripted_activity(text, "下午7.30", facts)
        self.assertEqual(first.reminder_plan["state"], "scheduled")
        original_jobs = set(self.ledger())
        later = self.parse_scripted_activity(text, "下午7.30", facts,
                                            source_event=event(text, ANCHOR + timedelta(days=1)))
        self.assertEqual(later.activity_id, first.activity_id)
        self.assertEqual(later.start_time, "2026-09-17 19:30")
        self.assertEqual(set(self.ledger()), original_jobs)
        self.assertNotEqual(later.reminder_plan["state"], "scheduled")
        self.assertFalse(self.repo.due_reminders(ANCHOR + timedelta(days=1)))

    def test_clock_only_past_start_does_not_roll_to_tomorrow_or_backfill(self):
        text = "下午7.30活动室模拟练习。\n\n1. 模拟参与人"
        result = self.parse_scripted_activity(text, "下午7.30", {"date_text": "", "time_text": "下午7.30"},
                                             source_event=event(text, ANCHOR + timedelta(hours=12)))
        self.assertEqual(result.start_time, "2026-09-17 19:30")
        self.assertEqual(result.reminder_plan["state"], "not_scheduled")
        self.assertEqual(result.reminder_plan["reason"], "activity_started")
        self.assertFalse(self.ledger())
        self.assertFalse(result.immediate_reminder)
        self.assertNotIn("已安排", self.live_ack(result))

    def test_clock_only_event_without_any_timestamp_never_uses_machine_date(self):
        text = "下午7.30活动室模拟练习。\n\n1. 模拟参与人"
        source_event = event(text, timestamp=None)
        source_event.raw_event_ref = {}
        result = self.parse_scripted_activity(text, "下午7.30", {"date_text": "", "time_text": "下午7.30"},
                                             source_event=source_event)
        stored = self.repo.get_activity(result.activity_id)
        self.assertEqual(stored["time_resolution"]["state"], "needs_confirmation")
        self.assertEqual(stored["time_resolution"]["reason"], "missing_anchor")
        self.assertEqual(stored["time_resolution"]["anchor"], "")
        self.assertEqual(stored["solitaire_created_at"], "")
        self.assertEqual(result.reminder_plan["state"], "needs_confirmation")
        self.assertFalse(self.ledger())

    def test_different_days_or_ungrounded_facts_never_schedule(self):
        for text, facts, reason in [
            ("今晚7:30一起学习，明天还有一场。\n\n1. 模拟参与人",
             {"date_text": "今晚", "time_text": "7:30"}, "multiple_dates"),
            ("今晚7:30一起学习。\n\n1. 模拟参与人",
             {"date_text": "明天", "time_text": "7:30"}, "ungrounded_time_facts"),
        ]:
            with self.subTest(reason=reason):
                result = self.parse_scripted_activity(text, "今晚7:30", facts)
                self.assertEqual(result.reminder_plan["state"], "needs_confirmation")
                stored = self.repo.get_activity(result.activity_id)
                self.assertEqual(stored["time_resolution"]["reason"], reason)
                self.assertFalse(self.ledger())
                ack = self.live_ack(result)
                self.assertIn("今晚7:30", ack)
                self.assertIn("待确认", ack)
                self.assertNotIn("时间二花也记下了", ack)
                self.assertNotIn("已安排", ack)

    def test_parse_to_plan_to_single_fake_delivery(self):
        fields = {"is_activity": True, "activity_subject": "测试活动", "activity_type": "社区活动",
                  "activity_detail": "测试", "start_time": "今天晚上七点",
                  "time_facts": {"date_text": "今天", "time_text": "晚上七点"},
                  "participant_names": ["测试参与人"], "promo_text": "测试"}
        async def complete(**kwargs):
            return SimpleNamespace(text=json.dumps(fields))
        parser = HermesSolitaireContentParser(SimpleNamespace(acomplete=complete))
        service = ActivityService(self.repo, self.service.writer, parser)
        with patch.object(service, "schedule_feishu_sync"):
            result = asyncio.run(service.upsert_from_solitaire(event()))
        self.assertEqual(result.reminder_plan["state"], "scheduled")
        self.assertEqual(result.reminder_plan["due_at"], [DUE.isoformat()])
        calls = []
        async def send(*args, **kwargs):
            calls.append(args)
            return SimpleNamespace(success=True, message_id="synthetic")
        worker = self.worker(send)
        self.assertEqual(asyncio.run(worker.run_once(DUE - timedelta(seconds=1))).scanned, 0)
        self.assertEqual(asyncio.run(worker.run_once(DUE)).sent, 1)
        restarted = self.worker(send, repository=ActivityRepository(self.temp.name))
        self.assertEqual(asyncio.run(restarted.run_once(DUE)).scanned, 0)
        self.assertEqual(len(calls), 1)
        self.assertEqual(self.repo.get_activity(result.activity_id)["reminder_plan"]["state"], "sent")

    def test_unresolved_then_clarified(self):
        first = activity("七点")
        self.service.upsert_activity(first)
        self.assertEqual(self.repo.get_activity(first.activity_id)["reminder_plan"]["state"], "needs_confirmation")
        self.assertFalse(self.ledger())
        second = activity()
        self.service.upsert_activity(second)
        self.assertEqual(first.activity_id, second.activity_id)
        self.assertEqual(second.reminder_plan["state"], "scheduled")

    def test_reschedule_and_duplicate_signup(self):
        first = activity()
        self.service.upsert_activity(first)
        old_id = next(iter(self.ledger()))
        edited = activity("今天晚上八点")
        self.service.upsert_activity(edited)
        self.service.upsert_activity(copy.deepcopy(edited))
        jobs = self.ledger()
        self.assertEqual(first.activity_id, edited.activity_id)
        self.assertEqual(jobs[old_id]["delivery_state"], "cancelled")
        self.assertEqual(sum(j["status"] == "pending" for j in jobs.values()), 1)
        self.assertFalse(self.repo.mark_reminder_sending(old_id, now=DUE))

    def test_late_reschedule_does_not_backfill_elapsed_reminder(self):
        self.service.upsert_activity(activity("今天晚上八点"))
        updated = activity("今天晚上七点", DUE + timedelta(minutes=10))
        self.service.upsert_activity(updated)
        self.assertEqual(updated.reminder_plan["reason"], "reminder_window_elapsed")
        self.assertFalse(self.repo.due_reminders(DUE + timedelta(minutes=11)))

    def test_ambiguous_edit_cancels_prior_plan_instead_of_reusing_old_time(self):
        item = activity()
        self.service.upsert_activity(item)
        edited = activity("七点")
        self.service.upsert_activity(edited)
        self.assertEqual(edited.reminder_plan["state"], "needs_confirmation")
        self.assertFalse(self.repo.due_reminders(DUE))

    def test_forward_retains_creation_date(self):
        first = activity("明天晚上七点")
        second = activity("明天晚上七点", ANCHOR + timedelta(days=1))
        self.service.upsert_activity(first)
        self.service.upsert_activity(second)
        self.assertEqual(second.start_time, "2026-09-18 19:00")
        self.assertEqual(len(self.ledger()), 1)

    def test_interrupted_commit_recovers_before_claim(self):
        original = self.repo._save_json
        def interrupted(name, value):
            if name == "reminders.json":
                raise OSError("synthetic disk interruption")
            original(name, value)
        item = activity()
        with patch.object(self.repo, "_save_json", side_effect=interrupted):
            with self.assertRaises(OSError):
                self.service.upsert_activity(item)
        restarted = ActivityRepository(self.temp.name)
        self.assertEqual(len(restarted.due_reminders(DUE)), 1)
        self.assertEqual(restarted.get_activity(item.activity_id)["reminder_plan"]["state"], "scheduled")
        self.assertFalse((Path(self.temp.name) / "solitaire/activity-transaction.json").exists())

    def test_concurrent_claim_once(self):
        self.service.upsert_activity(activity())
        job_id = next(iter(self.ledger()))
        with ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(lambda _: ActivityRepository(self.temp.name).mark_reminder_sending(job_id, now=DUE), range(2)))
        self.assertEqual(sorted(results), [False, True])

    def test_typeerror_does_not_invoke_send_twice(self):
        self.service.upsert_activity(activity())
        calls = []
        async def send(*args, **kwargs):
            calls.append(1)
            raise TypeError("synthetic exception after send")
        worker = self.worker(send)
        self.assertEqual(asyncio.run(worker.run_once(DUE)).failed, 1)
        self.assertEqual(next(iter(self.ledger().values()))["delivery_state"], "ambiguous")
        asyncio.run(self.worker(send, repository=ActivityRepository(self.temp.name)).run_once(DUE + timedelta(minutes=6)))
        self.assertEqual(calls, [1])

    def test_stale_sending_not_requeued(self):
        self.service.upsert_activity(activity())
        job_id = next(iter(self.ledger()))
        self.assertTrue(self.repo.mark_reminder_sending(job_id, now=DUE))
        self.assertFalse(self.repo.due_reminders(DUE + timedelta(minutes=6)))
        self.assertEqual(self.ledger()[job_id]["delivery_state"], "ambiguous")

    def test_dry_run_does_not_consume(self):
        self.service.upsert_activity(activity())
        async def send(*args, **kwargs):
            raise AssertionError("no external sends")
        result = asyncio.run(self.worker(send, dry_run=True).run_once(DUE))
        self.assertEqual(result.previewed, 1)
        self.assertEqual(result.sent, 0)
        self.assertEqual(next(iter(self.ledger().values()))["status"], "pending")

    def test_expired_never_backfills(self):
        self.service.upsert_activity(activity())
        self.assertFalse(self.repo.due_reminders(DUE + timedelta(hours=1)))
        self.assertEqual(next(iter(self.ledger().values()))["delivery_state"], "expired")

    def test_older_snapshot_cannot_revert_reschedule(self):
        old = activity()
        self.service.upsert_activity(old)
        edited = activity("今天晚上八点", ANCHOR + timedelta(minutes=5))
        self.service.upsert_activity(edited)
        self.service.upsert_activity(old)
        self.assertEqual(self.repo.get_activity(edited.activity_id)["start_time"], "2026-09-17 20:00")

    def test_unknown_outcome_survives_activity_updates(self):
        item = activity()
        self.service.upsert_activity(item)
        job_id = next(iter(self.ledger()))
        self.repo.mark_reminder_sending(job_id, now=DUE)
        self.repo.mark_reminder_failed(job_id, {"success": False, "retryable": True})
        self.service.upsert_activity(copy.deepcopy(item))
        self.assertEqual(self.ledger()[job_id]["delivery_state"], "ambiguous")
        self.assertEqual(self.repo.get_activity(item.activity_id)["reminder_plan"]["state"], "needs_reconciliation")

    def test_terminal_states_remain_non_sendable_to_old_workers(self):
        self.service.upsert_activity(activity())
        job_id = next(iter(self.ledger()))
        self.repo.mark_reminder_sending(job_id, now=DUE)
        self.repo.mark_reminder_failed(job_id, {"success": False})
        self.assertEqual(self.ledger()[job_id]["status"], "failed")
        self.assertFalse(self.ledger()[job_id]["sent"])

    def test_cancellation_prevents_claim(self):
        item = activity()
        self.service.upsert_activity(item)
        job_id = next(iter(self.ledger()))
        item.status = "cancelled"
        self.service.upsert_activity(item)
        self.assertFalse(self.repo.mark_reminder_sending(job_id, now=DUE))
        self.assertEqual(self.ledger()[job_id]["delivery_state"], "cancelled")

    def test_ack_only_promises_enabled_allowlisted_live_plan(self):
        holder = SimpleNamespace(qiwe=SimpleNamespace(activity_reminder_enabled=True,
                                 activity_reminder_dry_run=False, activity_reminder_allowed_groups=["synthetic-room"]))
        result = SimpleNamespace(activity_subject="测试", start_time="今天七点",
                                 reminder_plan={"state": "needs_confirmation"})
        text = QiWeAdapter._passive_ack_text(holder, result, group_id="synthetic-room")
        self.assertIn("暂未安排提醒", text)
        result.reminder_plan = {"state": "scheduled"}
        self.assertIn("已安排", QiWeAdapter._passive_ack_text(holder, result, group_id="synthetic-room"))
        self.assertNotIn("已安排", QiWeAdapter._passive_ack_text(holder, result, group_id="other-room"))
        holder.qiwe.activity_reminder_dry_run = True
        self.assertNotIn("已安排", QiWeAdapter._passive_ack_text(holder, result, group_id="synthetic-room"))

    def test_corrupt_state_fails_closed(self):
        self.service.upsert_activity(activity())
        path = Path(self.temp.name) / "solitaire/reminders.json"
        path.write_text("{")
        with self.assertRaisesRegex(RuntimeError, "activity_ledger_unreadable"):
            self.service.upsert_activity(activity())
        self.assertEqual(path.read_text(), "{")
