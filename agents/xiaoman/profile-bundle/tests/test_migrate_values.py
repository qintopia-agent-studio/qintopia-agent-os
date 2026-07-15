from __future__ import annotations

import hashlib
import json
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


BUNDLE_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BUNDLE_ROOT))

import migrate_values as migration  # noqa: E402
import render as bundle_renderer  # noqa: E402


FIXTURE_VALUES = BUNDLE_ROOT / "tests" / "fixtures" / "values.json"


class MigrateProfileValuesTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.bundle = self.root / "bundle"
        shutil.copytree(BUNDLE_ROOT, self.bundle)
        self.live = self.root / "live"
        bundle_renderer.render(FIXTURE_VALUES, self.live, BUNDLE_ROOT)
        self.soul_before = (self.live / "SOUL.md").read_bytes()
        self.profile_before = (self.live / "profile.yaml").read_bytes()
        self.output_parent = self.root / "etc"
        self.output_parent.mkdir()
        self.output = self.output_parent / "xiaoman-profile-bundle-values.json"

        manifest_path = self.bundle / "bundle.json"
        manifest = json.loads(manifest_path.read_text())
        files = {item["target"]: item for item in manifest["files"]}
        files["SOUL.md"]["production_source_sha256"] = hashlib.sha256(
            self.soul_before
        ).hexdigest()
        files["profile.yaml"]["production_source_sha256"] = hashlib.sha256(
            self.profile_before
        ).hexdigest()
        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def migrate(self, approval: str = migration.APPROVAL_PHRASE) -> dict[str, object]:
        return migration.migrate_values(
            bundle_root=self.bundle,
            live_soul_path=self.live / "SOUL.md",
            live_profile_path=self.live / "profile.yaml",
            output_path=self.output,
            approval=approval,
            effective_uid=0,
            require_root_parent=False,
        )

    def test_check_only_validates_migration_contract(self) -> None:
        result = subprocess.run(
            ["python3", str(BUNDLE_ROOT / "migrate_values.py"), "--check-only"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout, "Xiaoman profile values migration check passed.\n"
        )

    def test_success_writes_complete_private_values_after_parity(self) -> None:
        report = self.migrate()
        self.assertEqual(
            report["status"], "xiaoman_profile_values_migration_succeeded"
        )
        self.assertTrue(report["output_created"])
        self.assertFalse(report["live_profile_modified"])
        self.assertFalse(report["symlink_created"])
        self.assertEqual(
            json.loads(self.output.read_text()),
            json.loads(FIXTURE_VALUES.read_text()),
        )
        self.assertEqual(stat.S_IMODE(self.output.stat().st_mode), 0o600)
        self.assertEqual((self.live / "SOUL.md").read_bytes(), self.soul_before)
        self.assertEqual((self.live / "profile.yaml").read_bytes(), self.profile_before)
        report_text = json.dumps(report)
        for value in json.loads(FIXTURE_VALUES.read_text()).values():
            self.assertNotIn(value, report_text)

    def test_approval_fails_before_reading_source(self) -> None:
        with self.assertRaisesRegex(migration.MigrationError, "exact .* approval"):
            migration.migrate_values(
                bundle_root=self.bundle,
                live_soul_path=self.root / "missing-soul",
                live_profile_path=self.root / "missing-profile",
                output_path=self.output,
                approval="wrong",
                effective_uid=0,
                require_root_parent=False,
            )
        self.assertFalse(self.output.exists())

    def test_non_root_fails_before_reading_source(self) -> None:
        with self.assertRaisesRegex(migration.MigrationError, "must run as root"):
            migration.migrate_values(
                bundle_root=self.bundle,
                live_soul_path=self.root / "missing-soul",
                live_profile_path=self.root / "missing-profile",
                output_path=self.output,
                approval=migration.APPROVAL_PHRASE,
                effective_uid=1000,
                require_root_parent=False,
            )
        self.assertFalse(self.output.exists())

    def test_source_drift_fails_without_output(self) -> None:
        with (self.live / "SOUL.md").open("ab") as handle:
            handle.write(b"drift\n")
        with self.assertRaisesRegex(migration.MigrationError, "source hash mismatch"):
            self.migrate()
        self.assertFalse(self.output.exists())

    def test_render_parity_failure_does_not_create_output(self) -> None:
        with (self.bundle / "templates" / "SOUL.md.template").open("a") as handle:
            handle.write("parity drift\n")
        with self.assertRaisesRegex(migration.MigrationError, "parity mismatch"):
            self.migrate()
        self.assertFalse(self.output.exists())

    def test_live_source_change_before_write_does_not_create_output(self) -> None:
        original_read = migration.read_regular_bytes
        soul_reads = 0

        def changing_read(path: Path, maximum_bytes: int) -> bytes:
            nonlocal soul_reads
            data = original_read(path, maximum_bytes)
            if path == self.live / "SOUL.md":
                soul_reads += 1
                if soul_reads == 2:
                    return data + b"drift\n"
            return data

        with mock.patch.object(migration, "read_regular_bytes", changing_read):
            with self.assertRaisesRegex(migration.MigrationError, "changed during"):
                self.migrate()
        self.assertFalse(self.output.exists())

    def test_existing_output_is_not_replaced(self) -> None:
        self.output.write_text("existing\n")
        with self.assertRaisesRegex(migration.MigrationError, "already exists"):
            self.migrate()
        self.assertEqual(self.output.read_text(), "existing\n")


if __name__ == "__main__":
    unittest.main()
