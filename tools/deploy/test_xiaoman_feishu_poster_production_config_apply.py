#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import io
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
    REPO_ROOT
    / "deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py"
)
SPEC = importlib.util.spec_from_file_location("xiaoman_poster_config_apply", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("cannot load Xiaoman poster configuration module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def sha256(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


class XiaomanPosterConfigApplyTest(unittest.TestCase):
    def setUp(self) -> None:
        parent = "/private/tmp" if Path("/private/tmp").is_dir() else None
        self.temp = tempfile.TemporaryDirectory(
            prefix="qintopia-xiaoman-poster-config-", dir=parent
        )
        self.root = Path(self.temp.name)
        self.release_sha = "a" * 40
        self.release_root = self.root / self.release_sha
        self.release_root.mkdir()
        self.release_root.chmod(0o755)
        self.release_current = self.root / "current"
        self.release_current.symlink_to(self.release_root)
        self.sidecar = self.root / "message-sidecar.env"
        self.hermes = self.root / "xiaoman.env"
        self.erhua = self.root / "erhua.env"
        self.lock = self.root / "config.lock"
        self.database_url = (
            "postgresql://poster_user:old-secret@db.example.internal:5432/qintopia"
        )
        self.callback_key = "fixture-callback-key-with-more-than-32-characters"
        database_hash = sha256(self.database_url)
        self.write_env(
            self.sidecar,
            {
                "QINTOPIA_SIDECAR_DATABASE_URL": self.database_url,
                "QINTOPIA_XIAOMAN_FEISHU_APP_ID": "cli_fixture_app",
                "QINTOPIA_XIAOMAN_FEISHU_APP_SECRET": "fixture-app-secret",
                "QINTOPIA_XIAOMAN_POSTER_MEDIA_ALLOWED_HOSTS": "media.example.test",
                "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY": self.callback_key,
                "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS": "oc_private",
                "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS": "ou_requester",
                "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS": "ou_requester,ou_reviewer",
                "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_DATABASE_URL_SHA256": database_hash,
                "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256": database_hash,
                "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256": database_hash,
                "QINTOPIA_UNRELATED_SETTING": "preserved",
            },
            0o640,
        )
        self.write_env(
            self.hermes,
            {
                "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY": self.callback_key,
                "QINTOPIA_UNRELATED_HERMES_SETTING": "preserved",
            },
            0o600,
        )
        self.write_env(
            self.erhua,
            {
                "QINTOPIA_SIDECAR_DATABASE_URL": self.database_url,
                "QINTOPIA_UNRELATED_ERHUA_SETTING": "preserved",
            },
            0o640,
        )

    def tearDown(self) -> None:
        self.temp.cleanup()

    @staticmethod
    def write_env(path: Path, values: dict[str, str], mode: int) -> None:
        path.write_text(
            "# fixture\n"
            + "\n".join(f"{name}='{value}'" for name, value in values.items())
            + "\n",
            encoding="utf-8",
        )
        path.chmod(mode)

    def direct_request(self, **overrides):
        request = {
            "schema_version": 1,
            "desired_state": "direct",
            "release_sha": self.release_sha,
            "database_url_sha256": sha256(self.database_url),
        }
        request.update(overrides)
        return request

    def run_config(self, request, *, apply=False, approval=""):
        return MODULE.configure(
            request=request,
            sidecar_path=self.sidecar,
            hermes_path=self.hermes,
            erhua_path=self.erhua,
            release_current_path=self.release_current,
            lock_path=self.lock,
            apply=apply,
            approval=approval,
            effective_uid=0,
        )

    def values(self, path: Path) -> dict[str, str]:
        return MODULE.parse_env_text(path.read_text(encoding="utf-8"), MODULE.ALL_TRACKED_KEYS)

    def apply_direct(self):
        return self.run_config(
            self.direct_request(), apply=True, approval=MODULE.APPLY_APPROVAL
        )

    def cleanup_stages(self):
        return MODULE.cleanup_production_stage_files(
            sidecar_path=self.sidecar,
            hermes_path=self.hermes,
            erhua_path=self.erhua,
            release_current_path=self.release_current,
            lock_path=self.lock,
            release_sha=self.release_sha,
            approval=MODULE.APPLY_APPROVAL,
            effective_uid=0,
        )

    def test_direct_preview_and_apply_generate_one_redacted_hmac(self) -> None:
        original_sidecar = self.sidecar.read_bytes()
        original_hermes = self.hermes.read_bytes()
        original_erhua = self.erhua.read_bytes()
        preview = self.run_config(self.direct_request())
        self.assertEqual(preview["action_status"], "production_config_ready")
        self.assertEqual(preview["ingress_hmac_action"], "generated")
        self.assertEqual(self.sidecar.read_bytes(), original_sidecar)
        self.assertEqual(self.hermes.read_bytes(), original_hermes)
        self.assertEqual(self.erhua.read_bytes(), original_erhua)

        report = self.apply_direct()
        sidecar = self.values(self.sidecar)
        hermes = self.values(self.hermes)
        ingress_key = sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"]
        self.assertEqual(ingress_key, hermes["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"])
        self.assertGreaterEqual(len(ingress_key), 32)
        self.assertNotEqual(ingress_key, self.callback_key)
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED"], "1")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"], "1")
        self.assertEqual(hermes["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"], "1")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"], "0")
        self.assertEqual(hermes["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"], "0")
        self.assertNotIn("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID", sidecar)
        self.assertNotIn("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID", hermes)
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS"], "oc_private")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS"], "ou_requester")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_POSTER_RELEASE_SHA"], self.release_sha)
        self.assertIn("QINTOPIA_UNRELATED_SETTING='preserved'", self.sidecar.read_text())
        self.assertIn("QINTOPIA_UNRELATED_HERMES_SETTING='preserved'", self.hermes.read_text())
        self.assertEqual(stat.S_IMODE(self.sidecar.stat().st_mode), 0o640)
        self.assertEqual(stat.S_IMODE(self.hermes.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(self.erhua.stat().st_mode), 0o640)
        self.assertFalse(report["erhua_database_binding_checked"])
        self.assertFalse(report["erhua_change_required"])
        self.assertEqual(report["shared_database_env_count"], 1)

        rendered_report = json.dumps(report, sort_keys=True)
        for sensitive in [
            self.database_url,
            self.callback_key,
            ingress_key,
            "oc_private",
            "ou_requester",
            str(self.root),
        ]:
            self.assertNotIn(sensitive, rendered_report)
        self.assertFalse(report["external_calls_executed"])
        self.assertFalse(report["database_writes_executed"])
        self.assertFalse(report["service_changes_executed"])

        repeated = self.apply_direct()
        self.assertTrue(repeated["deduped"])
        self.assertEqual(repeated["ingress_hmac_action"], "preserved")
        self.assertEqual(
            self.values(self.sidecar)["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"],
            ingress_key,
        )

    def test_database_rotation_updates_url_and_all_present_production_hashes(self) -> None:
        self.apply_direct()
        originals = {
            path: path.read_bytes() for path in (self.sidecar, self.hermes, self.erhua)
        }
        metadata = {
            path: (
                stat.S_IMODE(path.stat().st_mode),
                path.stat().st_uid,
                path.stat().st_gid,
            )
            for path in (self.sidecar, self.hermes, self.erhua)
        }
        new_url = (
            "postgresql://poster_user:new-secret@db.example.internal:5432/qintopia"
        )
        new_hash = sha256(new_url)
        request = self.direct_request(
            database_url=new_url,
            database_url_sha256=new_hash,
            previous_database_url_sha256=sha256(self.database_url),
            rotate_ingress_hmac=True,
        )
        preview = self.run_config(request)
        self.assertTrue(preview["erhua_database_binding_checked"])
        self.assertTrue(preview["erhua_change_required"])
        self.assertEqual(preview["shared_database_env_count"], 2)
        self.assertTrue(preview["previous_database_url_sha256_matched"])
        self.assertFalse(preview["external_calls_executed"])
        self.assertFalse(preview["database_writes_executed"])
        self.assertFalse(preview["service_changes_executed"])
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)

        report = self.run_config(request, apply=True, approval=MODULE.APPLY_APPROVAL)
        values = self.values(self.sidecar)
        erhua_values = self.values(self.erhua)
        self.assertEqual(values["QINTOPIA_SIDECAR_DATABASE_URL"], new_url)
        self.assertEqual(erhua_values["QINTOPIA_SIDECAR_DATABASE_URL"], new_url)
        self.assertIn(
            "QINTOPIA_UNRELATED_ERHUA_SETTING='preserved'",
            self.erhua.read_text(encoding="utf-8"),
        )
        for name in MODULE.PRODUCTION_DATABASE_HASH_KEYS:
            if name in values:
                self.assertEqual(values[name], new_hash)
        for path, expected in metadata.items():
            current = path.stat()
            self.assertEqual(
                (stat.S_IMODE(current.st_mode), current.st_uid, current.st_gid),
                expected,
            )
        self.assertTrue(report["database_url_rotated"])
        self.assertTrue(report["erhua_database_binding_checked"])
        self.assertTrue(report["erhua_change_required"])
        self.assertEqual(report["shared_database_env_count"], 2)
        self.assertTrue(report["previous_database_url_sha256_matched"])
        self.assertEqual(report["ingress_hmac_action"], "rotated")
        for rendered_report in (
            json.dumps(preview, sort_keys=True),
            json.dumps(report, sort_keys=True),
        ):
            self.assertNotIn(self.database_url, rendered_report)
            self.assertNotIn(new_url, rendered_report)
            self.assertNotIn(new_hash, rendered_report)
            self.assertNotIn(str(self.erhua), rendered_report)

    def test_group_and_disabled_states_use_the_same_transaction(self) -> None:
        self.apply_direct()
        direct_hmac = self.values(self.sidecar)[
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"
        ]
        group_request = {
            "schema_version": 1,
            "desired_state": "group",
            "release_sha": self.release_sha,
            "database_url_sha256": sha256(self.database_url),
            "bot_open_id": "ou_xiaoman_bot",
            "allowed_chat_ids": ["oc_private", "oc_internal"],
            "allowed_user_ids": ["ou_requester", "ou_reviewer"],
            "reviewer_user_ids": ["ou_requester", "ou_reviewer"],
        }
        report = self.run_config(
            group_request, apply=True, approval=MODULE.APPLY_APPROVAL
        )
        sidecar = self.values(self.sidecar)
        hermes = self.values(self.hermes)
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"], "1")
        self.assertEqual(hermes["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"], "1")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"], "ou_xiaoman_bot")
        self.assertEqual(hermes["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"], "ou_xiaoman_bot")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS"], "oc_internal,oc_private")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS"], "oc_internal,oc_private")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"], direct_hmac)
        self.assertEqual(report["chat_allowlist_count"], 2)

        self.apply_direct()
        sidecar = self.values(self.sidecar)
        hermes = self.values(self.hermes)
        self.assertNotIn("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID", sidecar)
        self.assertNotIn("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID", hermes)

        disabled = {
            "schema_version": 1,
            "desired_state": "disabled",
            "release_sha": self.release_sha,
        }
        self.run_config(disabled, apply=True, approval=MODULE.APPLY_APPROVAL)
        sidecar = self.values(self.sidecar)
        hermes = self.values(self.hermes)
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED"], "0")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"], "0")
        self.assertEqual(hermes["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"], "0")
        self.assertEqual(hermes["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"], "0")
        self.assertEqual(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"], direct_hmac)

    def test_database_rotation_retry_reconciles_interrupted_first_replace(self) -> None:
        self.apply_direct()
        new_url = (
            "postgresql://poster_user:new-secret@db.example.internal:5432/qintopia"
        )
        request = self.direct_request(
            database_url=new_url,
            database_url_sha256=sha256(new_url),
            previous_database_url_sha256=sha256(self.database_url),
        )
        normalized = MODULE.validate_request(request)
        interrupted_plan = MODULE.build_plan(
            normalized,
            MODULE.read_env(self.sidecar),
            MODULE.read_env(self.hermes),
            MODULE.read_env(self.erhua, MODULE.SHARED_DATABASE_TRACKED_KEYS),
            self.release_sha,
        )
        self.sidecar.write_text(interrupted_plan.sidecar_text, encoding="utf-8")
        original_hermes = self.hermes.read_bytes()
        original_erhua = self.erhua.read_bytes()

        preview = self.run_config(request)
        self.assertTrue(preview["database_url_rotated"])
        self.assertTrue(preview["erhua_database_binding_checked"])
        self.assertTrue(preview["erhua_change_required"])
        self.assertTrue(preview["previous_database_url_sha256_matched"])
        self.assertEqual(self.hermes.read_bytes(), original_hermes)
        self.assertEqual(self.erhua.read_bytes(), original_erhua)

        report = self.run_config(request, apply=True, approval=MODULE.APPLY_APPROVAL)
        self.assertTrue(report["previous_database_url_sha256_matched"])
        self.assertEqual(
            self.values(self.erhua)["QINTOPIA_SIDECAR_DATABASE_URL"], new_url
        )
        repeated = self.run_config(
            request, apply=True, approval=MODULE.APPLY_APPROVAL
        )
        self.assertTrue(repeated["deduped"])
        self.assertFalse(repeated["database_url_rotated"])
        self.assertTrue(repeated["erhua_database_binding_checked"])
        self.assertFalse(repeated["erhua_change_required"])
        self.assertFalse(repeated["previous_database_url_sha256_matched"])

    def test_invalid_boundaries_fail_before_mutation(self) -> None:
        original_sidecar = self.sidecar.read_bytes()
        original_hermes = self.hermes.read_bytes()
        invalid_requests = [
            {**self.direct_request(), "release_sha": "b" * 40},
            {**self.direct_request(), "database_url_sha256": "b" * 64},
            {**self.direct_request(), "unsupported": True},
            {**self.direct_request(), "bot_open_id": "ou_stale_bot"},
            {
                **self.direct_request(),
                "database_url": (
                    "postgresql://poster_user:new-secret@db.example.internal:5432/qintopia"
                ),
                "database_url_sha256": sha256(
                    "postgresql://poster_user:new-secret@db.example.internal:5432/qintopia"
                ),
            },
            {
                "schema_version": 1,
                "desired_state": "group",
                "release_sha": self.release_sha,
                "database_url_sha256": sha256(self.database_url),
            },
            {
                "schema_version": 1,
                "desired_state": "group",
                "release_sha": self.release_sha,
                "database_url_sha256": sha256(self.database_url),
                "bot_open_id": "ou_xiaoman_bot",
                "allowed_chat_ids": ["oc_internal"],
                "allowed_user_ids": ["ou_requester"],
                "reviewer_user_ids": ["ou_reviewer"],
            },
        ]
        for request in invalid_requests:
            with self.subTest(request=request.get("desired_state")):
                with self.assertRaises(MODULE.ConfigError):
                    self.run_config(
                        request, apply=True, approval=MODULE.APPLY_APPROVAL
                    )
                self.assertEqual(self.sidecar.read_bytes(), original_sidecar)
                self.assertEqual(self.hermes.read_bytes(), original_hermes)

        with self.assertRaises(MODULE.ConfigError):
            MODULE.configure(
                request=self.direct_request(),
                sidecar_path=self.sidecar,
                hermes_path=self.hermes,
                erhua_path=self.erhua,
                release_current_path=self.release_current,
                lock_path=self.lock,
                apply=True,
                approval="",
                effective_uid=0,
            )
        with self.assertRaises(MODULE.ConfigError):
            MODULE.configure(
                request=self.direct_request(),
                sidecar_path=self.sidecar,
                hermes_path=self.hermes,
                erhua_path=self.erhua,
                release_current_path=self.release_current,
                lock_path=self.lock,
                apply=False,
                approval="",
                effective_uid=501,
            )

        escaped_release = self.root / "escaped" / ("c" * 40)
        escaped_release.mkdir(parents=True)
        escaped_release.chmod(0o555)
        self.release_current.unlink()
        self.release_current.symlink_to(escaped_release)
        with self.assertRaises(MODULE.ConfigError):
            self.run_config(self.direct_request())

    def test_database_rotation_rejects_invalid_erhua_binding_before_write(self) -> None:
        self.apply_direct()
        new_url = (
            "postgresql://poster_user:new-secret@db.example.internal:5432/qintopia"
        )
        request = self.direct_request(
            database_url=new_url,
            database_url_sha256=sha256(new_url),
            previous_database_url_sha256=sha256(self.database_url),
        )

        mismatched_url = (
            "postgresql://poster_user:other-secret@db.example.internal:5432/qintopia"
        )
        self.write_env(
            self.erhua,
            {"QINTOPIA_SIDECAR_DATABASE_URL": mismatched_url},
            0o640,
        )
        originals = {
            path: path.read_bytes() for path in (self.sidecar, self.hermes, self.erhua)
        }
        with self.assertRaises(MODULE.ConfigError):
            self.run_config(request, apply=True, approval=MODULE.APPLY_APPROVAL)
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)

        self.erhua.write_text(
            "QINTOPIA_SIDECAR_DATABASE_URL='{}'\n"
            "QINTOPIA_SIDECAR_DATABASE_URL='{}'\n".format(
                self.database_url, self.database_url
            ),
            encoding="utf-8",
        )
        self.erhua.chmod(0o640)
        originals[self.erhua] = self.erhua.read_bytes()
        with self.assertRaises(MODULE.ConfigError):
            self.run_config(request, apply=True, approval=MODULE.APPLY_APPROVAL)
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)

    def test_database_rotation_rejects_unsafe_erhua_file_before_write(self) -> None:
        self.apply_direct()
        new_url = (
            "postgresql://poster_user:new-secret@db.example.internal:5432/qintopia"
        )
        request = self.direct_request(
            database_url=new_url,
            database_url_sha256=sha256(new_url),
            previous_database_url_sha256=sha256(self.database_url),
        )
        original_sidecar = self.sidecar.read_bytes()
        original_hermes = self.hermes.read_bytes()

        self.erhua.chmod(0o660)
        with self.assertRaises(MODULE.ConfigError):
            self.run_config(request, apply=True, approval=MODULE.APPLY_APPROVAL)
        self.assertEqual(self.sidecar.read_bytes(), original_sidecar)
        self.assertEqual(self.hermes.read_bytes(), original_hermes)

        real_erhua = self.root / "real-erhua.env"
        self.erhua.replace(real_erhua)
        real_erhua.chmod(0o640)
        self.erhua.symlink_to(real_erhua)
        with self.assertRaises(MODULE.ConfigError):
            self.run_config(request, apply=True, approval=MODULE.APPLY_APPROVAL)
        self.assertEqual(self.sidecar.read_bytes(), original_sidecar)
        self.assertEqual(self.hermes.read_bytes(), original_hermes)

    def test_disabled_and_unchanged_url_do_not_depend_on_erhua_env(self) -> None:
        self.erhua.unlink()
        report = self.apply_direct()
        self.assertFalse(report["erhua_database_binding_checked"])
        self.assertFalse(report["erhua_change_required"])
        self.assertEqual(report["shared_database_env_count"], 1)

        repeated = self.apply_direct()
        self.assertTrue(repeated["deduped"])
        disabled = {
            "schema_version": 1,
            "desired_state": "disabled",
            "release_sha": self.release_sha,
        }
        report = self.run_config(
            disabled, apply=True, approval=MODULE.APPLY_APPROVAL
        )
        self.assertFalse(report["erhua_database_binding_checked"])
        self.assertFalse(report["erhua_change_required"])
        self.assertEqual(report["shared_database_env_count"], 0)

    def test_incomplete_hmac_requires_explicit_rotation(self) -> None:
        with self.sidecar.open("a", encoding="utf-8") as handle:
            handle.write("QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY='one-sided-ingress-key-with-more-than-32-characters'\n")
        with self.assertRaises(MODULE.ConfigError):
            self.apply_direct()
        report = self.run_config(
            self.direct_request(rotate_ingress_hmac=True),
            apply=True,
            approval=MODULE.APPLY_APPROVAL,
        )
        self.assertEqual(report["ingress_hmac_action"], "rotated")
        self.assertEqual(
            self.values(self.sidecar)["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"],
            self.values(self.hermes)["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"],
        )

    def test_release_owner_and_write_boundaries_match_promoter(self) -> None:
        self.assertEqual(
            MODULE.resolve_release_sha(self.release_current), self.release_sha
        )

        with mock.patch.object(MODULE.os, "geteuid", return_value=os.geteuid() + 1):
            with self.assertRaises(MODULE.ConfigError):
                MODULE.resolve_release_sha(self.release_current)

        self.release_root.chmod(0o775)
        with self.assertRaises(MODULE.ConfigError):
            MODULE.resolve_release_sha(self.release_current)

    def test_document_commit_restores_all_files_when_third_replace_fails(self) -> None:
        self.apply_direct()
        originals = {
            path: path.read_bytes() for path in (self.sidecar, self.hermes, self.erhua)
        }
        new_url = (
            "postgresql://poster_user:new-secret@db.example.internal:5432/qintopia"
        )
        request = self.direct_request(
            database_url=new_url,
            database_url_sha256=sha256(new_url),
            previous_database_url_sha256=sha256(self.database_url),
            rotate_ingress_hmac=True,
        )
        real_replace = MODULE.os.replace
        failed = False

        def flaky_replace(source, target):
            nonlocal failed
            if Path(target) == self.erhua and not failed:
                failed = True
                raise OSError("fixture third replace failure")
            return real_replace(source, target)

        with mock.patch.object(MODULE.os, "replace", side_effect=flaky_replace):
            with self.assertRaises(OSError):
                self.run_config(
                    request, apply=True, approval=MODULE.APPLY_APPROVAL
                )
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)

    def test_orphaned_secret_stages_are_cleaned_and_exact_retry_stages_nothing(self) -> None:
        self.apply_direct()
        sidecar_document = MODULE.read_env(self.sidecar)
        hermes_document = MODULE.read_env(self.hermes)
        orphan = MODULE.stage_file(
            sidecar_document,
            sidecar_document.text + "QINTOPIA_TEST_SECRET='orphaned-value'\n",
        )
        legacy_orphan = self.hermes.parent / f".{self.hermes.name}.abcdefgh"
        legacy_orphan.write_text(
            hermes_document.text + "QINTOPIA_TEST_SECRET='legacy-orphan'\n",
            encoding="utf-8",
        )
        legacy_orphan.chmod(hermes_document.mode)

        with mock.patch.object(MODULE, "stage_file", wraps=MODULE.stage_file) as stage:
            report = self.apply_direct()
        self.assertFalse(orphan.exists())
        self.assertFalse(legacy_orphan.exists())
        self.assertEqual(report["staged_secret_files_removed_count"], 2)
        self.assertTrue(report["staged_secret_files_absent"])
        stage.assert_not_called()

        cleanup = self.cleanup_stages()
        self.assertEqual(cleanup["staged_secret_files_removed_count"], 0)
        self.assertTrue(cleanup["staged_secret_files_absent"])

    def test_unsafe_orphaned_config_stage_fails_closed(self) -> None:
        orphan = self.sidecar.parent / f".{self.sidecar.name}.abcdefgh"
        orphan.symlink_to(self.sidecar)
        originals = {
            path: path.read_bytes() for path in (self.sidecar, self.hermes, self.erhua)
        }
        with self.assertRaisesRegex(
            MODULE.ConfigError, "protected staged file boundary is invalid"
        ):
            self.cleanup_stages()
        self.assertTrue(orphan.is_symlink())
        for path, original in originals.items():
            self.assertEqual(path.read_bytes(), original)

    def test_zero_byte_stage_before_metadata_update_is_recoverable(self) -> None:
        document = MODULE.read_env(self.sidecar)
        future_owner = MODULE.EnvDocument(
            path=document.path,
            text=document.text,
            values=document.values,
            mode=document.mode,
            uid=document.uid + 1,
            gid=document.gid,
        )
        orphan = self.sidecar.parent / (
            f".{self.sidecar.name}.abcdefgh{MODULE.STAGE_SUFFIX}"
        )
        orphan.write_bytes(b"")
        orphan.chmod(0o600)
        self.assertEqual(
            MODULE.cleanup_orphaned_stage_files([future_owner]),
            1,
        )
        self.assertFalse(orphan.exists())

    def test_input_and_cli_surface_are_bounded(self) -> None:
        with self.assertRaises(MODULE.ConfigError):
            MODULE.load_request(
                io.BytesIO(b'{"schema_version":1,"schema_version":1}')
            )
        with self.assertRaises(MODULE.ConfigError):
            MODULE.load_request(io.BytesIO(b"{" + b"x" * MODULE.MAX_INPUT_BYTES + b"}"))
        script = SCRIPT_PATH.read_text(encoding="utf-8")
        for required in [
            'SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")',
            'HERMES_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/.env")',
            'ERHUA_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/erhua/.env")',
            'RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")',
            MODULE.APPLY_APPROVAL,
            'parser.add_argument("--stdin", action="store_true")',
        ]:
            self.assertIn(required, script)
        for forbidden in [
            "--test-mode",
            "--output",
            "systemctl",
            "curl ",
            "psql ",
            "source ",
            "eval ",
        ]:
            self.assertNotIn(forbidden, script)


if __name__ == "__main__":
    unittest.main(verbosity=2)
