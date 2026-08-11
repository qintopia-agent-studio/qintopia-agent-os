#!/usr/bin/env python3
from __future__ import annotations

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKER_PATH = (
    REPO_ROOT
    / "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh"
)
BACKFILL_PATH = (
    REPO_ROOT
    / "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-backfill.sh"
)


class XiaomanDailyCaseReportBackfillWorkerTests(unittest.TestCase):
    def test_worker_date_override_requires_backfill_approval(self) -> None:
        worker = WORKER_PATH.read_text(encoding="utf-8")

        self.assertIn(
            'BACKFILL_APPROVAL="approved-production-xiaoman-daily-case-report-auto-publish-backfill"',
            worker,
        )
        self.assertIn(
            'if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL:-}" != "$BACKFILL_APPROVAL" ]]; then',
            worker,
        )
        self.assertIn(
            "xiaoman daily case report date override requires explicit backfill approval",
            worker,
        )
        self.assertIn(
            'report_date_args=(--date "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE")',
            worker,
        )
        self.assertIn(
            '"${report_date_args[@]}"',
            worker,
        )
        self.assertIn(
            'content_metrics = candidate.get("content_metrics") or {}',
            worker,
        )
        self.assertIn(
            'character_universe = rendered.get("character_universe") or {}',
            worker,
        )
        self.assertIn(
            '"content_metrics": {',
            worker,
        )
        self.assertIn(
            '"character_universe": {',
            worker,
        )
        self.assertIn(
            '"raw_messages_included": character_universe.get("raw_messages_included") is True',
            worker,
        )
        self.assertIn(
            '"profile_fact_text_included": character_universe.get("profile_fact_text_included") is True',
            worker,
        )
        self.assertIn(
            '"storyline_candidate_count": len(character_universe.get("storyline_candidates") or [])',
            worker,
        )
        self.assertIn(
            '"$PYTHON_BIN" - "$render_report" "$upload_report" "$publish_report"',
            worker,
        )
        self.assertNotIn(
            '"daily_report_markdown": rendered.get("daily_report_markdown")',
            worker,
        )
        self.assertNotIn(
            '"people": character_universe.get("people")',
            worker,
        )

    def test_backfill_starts_reviewed_service_with_single_date_override(self) -> None:
        backfill = BACKFILL_PATH.read_text(encoding="utf-8")

        for fragment in [
            'APPROVAL="approved-production-xiaoman-daily-case-report-auto-publish-backfill"',
            'EXPECTED_RELEASE_SHA="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_RELEASE_SHA:-}"',
            'BACKFILL_DATE="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE:-}"',
            'ENV_FILE="/etc/qintopia/message-sidecar.env"',
            'SYSTEMCTL="/usr/bin/systemctl"',
            'SERVICE_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.service"',
            'require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "1"',
            'require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID"',
            '"QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE=${BACKFILL_DATE}"',
            '"QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL=${APPROVAL}"',
            '"$SYSTEMCTL" start "$SERVICE_NAME"',
            '"$SYSTEMCTL" unset-environment',
        ]:
            self.assertIn(fragment, backfill)

        self.assertNotIn("run-qiwe-image-send-worker", backfill)
        self.assertNotIn("QIWE_TOKEN", backfill)
        self.assertNotIn("source ", backfill)
        self.assertNotIn("eval ", backfill)


if __name__ == "__main__":
    unittest.main()
