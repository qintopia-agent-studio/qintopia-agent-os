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
        for text, reason in [("七点", "missing_date"), ("9点", "missing_date"),
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
