#!/usr/bin/env python3
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "build_creative_profile_review_payload.py"
SPEC = importlib.util.spec_from_file_location("build_creative_profile_review_payload", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("failed to load build_creative_profile_review_payload.py")
build_payload = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = build_payload
SPEC.loader.exec_module(build_payload)


class BuildCreativeProfileReviewPayloadTest(unittest.TestCase):
    def universe(self, candidate_overrides=None):
        candidate = {
            "key": "person-safe-key-活动推进者-creative-profile",
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
        if candidate_overrides:
            candidate.update(candidate_overrides)
        return {
            "schema_version": "xiaoman-character-universe-v1",
            "raw_messages_included": False,
            "profile_fact_text_included": False,
            "creative_profile_candidate_policy": {
                "public_surface_allowed": False,
            },
            "creative_profile_candidates": [candidate],
        }

    def test_build_payload_defaults_to_review_draft_without_person_id(self) -> None:
        payload = build_payload._build_payload(
            self.universe(),
            reviewed_by="owner-review",
            reviewed_at="2026-08-13T10:00:00+08:00",
            include_rejected=False,
        )

        self.assertEqual(payload["source"], "xiaoman-daily-creative-profile-review-v1")
        self.assertEqual(payload["character_universe_schema_version"], "xiaoman-character-universe-v1")
        self.assertTrue(payload["review_notes"]["person_id_required"])
        self.assertFalse(payload["review_notes"]["display_name_binding_allowed"])
        candidate = payload["candidates"][0]
        self.assertEqual(candidate["review_decision"], "pending_review")
        self.assertEqual(candidate["person_id"], "")
        self.assertEqual(candidate["candidate_role_label"], "活动推进者")
        self.assertFalse(candidate["public_surface_allowed"])
        self.assertEqual(payload["review_notes"]["eligible_for_review_default"], "pending_review")
        self.assertTrue(payload["review_notes"]["apply_requires_owner_approved"])
        serialized = json.dumps(payload, ensure_ascii=False)
        self.assertNotIn("11111111", serialized)
        self.assertNotIn("fact_text should not enter review", serialized)

    def test_daily_note_only_candidates_are_omitted_by_default(self) -> None:
        with self.assertRaisesRegex(build_payload.PayloadBuildError, "no eligible"):
            build_payload._build_payload(
                self.universe(
                    {
                        "profile_upgrade_status": "daily_note_only",
                        "minimum_recurrence_met": False,
                        "recurrence_evidence_count": 1,
                    }
                ),
                reviewed_by="owner-review",
                reviewed_at="2026-08-13T10:00:00+08:00",
                include_rejected=False,
            )

    def test_include_rejected_keeps_daily_notes_for_review_but_not_apply(self) -> None:
        payload = build_payload._build_payload(
            self.universe(
                {
                    "profile_upgrade_status": "daily_note_only",
                    "minimum_recurrence_met": False,
                    "recurrence_evidence_count": 1,
                }
            ),
            reviewed_by="owner-review",
            reviewed_at="2026-08-13T10:00:00+08:00",
            include_rejected=True,
        )

        self.assertEqual(payload["candidates"][0]["review_decision"], "rejected")
        self.assertFalse(payload["candidates"][0]["minimum_recurrence_met"])
        self.assertEqual(payload["review_notes"]["daily_note_only_default"], "rejected")

    def test_raw_or_private_markers_are_rejected(self) -> None:
        with self.assertRaisesRegex(build_payload.PayloadBuildError, "forbidden raw/private marker"):
            build_payload._build_payload(
                self.universe({"callback_hint": "fact_text should not enter review"}),
                reviewed_by="owner-review",
                reviewed_at="2026-08-13T10:00:00+08:00",
                include_rejected=False,
            )

    def test_main_prints_payload_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            universe_path = Path(tmpdir) / "universe.json"
            universe_path.write_text(json.dumps(self.universe(), ensure_ascii=False), encoding="utf-8")
            old_argv = sys.argv
            try:
                sys.argv = [
                    str(SCRIPT),
                    "--character-universe-json",
                    str(universe_path),
                    "--reviewed-by",
                    "owner-review",
                    "--reviewed-at",
                    "2026-08-13T10:00:00+08:00",
                ]
                stdout = io.StringIO()
                with contextlib.redirect_stdout(stdout):
                    code = build_payload.main()
            finally:
                sys.argv = old_argv

        self.assertEqual(code, 0)
        payload = json.loads(stdout.getvalue())
        self.assertEqual(payload["candidates"][0]["person_id"], "")

    def test_main_reports_missing_universe_as_clean_error(self) -> None:
        old_argv = sys.argv
        try:
            sys.argv = [
                str(SCRIPT),
                "--character-universe-json",
                "/tmp/not-present-character-universe.json",
                "--reviewed-by",
                "owner-review",
            ]
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                code = build_payload.main()
        finally:
            sys.argv = old_argv

        self.assertEqual(code, 2)
        self.assertIn("character universe file cannot be read", stderr.getvalue())
        self.assertNotIn("Traceback", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
