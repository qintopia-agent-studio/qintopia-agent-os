#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import types
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "apply_creative_profile_candidates.py"
SPEC = importlib.util.spec_from_file_location("apply_creative_profile_candidates", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("failed to load apply_creative_profile_candidates.py")
apply_profiles = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = apply_profiles
SPEC.loader.exec_module(apply_profiles)


class ApplyCreativeProfileCandidatesTest(unittest.TestCase):
    def payload(self, **overrides):
        candidate = {
            "candidate_key": "safe-candidate-key",
            "review_decision": "approved",
            "person_id": "11111111-1111-1111-1111-111111111111",
            "candidate_role_label": "活动推进者",
            "story_function": "推进剧情",
            "daily_arc": "近90天稳定复现，今天继续以「活动推进者」推进",
            "memory_weight_label": "近90天稳定复现 · 长期线索可用",
            "meme_seed": "可作为「活动推进者」连续出场回调",
            "callback_hint": "今天不是孤例，可以回看「活动推进者」的长期复现",
            "evidence_anchor": "daily_character_note:person-safe-key",
            "recurrence_evidence_count": 7,
            "minimum_recurrence_met": True,
            "profile_upgrade_status": "eligible_for_review",
            "profile_upgrade_reason": "近90天已有 7 次角色复现；今日同一身份 2 条发言支撑",
            "evidence_policy": "daily_character_note_or_quote_map",
            "public_surface_allowed": False,
        }
        candidate.update(overrides)
        return {
            "schema_version": 1,
            "source": "xiaoman-daily-creative-profile-review-v1",
            "character_universe_schema_version": "xiaoman-character-universe-v1",
            "reviewed_by": "owner-review",
            "reviewed_at": "2026-08-12T20:00:00+08:00",
            "candidates": [candidate],
        }

    def test_validate_payload_accepts_reviewed_eligible_candidate(self) -> None:
        candidates = apply_profiles._validate_payload(self.payload())

        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0].role_label, "活动推进者")
        self.assertEqual(candidates[0].recurrence_evidence_count, 7)

    def test_daily_note_only_candidate_is_rejected_for_apply_payload(self) -> None:
        with self.assertRaisesRegex(apply_profiles.ApplyError, "not eligible_for_review"):
            apply_profiles._validate_payload(
                self.payload(
                    profile_upgrade_status="daily_note_only",
                    minimum_recurrence_met=False,
                    recurrence_evidence_count=1,
                )
            )

    def test_payload_requires_reviewed_person_id_not_display_name_guessing(self) -> None:
        with self.assertRaisesRegex(apply_profiles.ApplyError, "reviewed UUID"):
            apply_profiles._validate_payload(self.payload(person_id="小雨"))

    def test_apply_uses_fixed_psql_stdin_and_does_not_echo_database_url(self) -> None:
        captured: dict[str, object] = {}

        def fake_run(args, *, input, env, text, capture_output, timeout, check):
            captured["args"] = args
            captured["input"] = input
            captured["env"] = env
            self.assertTrue(text)
            self.assertTrue(capture_output)
            self.assertEqual(timeout, 30)
            self.assertFalse(check)
            return types.SimpleNamespace(returncode=0, stdout="", stderr="")

        old_run = apply_profiles.subprocess.run
        apply_profiles.subprocess.run = fake_run
        try:
            candidates = apply_profiles._validate_payload(self.payload())
            apply_profiles._apply_with_psql(
                candidates,
                "postgresql://user:p%40ss@db.example:5433/qintopia?sslmode=require",
            )
        finally:
            apply_profiles.subprocess.run = old_run

        self.assertEqual(captured["args"][0], "/usr/bin/psql")
        self.assertNotIn("postgresql://", " ".join(captured["args"]))
        self.assertNotIn("postgresql://", " ".join(captured["env"].values()))
        self.assertEqual(captured["env"]["PATH"], "/usr/bin:/bin")
        self.assertIn("profile_kind = 'creative_profile'", captured["input"])
        self.assertIn("jsonb_to_recordset((:payload_json)::jsonb)", captured["input"])
        self.assertIn("member_facts_fact_text", captured["input"])
        self.assertIn("public_surface_allowed", captured["input"])
        self.assertNotIn("活动推进者", " ".join(captured["args"]))
        self.assertNotIn("11111111", " ".join(captured["args"]))

    def test_main_dry_run_reports_sanitized_counts_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            payload_path = Path(tmpdir) / "payload.json"
            payload_path.write_text(json.dumps(self.payload(), ensure_ascii=False), encoding="utf-8")
            old_argv = sys.argv
            try:
                sys.argv = [str(SCRIPT), "--payload-json", str(payload_path)]
                from io import StringIO
                import contextlib

                stdout = StringIO()
                with contextlib.redirect_stdout(stdout):
                    code = apply_profiles.main()
            finally:
                sys.argv = old_argv

        self.assertEqual(code, 0)
        report = json.loads(stdout.getvalue())
        self.assertFalse(report["apply_executed"])
        self.assertEqual(report["approved_candidate_count"], 1)
        self.assertFalse(report["person_ids_included"])
        self.assertNotIn("11111111", stdout.getvalue())

    def test_main_apply_requires_exact_owner_approval_before_database_url(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            payload_path = Path(tmpdir) / "payload.json"
            payload_path.write_text(json.dumps(self.payload(), ensure_ascii=False), encoding="utf-8")
            old_argv = sys.argv
            try:
                sys.argv = [
                    str(SCRIPT),
                    "--payload-json",
                    str(payload_path),
                    "--apply",
                    "--approval",
                    "wrong",
                ]
                from io import StringIO
                import contextlib

                stderr = StringIO()
                with contextlib.redirect_stderr(stderr):
                    code = apply_profiles.main()
            finally:
                sys.argv = old_argv

        self.assertEqual(code, 2)
        self.assertIn("exact owner approval", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
