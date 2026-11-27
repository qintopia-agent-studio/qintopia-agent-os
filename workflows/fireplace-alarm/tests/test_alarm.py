import errno
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
from unittest.mock import MagicMock, Mock, patch

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

    def shadow(self, *points):
        properties = {point["code"]: point for point in points}
        readings = [listen.subscription.sensors.format_reading(code, {"label": code}, properties)
                    for code in state.SIGNALS]
        device = {"name": "烟雾报警器", "area": "一楼壁炉区", "readings": readings}
        rc = int(any(row["status"] != "ok" for row in readings))
        result = SimpleNamespace(returncode=rc, stdout=json.dumps({"devices": [device]}))
        return lambda: dispatch.read_shadow(run=Mock(return_value=result))

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
        verify.assert_called_once_with()
        verify.return_value = [dp(SMOKE, "normal", START + 70_000)]
        self.assertIsNone(state.claim(self.db, "silaoshi", START + 91_000, verify_late=verify))

    def test_shadow_returns_both_hazards_from_legacy_iso_timestamps(self):
        rows = [{"code": SMOKE, "status": "ok", "value": "normal", "reported_at": "2026-11-27T18:38:42+08:00"},
                {"code": CO, "status": "ok", "value": "alarm", "reported_at": "2026-11-27T18:38:43+08:00"}]
        device = {"name": "烟雾报警器", "area": "一楼壁炉区", "readings": rows}
        run = Mock(return_value=SimpleNamespace(returncode=0, stdout=json.dumps({"devices": [device]})))
        self.assertEqual(dispatch.read_shadow(run=run),
                         [dp(SMOKE, "normal", START + 1000), dp(CO, "alarm", START + 2000)])

    def test_alarming_shadow_blocks_both_recoveries_until_matching_push(self):
        self.push([dp(SMOKE, "alarm", START)])
        for role in state.PROFILES:
            self.claim(START, role)
        verify = self.shadow(dp(SMOKE, "normal", START + 100_000), dp(CO, "alarm", START + 105_000))
        self.assertIsNone(state.claim(self.db, "erhua", START + 125_000, verify_late=verify))
        # An older normal or a conflicting normal at the exact time cannot lift the hold.
        for offset in (104_000, 105_000):
            self.push([dp(CO, "normal", START + offset)], START + 126_000)
        with closing(state.connect(self.path)) as restarted:
            state.initialize(restarted, START + 126_000)
            for role in state.PROFILES:
                self.assertIsNone(state.claim(restarted, role, START + 126_000))
        self.push([dp(CO, "alarm", START + 105_000)], START + 127_000)
        for role in state.PROFILES:
            job = self.claim(START + 127_000, role)
            self.assertEqual((job["kind"], job["hazards"], job["ordinal"]), ("alarm", 2, 2))
        self.push([dp(CO, "normal", START + 130_000)], START + 130_000)
        for role in state.PROFILES:
            recovery = self.claim(START + 130_000, role)
            self.assertEqual((recovery["kind"], recovery["hazards"]), ("recovery", 3))
            self.assertIsNone(self.claim(START + 150_000, role))

    def test_newer_subscribed_normal_releases_shadow_hold_without_sending_shadow_alarm(self):
        self.push([dp(SMOKE, "alarm", START)])
        self.claim(START)
        verify = self.shadow(dp(SMOKE, "normal", START + 100_000), dp(CO, "alarm", START + 105_000))
        self.assertIsNone(state.claim(self.db, "erhua", START + 125_000, verify_late=verify))
        self.push([dp(CO, "normal", START + 126_000)], START + 127_000)
        job = self.claim(START + 127_000)
        self.assertEqual((job["kind"], job["reported_at"]), ("recovery", START + 126_000))
        self.assertIsNone(self.claim(START + 127_000, "silaoshi"))

    def test_stale_cloud_alarm_cannot_override_a_later_subscribed_normal(self):
        self.push([dp(SMOKE, "alarm", START)])
        self.claim(START)
        self.push([dp(CO, "normal", START + 80_000)], START + 80_000)
        verify = self.shadow(dp(SMOKE, "normal", START + 95_000), dp(CO, "alarm", START + 70_000))
        job = state.claim(self.db, "erhua", START + 125_000, verify_late=verify)
        self.assertEqual((job["kind"], job["reported_at"]), ("recovery", START + 95_000))

    def test_same_second_newer_normal_suppresses_old_evacuation(self):
        self.push([dp(SMOKE, "alarm", START + 100)], START + 95_000)
        verify = self.shadow(dp(SMOKE, "normal", START + 900))
        self.assertEqual(verify(), [dp(SMOKE, "normal", START + 900)])
        for role in state.PROFILES:
            self.assertIsNone(state.claim(self.db, role, START + 95_000, verify_late=verify))
        self.assertEqual(self.db.execute("SELECT count(*) FROM deliveries").fetchone()[0], 0)

    def test_late_recovery_checks_both_hazards_after_an_unannounced_incident(self):
        self.push([dp(SMOKE, "alarm", START)])
        self.claim(START)
        for code, value, offset in [(SMOKE, "normal", 1000), (CO, "alarm", 1200), (CO, "normal", 1400)]:
            self.push([dp(code, value, START + offset)], START + offset)
        verify = self.shadow(dp(SMOKE, "alarm", START + 80_000), dp(CO, "normal", START + 1400))
        self.assertIsNone(state.claim(self.db, "erhua", START + 95_000, verify_late=verify))
        self.assertIsNone(self.claim(START + 96_000))
        self.push([dp(SMOKE, "normal", START + 97_000)], START + 97_000)
        job = self.claim(START + 97_000)
        self.assertEqual((job["kind"], job["hazards"], job["reported_at"]), ("recovery", 3, START + 97_000))
        self.assertIsNone(self.claim(START + 98_000, "silaoshi"))

    def test_shadow_query_started_before_a_new_incident_cannot_clear_it(self):
        self.push([dp(SMOKE, "alarm", START)])
        self.claim(START)

        def verify():
            self.push([dp(SMOKE, "normal", START + 100_000)], START + 100_000)
            self.push([dp(CO, "alarm", START + 120_000)], START + 120_000)
            return [dp(CO, "normal", START + 121_000)]

        self.assertIsNone(state.claim(self.db, "erhua", START + 125_000, verify_late=verify))
        job = self.claim(START + 125_001)
        self.assertEqual((job["incident"], job["kind"], job["ordinal"], job["hazards"]), (2, "alarm", 1, 2))

    def test_short_followup_incident_preserves_final_recovery_for_both_groups(self):
        self.push([dp(SMOKE, "alarm", START)])
        for role in state.PROFILES:
            self.claim(START, role)
        self.push([dp(SMOKE, "normal", START + 1000)], START + 1000)
        self.push([dp(CO, "alarm", START + 1050)], START + 1050)
        self.push([dp(CO, "normal", START + 1150)], START + 1150)
        for role in state.PROFILES:
            job = self.claim(START + 1250, role)
            self.assertEqual((job["kind"], job["hazards"], job["reported_at"]),
                             ("recovery", 3, START + 1150))
            self.assertIsNone(self.claim(START + 60_000, role))

    def test_new_incident_supersedes_only_the_group_with_an_accepted_warning(self):
        self.push([dp(SMOKE, "alarm", START)])
        for role in state.PROFILES:
            self.claim(START, role)
        self.push([dp(SMOKE, "normal", START + 1000)], START + 1000)
        self.push([dp(CO, "alarm", START + 1050)], START + 1050)
        self.claim(START + 1100, "silaoshi")
        uncertain = state.claim(self.db, "erhua", START + 1100)
        state.finish(self.db, uncertain, "unknown", "timeout")
        self.push([dp(CO, "normal", START + 1500)], START + 1500)
        for role, incident, hazards in [("silaoshi", 2, 2), ("erhua", 1, 3)]:
            recovery = self.claim(START + 1600, role)
            self.assertEqual((recovery["kind"], recovery["incident"], recovery["hazards"]),
                             ("recovery", incident, hazards))
            self.assertIsNone(self.claim(START + 1700, role))

    def test_retracted_escalations_leave_original_reminder_deadlines(self):
        self.push([dp(SMOKE, "alarm", START)])
        for role in state.PROFILES:
            self.claim(START, role)
        for offset in (0, 250):
            for after, value in [(80, "alarm"), (180, "normal")]:
                at = START + offset + after
                self.push([dp(CO, value, at)], at)
            for role in state.PROFILES:
                self.assertIsNone(self.claim(START + offset + 250, role))
        for role in state.PROFILES:
            for ordinal, offset in [(2, 30_000), (3, 60_000)]:
                job = self.claim(START + offset, role)
                self.assertEqual((job["ordinal"], job["hazards"]), (ordinal, 1))
            self.assertIsNone(self.claim(START + 90_000, role))

    def test_prelaunch_failure_retries_same_warning_and_same_recovery(self):
        self.push([dp(CO, "alarm", START)])
        first = state.claim(self.db, "erhua", START)
        with patch.object(subprocess, "Popen", side_effect=BlockingIOError(errno.EAGAIN, "simulated")):
            status, receipt = dispatch.send(first, "qiwe:fixture", TEXT)
        self.assertEqual(status, "failed")
        state.finish(self.db, first, status, receipt, START)
        self.assertEqual(self.claim(START, "silaoshi")["ordinal"], 1)
        self.assertIsNone(self.claim(START + 4999))
        with closing(state.connect(self.path)) as restarted:
            retry = state.claim(restarted, "erhua", START + 5000)
            self.assertEqual((retry["ordinal"], retry["attempt"]), (1, 2))
            # A delayed duplicate completion for attempt 1 cannot overwrite the new reservation.
            state.finish(restarted, first, "failed", "late", START + 5100)
            state.finish(restarted, retry, "accepted", "mock-1")
        for ordinal, offset in [(2, 35_000), (3, 65_000)]:
            self.assertIsNone(self.claim(START + offset - 1))
            self.assertEqual(self.claim(START + offset)["ordinal"], ordinal)
        self.push([dp(CO, "normal", START + 70_000)], START + 70_000)
        recovery = state.claim(self.db, "erhua", START + 70_000)
        state.finish(self.db, recovery, "failed", "prelaunch", START + 70_000)
        self.assertEqual(self.claim(START + 70_000, "silaoshi")["kind"], "recovery")
        self.assertIsNone(self.claim(START + 74_999))
        retry = self.claim(START + 75_000)
        self.assertEqual((retry["kind"], retry["ordinal"], retry["attempt"]), ("recovery", 4, 2))
        self.assertIsNone(self.claim(START + 90_000))

    def test_prolonged_prelaunch_failure_retries_at_most_once_a_minute_then_delivers(self):
        self.push([dp(CO, "alarm", START)])
        for attempt, offset in enumerate((0, 5000, 30_000, 60_000, 120_000), 1):
            if offset:
                self.assertIsNone(self.claim(START + offset - 1))
            job = state.claim(self.db, "erhua", START + offset)
            self.assertEqual((job["ordinal"], job["attempt"]), (1, attempt))
            state.finish(self.db, job, "failed", "prelaunch", START + offset)
        self.assertEqual(self.claim(START + 125_000, "silaoshi")["ordinal"], 1)
        self.assertIsNone(self.claim(START + 179_999))
        for ordinal, offset in [(1, 180_000), (2, 210_000), (3, 240_000)]:
            self.assertEqual(self.claim(START + offset)["ordinal"], ordinal)
        self.assertIsNone(self.claim(START + 270_000))

    def test_cloud_latency_does_not_shorten_a_live_reservation(self):
        self.push([dp(SMOKE, "alarm", START)])
        with patch.object(state, "now_ms", side_effect=[START + 95_000, START + 105_000]):
            job = state.claim(self.db, "erhua", verify_late=lambda: [])
        self.assertEqual(job["started_at"], START + 105_000)
        self.assertIsNone(self.claim(START + 125_000))
        self.assertEqual(self.claim(START + 135_000)["ordinal"], 2)

    def test_withdrawn_escalation_before_prelaunch_retry_uses_original_deadline(self):
        self.push([dp(SMOKE, "alarm", START)])
        self.claim(START)
        self.push([dp(CO, "alarm", START + 1000)], START + 1000)
        expedited = state.claim(self.db, "erhua", START + 1000)
        self.assertEqual((expedited["ordinal"], expedited["hazards"]), (2, 3))
        state.finish(self.db, expedited, "failed", "prelaunch", START + 1000)
        self.push([dp(CO, "normal", START + 2000)], START + 2000)
        self.assertIsNone(self.claim(START + 6000))
        regular = self.claim(START + 30_000)
        self.assertEqual((regular["ordinal"], regular["attempt"], regular["hazards"]), (2, 2, 1))
        self.assertEqual(self.claim(START + 60_000)["ordinal"], 3)

    def test_postlaunch_io_error_or_uncertain_recovery_is_never_replayed(self):
        self.push([dp(CO, "alarm", START)])
        self.claim(START)
        self.push([dp(CO, "normal", START + 1000)], START + 1000)
        recovery = state.claim(self.db, "erhua", START + 1000)
        with patch.object(subprocess, "Popen", side_effect=OSError(errno.EIO, "unknown spawn outcome")):
            self.assertEqual(dispatch.send(recovery, "qiwe:fixture", TEXT)[0], "unknown")
        child = MagicMock()
        child.communicate.side_effect = OSError("lost the output pipe")
        with patch.object(subprocess, "Popen", return_value=child):
            status, receipt = dispatch.send(recovery, "qiwe:fixture", TEXT)
        self.assertEqual(status, "unknown")
        child.kill.assert_called_once_with()
        state.finish(self.db, recovery, status, receipt, START + 2000)
        self.assertIsNone(self.claim(START + 100_000))

    def test_upgrade_preserves_legacy_arm_time_receipts_and_consumed_slots(self):
        with closing(state.connect(self.path.with_name("legacy.db"), create=True)) as legacy:
            legacy.executescript("""
                CREATE TABLE installation(id INTEGER PRIMARY KEY, armed_at INTEGER NOT NULL);
                CREATE TABLE signals(code TEXT PRIMARY KEY, value TEXT, reported_at INTEGER);
                CREATE TABLE incidents(id INTEGER PRIMARY KEY, active INTEGER, hazards INTEGER, reported_at INTEGER);
                CREATE TABLE routes(incident INTEGER, profile TEXT, count INTEGER DEFAULT 0, first_at INTEGER,
                                    due INTEGER, PRIMARY KEY(incident,profile));
                CREATE TABLE deliveries(incident INTEGER, profile TEXT, ordinal INTEGER, kind TEXT, hazards INTEGER,
                    reported_at INTEGER, started_at INTEGER, status TEXT, receipt TEXT,
                    PRIMARY KEY(incident,profile,ordinal), FOREIGN KEY(incident,profile) REFERENCES routes);
            """)
            legacy.execute("INSERT INTO installation VALUES(1,?)", (START,))
            legacy.execute("INSERT INTO signals VALUES(?,?,?)", (SMOKE, "alarm", START))
            legacy.execute("INSERT INTO incidents VALUES(1,1,1,?)", (START,))
            legacy.execute("INSERT INTO routes VALUES(1,'erhua',2,?,?)", (START, START + 60_000))
            legacy.execute("INSERT INTO routes VALUES(1,'silaoshi',1,?,?)", (START, START + 30_000))
            legacy.execute("INSERT INTO deliveries VALUES(1,'silaoshi',1,'alarm',1,?,?,'failed','legacy')",
                           (START, START))
            for ordinal, status in [(1, "accepted"), (2, "unknown")]:
                legacy.execute("INSERT INTO deliveries VALUES(1,'erhua',?,'alarm',1,?,?,?,'old receipt')",
                               (ordinal, START, START + (ordinal - 1) * 30_000, status))
            for _ in range(2):
                state.initialize(legacy, START + 55_000)
            self.assertEqual(legacy.execute("SELECT armed_at FROM installation").fetchone()[0], START)
            self.assertIsNone(state.claim(legacy, "silaoshi", START + 29_999))
            self.assertEqual(state.claim(legacy, "silaoshi", START + 30_000)["ordinal"], 2)
            self.assertIsNone(state.claim(legacy, "erhua", START + 59_999))
            third = state.claim(legacy, "erhua", START + 60_000)
            self.assertEqual((third["ordinal"], third["attempt"]), (3, 1))
            state.finish(legacy, third, "accepted", "new receipt")
            state.apply(legacy, [dp(SMOKE, "normal", START + 61_000)], START + 61_000)
            self.assertEqual(state.claim(legacy, "erhua", START + 61_000)["kind"], "recovery")
            self.assertEqual(legacy.execute("PRAGMA foreign_key_check").fetchall(), [])

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
