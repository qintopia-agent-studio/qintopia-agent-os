from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
VALIDATOR = ROOT / "runtime/hermes/validate_hermes_python.py"


class HermesPythonValidatorTests(unittest.TestCase):
    def run_validator(
        self, python: Path, venv: Path, release: Path, expect: int = 0
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            [
                os.sys.executable,
                str(VALIDATOR),
                "--python",
                str(python),
                "--venv-dir",
                str(venv),
                "--release-dir",
                str(release),
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(expect, result.returncode, result.stderr)
        return result

    def test_accepts_direct_venv_home_and_contained_release_entry(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory).resolve()
            venv = directory / "home/.hermes/hermes-agent/venv"
            venv_bin = venv / "bin"
            release = directory / "release"
            base_home = directory / "base-python"
            external_python = base_home / "python3.11"
            rogue = directory / "rogue-python"
            venv_bin.mkdir(parents=True)
            base_home.mkdir()
            release.mkdir()
            (venv / "pyvenv.cfg").write_text(f"home = {base_home}\n")
            external_python.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(external_python, 0o755)
            (venv_bin / "python").symlink_to(external_python)

            self.run_validator(venv_bin / "python", venv, release)

            rogue.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(rogue, 0o755)
            (venv_bin / "python").unlink()
            (venv_bin / "python").symlink_to(rogue)
            rejected_rogue = self.run_validator(
                venv_bin / "python", venv, release, expect=1
            )
            self.assertIn("does not match pyvenv.cfg home", rejected_rogue.stderr)

            rejected_external = self.run_validator(rogue, venv, release, expect=1)
            self.assertIn("fixed venv entry or remain inside", rejected_external.stderr)

            release_python = release / "python"
            release_python.symlink_to(external_python)
            escaped_release = self.run_validator(
                release_python, venv, release, expect=1
            )
            self.assertIn("fixed venv entry or remain inside", escaped_release.stderr)

            release_python.unlink()
            release_python.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(release_python, 0o755)
            self.run_validator(release_python, venv, release)

            (venv / "pyvenv.cfg").unlink()
            missing_metadata = self.run_validator(
                venv_bin / "python", venv, release, expect=1
            )
            self.assertIn("pyvenv.cfg", missing_metadata.stderr)

    def test_accepts_only_version_matched_uv_home_alias(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory).resolve() / "home"
            venv = home / ".hermes/hermes-agent/venv"
            venv_bin = venv / "bin"
            release = Path(directory).resolve() / "release"
            uv_root = home / ".local/share/uv/python"
            versioned_dir = uv_root / "cpython-3.11.15-linux-x86_64-gnu"
            versioned_bin = versioned_dir / "bin"
            versioned_python = versioned_bin / "python3.11"
            alias_dir = uv_root / "cpython-3.11-linux-x86_64-gnu"
            venv_bin.mkdir(parents=True)
            versioned_bin.mkdir(parents=True)
            release.mkdir()
            versioned_python.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(versioned_python, 0o755)
            alias_dir.symlink_to(versioned_dir)
            (venv / "pyvenv.cfg").write_text(f"home = {alias_dir / 'bin'}\n")
            (venv_bin / "python").symlink_to(versioned_python)

            self.run_validator(venv_bin / "python", venv, release)

            mismatched_dir = uv_root / "cpython-3.12.1-linux-x86_64-gnu"
            mismatched_bin = mismatched_dir / "bin"
            mismatched_python = mismatched_bin / "python3.12"
            mismatched_bin.mkdir(parents=True)
            mismatched_python.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(mismatched_python, 0o755)
            alias_dir.unlink()
            alias_dir.symlink_to(mismatched_dir)
            (venv_bin / "python").unlink()
            (venv_bin / "python").symlink_to(mismatched_python)
            mismatched = self.run_validator(
                venv_bin / "python", venv, release, expect=1
            )
            self.assertIn("version or platform does not match", mismatched.stderr)

            outside_dir = home.parent / "cpython-3.11.15-linux-x86_64-gnu"
            outside_bin = outside_dir / "bin"
            outside_python = outside_bin / "python3.11"
            outside_bin.mkdir(parents=True)
            outside_python.write_text("#!/bin/sh\nexit 0\n")
            os.chmod(outside_python, 0o755)
            alias_dir.unlink()
            alias_dir.symlink_to(outside_dir)
            (venv_bin / "python").unlink()
            (venv_bin / "python").symlink_to(outside_python)
            escaped = self.run_validator(
                venv_bin / "python", venv, release, expect=1
            )
            self.assertIn("one absolute in-root target", escaped.stderr)

            alias_dir.unlink()
            alias_dir.symlink_to(versioned_dir.name)
            (venv_bin / "python").unlink()
            (venv_bin / "python").symlink_to(versioned_python)
            relative_alias = self.run_validator(
                venv_bin / "python", venv, release, expect=1
            )
            self.assertIn("one absolute in-root target", relative_alias.stderr)


if __name__ == "__main__":
    unittest.main()
