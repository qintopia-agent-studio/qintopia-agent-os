from __future__ import annotations

import hashlib
import json
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


BUNDLE_ROOT = Path(__file__).resolve().parents[1]
RENDERER = BUNDLE_ROOT / "render.py"
FIXTURE_VALUES = BUNDLE_ROOT / "tests" / "fixtures" / "values.json"


def run_renderer(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(RENDERER), *args],
        check=False,
        capture_output=True,
        text=True,
    )


class RenderProfileBundleTest(unittest.TestCase):
    def test_check_only_validates_package(self) -> None:
        result = run_renderer("--check-only")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "Xiaoman profile bundle check passed.\n")

    def test_fixture_render_is_complete_and_hashes_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "rendered"
            result = run_renderer(
                "--values-file",
                str(FIXTURE_VALUES),
                "--output-dir",
                str(output),
            )
            self.assertEqual(result.returncode, 0, result.stderr)

            soul = (output / "SOUL.md").read_text(encoding="utf-8")
            self.assertNotIn("{{", soul)
            self.assertIn("运营负责人甲", soul)
            self.assertIn("OperationsOwnerFixture", soul)
            self.assertIn("技术负责人乙", soul)
            self.assertIn("TechnicalHomeFixture", soul)

            manifest = json.loads((output / "bundle-manifest.json").read_text())
            self.assertEqual(manifest["status"], "observation-only")
            self.assertEqual(
                set(manifest["input_names"]),
                set(json.loads(FIXTURE_VALUES.read_text())),
            )
            for item in manifest["files"]:
                data = (output / item["path"]).read_bytes()
                self.assertEqual(item["sha256"], hashlib.sha256(data).hexdigest())
                mode = stat.S_IMODE((output / item["path"]).stat().st_mode)
                self.assertEqual(mode, int(item["mode"], 8))

    def test_missing_input_fails_without_creating_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            values = json.loads(FIXTURE_VALUES.read_text())
            missing_name = "QINTOPIA_XIAOMAN_OPERATIONS_OWNER_WECOM_TARGET"
            values.pop(missing_name)
            values_path = root / "values.json"
            values_path.write_text(json.dumps(values), encoding="utf-8")
            output = root / "rendered"
            result = run_renderer(
                "--values-file",
                str(values_path),
                "--output-dir",
                str(output),
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(missing_name, result.stderr)
            self.assertFalse(output.exists())

    def test_unallowlisted_input_fails_without_echoing_value(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            values = json.loads(FIXTURE_VALUES.read_text())
            values["UNREVIEWED_INPUT"] = "must-not-be-printed"
            values_path = root / "values.json"
            values_path.write_text(json.dumps(values), encoding="utf-8")
            output = root / "rendered"
            result = run_renderer(
                "--values-file",
                str(values_path),
                "--output-dir",
                str(output),
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("UNREVIEWED_INPUT", result.stderr)
            self.assertNotIn("must-not-be-printed", result.stderr)
            self.assertFalse(output.exists())

    def test_invalid_wecom_target_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            values = json.loads(FIXTURE_VALUES.read_text())
            values["QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL"] = "invalid target"
            values_path = root / "values.json"
            values_path.write_text(json.dumps(values), encoding="utf-8")
            output = root / "rendered"
            result = run_renderer(
                "--values-file",
                str(values_path),
                "--output-dir",
                str(output),
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL", result.stderr)
            self.assertNotIn("invalid target", result.stderr)
            self.assertFalse(output.exists())

    def test_non_utf8_values_file_fails_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            values_path = root / "values.json"
            values_path.write_bytes(b"\xff")
            output = root / "rendered"
            result = run_renderer(
                "--values-file",
                str(values_path),
                "--output-dir",
                str(output),
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("cannot read JSON file", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
