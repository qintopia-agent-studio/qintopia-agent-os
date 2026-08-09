#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import os
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = (
    REPO_ROOT
    / "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py"
)
spec = importlib.util.spec_from_file_location(
    "xiaoman_daily_case_report_config", MODULE_PATH
)
MODULE = importlib.util.module_from_spec(spec)
assert spec and spec.loader
sys.modules[spec.name] = MODULE
spec.loader.exec_module(MODULE)


class XiaomanDailyCaseReportProductionConfigTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.release_sha = "0123456789abcdef0123456789abcdef01234567"
        self.release_root = self.root / "releases"
        self.release_dir = self.release_root / self.release_sha
        self.release_current = self.release_root / "current"
        self.release_dir.mkdir(parents=True)
        self.release_current.symlink_to(self.release_dir)
        self.env_path = self.root / "message-sidecar.env"
        self.lock_path = self.root / "locks/config.lock"
        self.database_url = "postgres://qintopia:secret@127.0.0.1:5432/qintopia"
        self.database_hash = hashlib.sha256(self.database_url.encode()).hexdigest()
        self.env_path.write_text(self.base_env_text(), encoding="utf-8")
        self.env_path.chmod(0o640)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def base_env_text(self) -> str:
        return "\n".join(
            [
                f"QINTOPIA_SIDECAR_DATABASE_URL={self.database_url}",
                "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1",
                "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY=1",
                "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send",
                f"QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256={self.database_hash}",
                "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=media.example.test,assets.example.test",
                "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS=group-alpha,group-beta",
                "QINTOPIA_XIAOMAN_ACTIVITY_TARGET_GROUP_ID=group-alpha",
                "UNRELATED_KEY=preserved",
                "",
            ]
        )

    def enabled_request(self) -> dict[str, object]:
        return {
            "schema_version": 1,
            "desired_state": "enabled",
            "release_sha": self.release_sha,
            "database_url_sha256": self.database_hash,
            "chat_id": "chat-alpha",
            "target_group_id": "group-alpha",
            "media_upload_endpoint": "https://upload.example.test/daily",
            "media_public_base_url": "https://media.example.test/daily",
            "media_allowed_hosts": "media.example.test",
            "message_text": "小满日报已自动生成。",
        }

    def configure(
        self,
        request: dict[str, object],
        *,
        apply: bool = False,
        approval: str = "",
        effective_uid: int | None = 0,
    ) -> dict[str, object]:
        return MODULE.configure(
            request=request,
            sidecar_path=self.env_path,
            release_current_path=self.release_current,
            release_root_path=self.release_root,
            lock_path=self.lock_path,
            apply=apply,
            approval=approval,
            effective_uid=effective_uid if effective_uid is not None else os.geteuid(),
            expected_uid=os.getuid(),
            expected_gid=os.getgid(),
            expected_mode=0o640,
        )

    def test_enable_apply_writes_only_reviewed_daily_report_keys(self) -> None:
        report = self.configure(
            self.enabled_request(),
            apply=True,
            approval=MODULE.APPLY_APPROVAL,
        )

        self.assertTrue(report["success"])
        self.assertTrue(report["auto_publish_enabled"])
        self.assertTrue(report["target_group_allowlisted"])
        self.assertTrue(report["media_boundary_bound"])
        self.assertFalse(report["external_calls_executed"])
        self.assertFalse(report["database_writes_executed"])
        self.assertFalse(report["service_changes_executed"])

        text = self.env_path.read_text(encoding="utf-8")
        self.assertIn("UNRELATED_KEY=preserved", text)
        self.assertIn(MODULE.MANAGED_COMMENT, text)
        self.assertIn("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1", text)
        self.assertIn(
            "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL="
            "approved-production-xiaoman-daily-case-report-auto-publish",
            text,
        )
        self.assertIn("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE=1", text)
        self.assertIn("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID=group-alpha", text)
        self.assertIn(
            "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL="
            "https://media.example.test/daily",
            text,
        )
        self.assertEqual(self.env_path.stat().st_mode & 0o777, 0o640)

        second = self.configure(
            self.enabled_request(),
            apply=True,
            approval=MODULE.APPLY_APPROVAL,
        )
        self.assertTrue(second["deduped"])

    def test_disable_apply_sets_only_persistent_enablement_to_zero(self) -> None:
        request = {
            "schema_version": 1,
            "desired_state": "disabled",
            "release_sha": self.release_sha,
        }
        report = self.configure(request, apply=True, approval=MODULE.APPLY_APPROVAL)

        self.assertTrue(report["success"])
        self.assertFalse(report["auto_publish_enabled"])
        text = self.env_path.read_text(encoding="utf-8")
        self.assertIn("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0", text)
        self.assertNotIn("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID", text)

    def test_apply_requires_exact_owner_approval_and_root(self) -> None:
        with self.assertRaisesRegex(MODULE.ConfigError, "exact owner approval"):
            self.configure(self.enabled_request(), apply=True, approval="")
        with self.assertRaisesRegex(MODULE.ConfigError, "requires root"):
            self.configure(
                self.enabled_request(),
                apply=True,
                approval=MODULE.APPLY_APPROVAL,
                effective_uid=501,
            )

    def test_rejects_release_current_mismatch(self) -> None:
        request = self.enabled_request()
        request["release_sha"] = "fedcba9876543210fedcba9876543210fedcba98"
        with self.assertRaisesRegex(MODULE.ConfigError, "release/current"):
            self.configure(request)

    def test_rejects_unreviewed_media_host_or_group(self) -> None:
        media_request = self.enabled_request()
        media_request["media_public_base_url"] = "https://unreviewed.example.test/daily"
        media_request["media_allowed_hosts"] = "unreviewed.example.test"
        with self.assertRaisesRegex(MODULE.ConfigError, "media hosts"):
            self.configure(media_request)

        group_request = self.enabled_request()
        group_request["target_group_id"] = "group-beta"
        with self.assertRaisesRegex(MODULE.ConfigError, "reviewed Xiaoman target"):
            self.configure(group_request)

        outside_request = self.enabled_request()
        outside_request["target_group_id"] = "group-gamma"
        with self.assertRaisesRegex(MODULE.ConfigError, "not allowlisted"):
            self.configure(outside_request)

    def test_rejects_duplicate_or_unsafe_tracked_env_values(self) -> None:
        self.env_path.write_text(
            self.base_env_text()
            + "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0\n"
            + "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(MODULE.ConfigError, "duplicate tracked keys"):
            self.configure(self.enabled_request())

        self.env_path.write_text(
            self.base_env_text()
            + "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID=$(bad)\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(MODULE.ConfigError, "unsafe tracked values"):
            self.configure(self.enabled_request())


if __name__ == "__main__":
    unittest.main()
