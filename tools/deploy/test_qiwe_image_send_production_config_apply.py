#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


sys.dont_write_bytecode = True

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = (
    REPO_ROOT / "deploy/sidecar/scripts/apply-qiwe-image-send-production-config.py"
)
SPEC = importlib.util.spec_from_file_location("qiwe_send_config_apply", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load QiWe send configuration module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def sha256(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


class QiweImageSendProductionConfigApplyTest(unittest.TestCase):
    def setUp(self) -> None:
        parent = "/private/tmp" if Path("/private/tmp").is_dir() else None
        self.temp = tempfile.TemporaryDirectory(
            prefix="qintopia-qiwe-send-config-", dir=parent
        )
        self.root = Path(self.temp.name)
        self.release_sha = "b" * 40
        self.release_root = self.root / self.release_sha
        self.release_root.mkdir()
        self.release_root.chmod(0o755)
        self.release_current = self.root / "current"
        self.release_current.symlink_to(self.release_root)
        self.sidecar = self.root / "message-sidecar.env"
        self.lock = self.root / "config.lock"
        self.database_url = (
            "postgresql://qiwe_user:old-secret@db.example.internal:5432/qintopia"
        )
        self.database_hash = sha256(self.database_url)
        self.qiwe_token = "fixture-qiwe-token-with-more-than-32-characters"
        self.base_env = {
            "QINTOPIA_SIDECAR_DATABASE_URL": self.database_url,
            "QIWE_API_URL": "https://manager.example.test/qiwe/api/qw/doApi",
            "QIWE_TOKEN": self.qiwe_token,
            "QIWE_GUID": "fixture-qiwe-guid",
            "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS": "manager.example.test",
            "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS": "media.example.test",
            "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS": "group_secret",
            "QINTOPIA_QIWE_IMAGE_SEND_ENABLED": "0",
            "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL": MODULE.SEND_APPROVAL,
            "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256": self.database_hash,
            "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY": "1",
            "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED": "1",
            "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL": MODULE.FEISHU_MIRROR_APPROVAL,
            "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA": self.release_sha,
            "QINTOPIA_DEPLOYED_COMMIT_SHA": self.release_sha,
            "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256": self.database_hash,
            "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN": "bascnSecret",
            "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS": "bascnSecret",
            "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID": "tblSecret",
            "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS": "tblSecret",
            "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH": "/home/ubuntu/.hermes/profiles/huabaosi/.env",
            "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION": "huabaosi-generated-image-v1",
            "QINTOPIA_UNRELATED_SETTING": "preserved",
        }
        self.write_env(self.base_env)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_env(self, values: dict[str, str]) -> None:
        self.sidecar.write_text(
            "# fixture\n"
            + "\n".join(f"{name}={json.dumps(value)}" for name, value in values.items())
            + "\n",
            encoding="utf-8",
        )
        self.sidecar.chmod(0o640)

    def request(self, desired_state: str = "enabled", **overrides):
        request = {
            "schema_version": 1,
            "desired_state": desired_state,
            "release_sha": self.release_sha,
        }
        if desired_state == "enabled":
            request["database_url_sha256"] = self.database_hash
        request.update(overrides)
        return request

    def run_config(self, request, *, apply=False, approval=""):
        return MODULE.configure(
            request=request,
            sidecar_path=self.sidecar,
            release_current_path=self.release_current,
            lock_path=self.lock,
            release_root_path=self.root,
            apply=apply,
            approval=approval,
            effective_uid=0,
            expected_uid=self.sidecar.stat().st_uid,
            expected_gid=self.sidecar.stat().st_gid,
            expected_mode=0o640,
        )

    def values(self) -> dict[str, str]:
        return MODULE.parse_env_text(
            self.sidecar.read_text(encoding="utf-8"), MODULE.TRACKED_KEYS
        )

    def test_preview_and_apply_enable_without_leaking_sensitive_values(self) -> None:
        original = self.sidecar.read_bytes()
        preview = self.run_config(self.request())
        self.assertEqual(preview["action_status"], "qiwe_image_send_config_ready")
        self.assertTrue(preview["sidecar_change_required"])
        self.assertEqual(self.sidecar.read_bytes(), original)

        report = self.run_config(
            self.request(), apply=True, approval=MODULE.APPLY_APPROVAL
        )
        values = self.values()
        self.assertEqual(values["QINTOPIA_QIWE_IMAGE_SEND_ENABLED"], "1")
        self.assertEqual(
            values["QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL"],
            MODULE.SEND_APPROVAL,
        )
        self.assertEqual(
            values["QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256"],
            self.database_hash,
        )
        self.assertEqual(values["QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY"], "1")
        self.assertEqual(stat.S_IMODE(self.sidecar.stat().st_mode), 0o640)
        self.assertIn("QINTOPIA_UNRELATED_SETTING", self.sidecar.read_text())
        self.assertFalse(report["external_calls_executed"])
        self.assertFalse(report["database_writes_executed"])
        self.assertFalse(report["service_changes_executed"])

        rendered_report = json.dumps(report, sort_keys=True)
        for sensitive in [
            self.database_url,
            self.database_hash,
            self.qiwe_token,
            "group_secret",
            str(self.root),
        ]:
            self.assertNotIn(sensitive, rendered_report)

        repeated = self.run_config(
            self.request(), apply=True, approval=MODULE.APPLY_APPROVAL
        )
        self.assertTrue(repeated["deduped"])
        self.assertFalse(repeated["sidecar_change_required"])

    def test_disabled_only_flips_enable_flag(self) -> None:
        self.run_config(self.request(), apply=True, approval=MODULE.APPLY_APPROVAL)
        report = self.run_config(
            self.request("disabled"), apply=True, approval=MODULE.APPLY_APPROVAL
        )
        values = self.values()
        self.assertEqual(report["desired_state"], "disabled")
        self.assertEqual(values["QINTOPIA_QIWE_IMAGE_SEND_ENABLED"], "0")
        self.assertEqual(
            values["QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL"],
            MODULE.SEND_APPROVAL,
        )
        self.assertEqual(
            values["QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256"],
            self.database_hash,
        )
        self.assertEqual(values["QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY"], "1")

    def test_enable_rejects_unmatched_database_hash_before_mutation(self) -> None:
        original = self.sidecar.read_bytes()
        bad_request = self.request(database_url_sha256="c" * 64)
        with self.assertRaisesRegex(
            MODULE.ConfigError, "QiWe send production database hash is not approved"
        ):
            self.run_config(
                bad_request, apply=True, approval=MODULE.APPLY_APPROVAL
            )
        self.assertEqual(self.sidecar.read_bytes(), original)

    def test_enable_allows_persistent_release_identity_to_lag_current(self) -> None:
        updated = dict(self.base_env)
        updated["QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA"] = "c" * 40
        updated["QINTOPIA_DEPLOYED_COMMIT_SHA"] = "d" * 40
        self.write_env(updated)

        report = self.run_config(
            self.request(), apply=True, approval=MODULE.APPLY_APPROVAL
        )

        self.assertTrue(report["success"])
        rendered = self.sidecar.read_text()
        self.assertIn("QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA", rendered)
        self.assertIn("QINTOPIA_DEPLOYED_COMMIT_SHA", rendered)

    def test_enable_rejects_feishu_delivery_drift_before_mutation(self) -> None:
        for key, value, error in [
            (
                "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
                "approved-production-huabaosi-feishu-artifact-mirror",
                "Feishu primary-storage approval is not approved",
            ),
            (
                "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
                "d" * 64,
                "Feishu primary-storage database hash is not approved",
            ),
            (
                "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
                "legacy-schema",
                "Feishu primary-storage schema is not approved",
            ),
            (
                "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
                "tblSecret,tblOther",
                "Feishu artifact table allowlist is not exact",
            ),
        ]:
            updated = dict(self.base_env)
            updated[key] = value
            self.write_env(updated)
            before = self.sidecar.read_bytes()
            with self.assertRaisesRegex(MODULE.ConfigError, error):
                self.run_config(
                    self.request(), apply=True, approval=MODULE.APPLY_APPROVAL
                )
            self.assertEqual(self.sidecar.read_bytes(), before)

    def test_apply_requires_exact_owner_approval_and_root(self) -> None:
        with self.assertRaisesRegex(MODULE.ConfigError, "exact owner approval"):
            self.run_config(self.request(), apply=True, approval="")
        with self.assertRaisesRegex(MODULE.ConfigError, "requires root"):
            MODULE.configure(
                request=self.request(),
                sidecar_path=self.sidecar,
                release_current_path=self.release_current,
                lock_path=self.lock,
                release_root_path=self.root,
                apply=True,
                approval=MODULE.APPLY_APPROVAL,
                effective_uid=1000,
                expected_uid=self.sidecar.stat().st_uid,
                expected_gid=self.sidecar.stat().st_gid,
                expected_mode=0o640,
            )

    def test_failed_commit_removes_secret_stage_file(self) -> None:
        original = self.sidecar.read_bytes()
        with mock.patch.object(
            MODULE.os, "replace", side_effect=OSError("replace failed")
        ):
            with self.assertRaises(OSError):
                self.run_config(
                    self.request(), apply=True, approval=MODULE.APPLY_APPROVAL
                )
        self.assertEqual(self.sidecar.read_bytes(), original)
        self.assertEqual(
            list(self.root.glob(".message-sidecar.env.*.qintopia-stage")),
            [],
        )

    def test_release_current_must_match_request(self) -> None:
        with self.assertRaisesRegex(MODULE.ConfigError, "release/current"):
            self.run_config(self.request(release_sha="d" * 40))

    def test_release_current_must_stay_under_fixed_root(self) -> None:
        outside_root = self.root / "outside"
        outside_release = outside_root / self.release_sha
        outside_release.mkdir(parents=True)
        self.release_current.unlink()
        self.release_current.symlink_to(outside_release)
        with self.assertRaisesRegex(MODULE.ConfigError, "fixed release root"):
            self.run_config(self.request())

    def test_env_metadata_must_match_production_boundary(self) -> None:
        with self.assertRaisesRegex(MODULE.ConfigError, "owner is not approved"):
            MODULE.configure(
                request=self.request(),
                sidecar_path=self.sidecar,
                release_current_path=self.release_current,
                lock_path=self.lock,
                release_root_path=self.root,
                apply=False,
                approval="",
                effective_uid=0,
                expected_uid=0,
                expected_gid=self.sidecar.stat().st_gid,
                expected_mode=0o640,
            )
        with self.assertRaisesRegex(MODULE.ConfigError, "mode is not approved"):
            MODULE.configure(
                request=self.request(),
                sidecar_path=self.sidecar,
                release_current_path=self.release_current,
                lock_path=self.lock,
                release_root_path=self.root,
                apply=False,
                approval="",
                effective_uid=0,
                expected_uid=self.sidecar.stat().st_uid,
                expected_gid=self.sidecar.stat().st_gid,
                expected_mode=0o600,
            )


if __name__ == "__main__":
    unittest.main()
