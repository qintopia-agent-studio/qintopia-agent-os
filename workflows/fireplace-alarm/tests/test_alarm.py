import json
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

WORKFLOW = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(WORKFLOW))
import dispatch
import listen
import state


START = 1_795_775_921_000
SMOKE, CO = state.SIGNALS
TEXT = json.loads(dispatch.TEMPLATES.read_text())


def dp(code, value, at):
    return {"code": code, "value": value, "time": at}


def report(points, device="test-fireplace"):
    return {"bizCode": "devicePropertyMessage", "bizData": {"devId": device, "properties": points}}


class FireplaceAlarm(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.path = Path(temp.name) / "state.db"
        self.db = state.connect(self.path, create=True)
        self.addCleanup(self.db.close)
        state.initialize(self.db, START)

    def push(self, points, at=START, device="test-fireplace"):
        consumer = Mock()
        event = report(points, device)
        listen.subscription.process(consumer, event, None, "test-fireplace",
                                    lambda rows: state.apply(self.db, rows, at),
                                    decrypt=lambda payload, _: payload)
        consumer.acknowledge.assert_called_once_with(event)

    def claim(self, at, profile="erhua"):
        job = state.claim(self.db, profile, at)
        if job:
            state.finish(self.db, job, "accepted", "simulated")
        return job

    def test_button_self_test_and_link_alarm_are_silent(self):
        # Property values and millisecond ordering from the three real button-test reports.
        self.push([dp("battery_state", "high", START + 141),
                   dp("checking_result", "check_success", START + 141)])
        self.push([dp("fault", 0, START + 203), dp(SMOKE, "normal", START + 203)])
        self.push([dp(CO, "normal", START + 259)])
        self.push([dp("link_alarm", "alarm", START + 800), dp("battery_state", "low", START + 800)])
        self.assertEqual(self.db.execute("SELECT count(*) FROM incidents").fetchone()[0], 0)
        for profile in state.PROFILES:
            self.assertIsNone(self.claim(START + 61_000, profile))

    def test_three_reminders_and_recovery_for_each_group(self):
        self.push([dp("checking_result", "check_success", START), dp(CO, "alarm", START)])
        for seconds, ordinal in [(0, 1), (29, None), (30, 2), (59, None), (60, 3), (90, None)]:
            for profile in state.PROFILES:
                job = self.claim(START + seconds * 1000, profile)
                if ordinal is None:
                    self.assertIsNone(job)
                    continue
                text = dispatch.render(job, TEXT)
                self.assertIn("立即撤到室外空气新鲜处", text)
                self.assertIn("120", text)
                self.assertIn(f"第 {ordinal}/3 次", text)
                self.assertIn("2026-11-27 18:38:41", text)
                self.assertEqual("当班同事" in text, profile == "silaoshi")
        now = START + 100_000
        self.push([dp(CO, "normal", now)], now)
        for profile in state.PROFILES:
            text = dispatch.render(self.claim(now, profile), TEXT)
            self.assertIn("一氧化碳状态恢复正常", text)
            self.assertIn("等现场救援人员确认安全后再返回", text)
            self.assertIsNone(self.claim(now + 61_000, profile))

    def test_escalation_uses_next_slot_and_partial_recovery_keeps_active_hazard(self):
        self.push([dp(SMOKE, "alarm", START)])
        self.assertEqual(self.claim(START)["hazards"], 1)
        self.push([dp(SMOKE, "alarm", START)], START + 1000)
        self.assertIsNone(self.claim(START + 1000))
        self.push([dp(CO, "alarm", START + 2000)], START + 2000)
        combined = self.claim(START + 2000)
        self.assertEqual((combined["hazards"], combined["ordinal"]), (3, 2))
        self.assertIn("同时报警", dispatch.render(combined, TEXT))
        self.assertEqual(self.claim(START + 2000, "silaoshi")["ordinal"], 1)
        self.push([dp(SMOKE, "normal", START + 4000)], START + 4000)
        self.assertIsNone(self.claim(START + 30_000))
        last = self.claim(START + 60_000)
        self.assertEqual((last["hazards"], last["ordinal"]), (2, 3))
        self.push([dp(SMOKE, "alarm", START + 65_000)], START + 65_000)
        self.assertIsNone(self.claim(START + 65_000))

    def test_full_recovery_cancels_repeats_and_only_updates_notified_routes(self):
        self.push([dp(SMOKE, "alarm", START), dp(CO, "alarm", START)])
        self.assertEqual(self.claim(START)["hazards"], 3)
        self.push([dp(CO, "normal", START + 500), dp(SMOKE, "normal", START + 750)], START + 750)
        for _ in range(2):
            self.push([dp(SMOKE, "normal", START + 750)], START + 751)
        job = self.claim(START + 751)
        self.assertEqual((job["kind"], job["hazards"]), ("recovery", 3))
        self.assertIsNone(self.claim(START + 751, "silaoshi"))
        self.assertIsNone(self.claim(START + 60_000))
        self.push([dp(SMOKE, "alarm", START + 80_000)], START + 80_000)
        self.assertEqual(self.claim(START + 80_000)["ordinal"], 1)

    def test_preactivation_other_device_and_out_of_order(self):
        self.push([dp(SMOKE, "alarm", START - 1)])
        self.push([dp(SMOKE, "alarm", START)], device="different-device")
        self.push([dp(CO, "normal", START + 5)])
        self.push([dp(CO, "alarm", START + 4)])
        self.assertIsNone(self.claim(START + 10))
        self.push([dp(SMOKE, "normal", START + 10), dp(SMOKE, "alarm", START + 10)])
        self.push([dp(SMOKE, "normal", START + 10)])
        self.assertEqual(self.claim(START + 10)["hazards"], 1)

    def test_concurrent_claim_and_restart_do_not_replay_a_reserved_notification(self):
        self.push([dp(SMOKE, "alarm", START)])

        def reserve(_):
            with closing(state.connect(self.path)) as connection:
                return state.claim(connection, "erhua", START)

        with ThreadPoolExecutor(max_workers=2) as pool:
            jobs = [j for j in pool.map(reserve, range(2)) if j]
        self.assertEqual(len(jobs), 1)
        state.initialize(self.db, START + 1000)
        self.assertIsNone(self.claim(START + 1000))
        self.assertEqual(self.claim(START + 30_000)["ordinal"], 2)
        self.assertEqual(self.claim(START + 30_000, "silaoshi")["ordinal"], 1)

    def test_old_pending_recovery_is_superseded_by_new_incident(self):
        self.push([dp(SMOKE, "alarm", START)])
        self.claim(START)
        self.push([dp(SMOKE, "normal", START + 40)], START + 40)
        self.push([dp(CO, "alarm", START + 60)], START + 60)
        self.assertEqual(self.claim(START + 60)["kind"], "alarm")
        self.assertIsNone(self.claim(START + 100))

    def test_delayed_event_suppressed_only_on_newer_normal(self):
        self.push([dp(SMOKE, "alarm", START)], START + 90_000)
        verify = Mock(return_value=[dp(SMOKE, "normal", START - 1000)])
        first = state.claim(self.db, "erhua", START + 90_000, verify_late=verify)
        self.assertEqual(first["kind"], "alarm")
        verify.assert_called_once_with(1)
        verify.return_value = [dp(SMOKE, "normal", START + 70_000)]
        self.assertIsNone(state.claim(self.db, "silaoshi", START + 91_000, verify_late=verify))

    def test_only_reported_normal_for_correct_sensor_can_reconcile_delayed_job(self):
        rows = [{"code": SMOKE, "status": "ok", "value": "normal", "reported_at": "2026-11-27T18:38:42+08:00"},
                {"code": CO, "status": "ok", "value": "alarm", "reported_at": "2026-11-27T18:38:43+08:00"}]
        device = {"name": "烟雾报警器", "area": "一楼壁炉区", "readings": rows}
        run = Mock(return_value=SimpleNamespace(stdout=json.dumps({"devices": [device]})))
        self.assertEqual(dispatch.newer_normals(3, run), [dp(SMOKE, "normal", START + 1000)])

    def test_platform_receipts_and_uncertain_delivery_are_independent(self):
        self.push([dp(SMOKE, "alarm", START)])
        role, group = "silaoshi", "work-example"
        job = state.claim(self.db, role, START)
        run = Mock(return_value=SimpleNamespace(returncode=0, stdout=json.dumps({
            "success": True, "message_id": "accepted-1", "platform": "wecom", "chat_id": group})))
        self.assertEqual(dispatch.send(job, "wecom:" + group, TEXT, run), ("accepted", "accepted-1"))
        argv = run.call_args.args[0]
        self.assertEqual(argv[1:-1], ["-p", role, "send", "--json", "--to", "wecom:" + group])
        self.assertIn("请当班同事", argv[-1])
        self.assertEqual(dispatch.send(job, "wecom:different", TEXT, run)[0], "unknown")
        state.finish(self.db, job, "accepted", "accepted-1")
        other = state.claim(self.db, "erhua", START)
        run.return_value.stdout = json.dumps({"success": True, "raw_response": {"code": 0,
            "data": {"isSendSuccess": 1, "msgServerId": 42}}})
        self.assertEqual(dispatch.send(other, "qiwe:community-example", TEXT, run), ("accepted", "42"))
        run.side_effect = subprocess.TimeoutExpired("hermes", 24)
        status, evidence = dispatch.send(other, "qiwe:community-example", TEXT, run)
        self.assertEqual(status, "unknown")
        state.finish(self.db, other, status, evidence)
        self.push([dp(SMOKE, "normal", START + 1000)], START + 1000)
        self.assertIsNone(self.claim(START + 1000))
        self.assertEqual(self.claim(START + 1000, "silaoshi")["kind"], "recovery")

    def test_only_acknowledge_after_commit(self):
        consumer = Mock()
        with self.assertRaises(sqlite3.OperationalError):
            listen.subscription.process(consumer, object(), None, "test-fireplace",
                Mock(side_effect=sqlite3.OperationalError("test")), decrypt=lambda *_: report([]))
        consumer.acknowledge.assert_not_called()
        listen.subscription.process(consumer, object(), None, "test-fireplace", Mock(),
                                    decrypt=Mock(side_effect=ValueError("invalid ciphertext")))
        consumer.negative_acknowledge.assert_called_once()
        consumer.acknowledge.assert_not_called()
