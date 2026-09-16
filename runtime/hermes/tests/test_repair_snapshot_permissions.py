import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("repair_snapshot_permissions", Path(__file__).resolve().parents[1] / "repair_snapshot_permissions.py")
repair = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(repair)


class PermissionsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.source = self.root / "source.sh"
        self.target = self.root / "target.sh"
        self.source.write_text("#!/bin/sh\nexit 0\n")
        self.target.write_bytes(self.source.read_bytes())
        self.target.chmod(0o700)

    def test_preview_never_changes_metadata(self):
        before = self.target.stat()
        result = repair.inspect_wrapper(self.source, self.target, 1000)
        self.assertEqual(result["status"], "ready")
        self.assertEqual(self.target.stat().st_mode, before.st_mode)

    def test_content_drift_refuses_even_apply(self):
        self.target.write_text("unreviewed secret-bearing runtime script")
        with patch.object(os, "fchown") as chown:
            with self.assertRaisesRegex(repair.RepairError, "content_drift"):
                repair.inspect_wrapper(self.source, self.target, 1000, apply=True)
            chown.assert_not_called()

    def test_parent_symlink_refused(self):
        (self.root / "alias").symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(OSError):
            repair.inspect_wrapper(self.source, self.root / "alias/target.sh", 1000)

    def test_target_symlink_and_hardlink_refused(self):
        self.target.unlink()
        self.target.symlink_to(self.source)
        with self.assertRaises(OSError):
            repair.inspect_wrapper(self.source, self.target, 1000)
        self.target.unlink()
        os.link(self.source, self.target)
        with self.assertRaisesRegex(repair.RepairError, "regular_file"):
            repair.inspect_wrapper(self.source, self.target, 1000)

    def test_apply_keeps_bytes_and_scopes_mode(self):
        with patch.object(os, "fchown") as chown:
            repair.inspect_wrapper(self.source, self.target, os.getgid(), apply=True)
            chown.assert_called_once()
        self.assertEqual(self.target.read_bytes(), self.source.read_bytes())
        self.assertEqual(self.target.stat().st_mode & 0o777, 0o750)


if __name__ == "__main__":
    unittest.main()
