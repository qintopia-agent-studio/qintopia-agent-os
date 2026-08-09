#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = (
    REPO_ROOT
    / "deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py"
)
spec = importlib.util.spec_from_file_location(
    "xiaoman_activity_read_through_config", MODULE_PATH
)
MODULE = importlib.util.module_from_spec(spec)
assert spec and spec.loader
sys.modules[spec.name] = MODULE
spec.loader.exec_module(MODULE)


class XiaomanActivityReadThroughProductionConfigTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.release_sha = "0123456789abcdef0123456789abcdef01234567"
        self.release_root = self.root / "releases"
        self.release_dir = self.release_root / self.release_sha
        self.release_current = self.release_root / "current"
        self.release_dir.mkdir(parents=True)
        self.release_current.symlink_to(self.release_dir)
        self.sidecar_env = self.root / "message-sidecar.env"
        self.profile_env = self.root / "xiaoman.env"
        self.lock_path = self.root / "locks/config.lock"
        self.sidecar_env.write_text(
            "QINTOPIA_SIDECAR_DATABASE_URL=postgres://local/db\n"
            "UNRELATED_KEY=preserved\n",
            encoding="utf-8",
        )
        self.sidecar_env.chmod(0o640)
        self.profile_env.write_text(self.profile_text(), encoding="utf-8")
        self.profile_env.chmod(0o600)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def profile_text(
        self,
        *,
        token: str = "base-token-alpha",
        allowed_tokens: str = "base-token-alpha,base-token-beta",
        profile_path: str | None = None,
    ) -> str:
        profile_path = profile_path or str(self.profile_env)
        base_token_key = MODULE.READ_THROUGH_KEYS[0]
        allowed_tokens_key = MODULE.READ_THROUGH_KEYS[1]
        plan_table_key = MODULE.READ_THROUGH_KEYS[2]
        occurrence_table_key = MODULE.READ_THROUGH_KEYS[3]
        profile_path_key = MODULE.READ_THROUGH_KEYS[4]
        return "\n".join(
            [
                self.env_line(base_token_key, token),
                self.env_line(allowed_tokens_key, allowed_tokens),
                self.env_line(plan_table_key, "plan-table-alpha"),
                self.env_line(occurrence_table_key, "occurrence-table-alpha"),
                self.env_line(profile_path_key, profile_path),
                "EXTRA_PROFILE_ONLY=profile-only-value",
                "",
            ]
        )

    def env_line(self, key: str, value: str) -> str:
        return f"{key}={value}"

    def configure(
        self,
        *,
        release_sha: str | None = None,
        apply: bool = False,
        approval: str = "",
        effective_uid: int = 0,
    ) -> dict[str, object]:
        return MODULE.configure(
            release_sha=release_sha or self.release_sha,
            apply=apply,
            approval=approval,
            effective_uid=effective_uid,
            sidecar_env_path=self.sidecar_env,
            profile_env_path=self.profile_env,
            release_current_path=self.release_current,
            release_root_path=self.release_root,
            lock_path=self.lock_path,
            expected_sidecar_uid=os.getuid(),
            expected_sidecar_gid=os.getgid(),
            expected_profile_uid=os.getuid(),
            expected_profile_gid=os.getgid(),
        )

    def test_apply_copies_only_reviewed_read_through_keys_without_leaking_values(
        self,
    ) -> None:
        report = self.configure(apply=True, approval=MODULE.APPLY_APPROVAL)

        self.assertTrue(report["success"])
        self.assertEqual(report["copied_key_count"], 5)
        self.assertTrue(report["feishu_base_mode_enabled"])
        self.assertTrue(report["sensitive_values_redacted"])
        self.assertFalse(report["external_calls_executed"])
        self.assertFalse(report["service_changes_executed"])

        text = self.sidecar_env.read_text(encoding="utf-8")
        self.assertIn("UNRELATED_KEY=preserved", text)
        self.assertIn(MODULE.MANAGED_COMMENT, text)
        for key in MODULE.READ_THROUGH_KEYS:
            self.assertIn(f"{key}=", text)
        self.assertIn("QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1", text)
        self.assertNotIn("EXTRA_PROFILE_ONLY", text)
        self.assertNotIn("profile-only-value", text)
        self.assertEqual(self.sidecar_env.stat().st_mode & 0o777, 0o640)

        redacted_report = json.dumps(report, sort_keys=True)
        self.assertNotIn("base-token-alpha", redacted_report)
        self.assertNotIn("plan-table-alpha", redacted_report)

        second = self.configure(apply=True, approval=MODULE.APPLY_APPROVAL)
        self.assertTrue(second["deduped"])

    def test_rejects_release_current_mismatch(self) -> None:
        with self.assertRaisesRegex(MODULE.ConfigError, "release/current"):
            self.configure(
                release_sha="fedcba9876543210fedcba9876543210fedcba98",
            )

    def test_apply_requires_exact_owner_approval_and_root(self) -> None:
        with self.assertRaisesRegex(MODULE.ConfigError, "exact owner approval"):
            self.configure(apply=True, approval="")
        with self.assertRaisesRegex(MODULE.ConfigError, "requires root"):
            self.configure(
                apply=True,
                approval=MODULE.APPLY_APPROVAL,
                effective_uid=501,
            )

    def test_rejects_bad_env_metadata_before_mutation(self) -> None:
        self.sidecar_env.chmod(0o600)
        before = self.sidecar_env.read_text(encoding="utf-8")
        with self.assertRaisesRegex(MODULE.ConfigError, "mode"):
            self.configure(apply=True, approval=MODULE.APPLY_APPROVAL)
        self.assertEqual(self.sidecar_env.read_text(encoding="utf-8"), before)

        self.sidecar_env.chmod(0o640)
        self.profile_env.chmod(0o640)
        with self.assertRaisesRegex(MODULE.ConfigError, "mode"):
            self.configure(apply=True, approval=MODULE.APPLY_APPROVAL)

    def test_rejects_duplicate_or_unsafe_profile_values(self) -> None:
        self.profile_env.write_text(
            self.profile_text()
            + self.env_line(MODULE.READ_THROUGH_KEYS[0], "base-token-beta")
            + "\n",
            encoding="utf-8",
        )
        self.profile_env.chmod(0o600)
        with self.assertRaisesRegex(MODULE.ConfigError, "duplicate"):
            self.configure()

        self.profile_env.write_text(
            self.profile_text().replace(
                "plan-table-alpha",
                "$(bad)",
            ),
            encoding="utf-8",
        )
        self.profile_env.chmod(0o600)
        with self.assertRaisesRegex(MODULE.ConfigError, "unsafe"):
            self.configure()

    def test_rejects_unallowlisted_token_or_wrong_profile_path(self) -> None:
        self.profile_env.write_text(
            self.profile_text(allowed_tokens="base-token-beta"),
            encoding="utf-8",
        )
        self.profile_env.chmod(0o600)
        with self.assertRaisesRegex(MODULE.ConfigError, "allowlisted"):
            self.configure()

        self.profile_env.write_text(
            self.profile_text(profile_path="/tmp/unreviewed-xiaoman.env"),
            encoding="utf-8",
        )
        self.profile_env.chmod(0o600)
        with self.assertRaisesRegex(MODULE.ConfigError, "fixed production path"):
            self.configure()


if __name__ == "__main__":
    unittest.main()
