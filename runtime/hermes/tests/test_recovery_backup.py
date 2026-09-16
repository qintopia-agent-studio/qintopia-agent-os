import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("recovery_backup", Path(__file__).resolve().parents[1] / "recovery_backup.py")
backup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(backup)


class RecoveryBackupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def test_consistent_wal_snapshot_contains_committed_rows_without_wal_copy(self):
        source = self.root / "source.db"
        connection = sqlite3.connect(source)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("CREATE TABLE fixture(value TEXT)")
        connection.execute("INSERT INTO fixture VALUES ('synthetic')")
        connection.commit()
        self.assertTrue(Path(str(source) + "-wal").exists())
        target = self.root / "backup.db"
        backup.backup_sqlite(source, target)
        with sqlite3.connect(target) as copy:
            self.assertEqual(copy.execute("SELECT value FROM fixture").fetchall(), [("synthetic",)])
            self.assertEqual(copy.execute("PRAGMA quick_check").fetchall(), [("ok",)])
        self.assertEqual(target.stat().st_mode & 0o777, 0o600)

    def test_file_copy_preserves_original_and_keeps_backup_private(self):
        source = self.root / "fixture.env"
        source.write_text("FIXTURE=synthetic\n")
        source.chmod(0o640)
        target = self.root / "copy/fixture.env"
        report = backup.capture_file(source, target)
        self.assertTrue(report["copied"])
        self.assertEqual(report["mode"], 0o640)
        self.assertEqual(source.stat().st_mode & 0o777, 0o640)
        self.assertEqual(target.stat().st_mode & 0o777, 0o600)
        self.assertEqual(source.read_bytes(), target.read_bytes())

    def test_symlink_is_inventory_only_and_never_followed(self):
        link = self.root / "link"
        link.symlink_to("/unavailable/private-source")
        target = self.root / "copy"
        record = backup.capture_file(link, target)
        self.assertFalse(record["copied"])
        self.assertFalse(target.exists())

    def test_existing_backup_and_hardlinked_sources_are_rejected(self):
        source = self.root / "fixture"
        source.write_text("fixture")
        target = self.root / "copy"
        target.write_text("previous")
        with self.assertRaises(FileExistsError):
            backup.capture_file(source, target)
        os.link(source, self.root / "alias")
        with self.assertRaises(ValueError):
            backup.capture_file(source, self.root / "new")
        self.assertEqual(target.read_text(), "previous")


if __name__ == "__main__":
    unittest.main()
