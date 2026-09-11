from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
LAUNCHER_TEMPLATE = ROOT / "hermes-core-launcher.py"
COMMIT = "0123456789abcdef0123456789abcdef01234567"


class HermesCoreLauncherTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = Path(tempfile.mkdtemp(prefix="hermes-core-launcher-"))
        self.release = self.directory / COMMIT
        self.core = self.release / "core" / "hermes_cli"
        self.runtime = self.release / "runtime"
        self.venv_bin = self.runtime / "venv" / "bin"
        self.site_packages = self.runtime / "venv" / "lib" / "python3.12" / "site-packages"
        self.core.mkdir(parents=True)
        self.site_packages.mkdir(parents=True)
        self.venv_bin.mkdir(parents=True)
        # Match the builder's venv layout: a copied standalone interpreter
        # without pyvenv.cfg cannot locate its standard library.
        subprocess.run(
            [sys.executable, "-m", "venv", "--copies", "--without-pip", str(self.runtime / "venv")],
            check=True,
        )
        (self.venv_bin / "python").chmod(0o555)
        shutil.copy2(LAUNCHER_TEMPLATE, self.runtime / "hermes-core-launcher.py")
        (self.runtime / "hermes-core-launcher.py").chmod(0o555)
        (self.core / "__init__.py").write_text("", encoding="utf-8")
        (self.core / "main.py").write_text(
            "import json, os, sys\n"
            "print(json.dumps({'release': os.environ.get('QINTOPIA_HERMES_CORE_RELEASE'), "
            "'argv': sys.argv[1:], 'module': __file__}))\n",
            encoding="utf-8",
        )
        (self.site_packages / "hermes_cli").mkdir()
        (self.site_packages / "hermes_cli" / "__init__.py").write_text(
            "", encoding="utf-8"
        )
        (self.site_packages / "hermes_cli" / "main.py").write_text(
            "raise SystemExit('wrong interpreter source')\n", encoding="utf-8"
        )
        (self.release / "artifact-manifest.json").write_text(
            json.dumps(
                {
                    "runtime": {
                        "kind": "release-local-venv",
                        "platform": "linux-x86_64",
                        "interpreter_path": "runtime/venv/bin/python",
                        "site_packages_path": "runtime/venv/lib/python3.12/site-packages",
                        "launcher_path": "runtime/hermes-core-launcher.py",
                    }
                }
            ),
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        shutil.rmtree(self.directory)

    def run_launcher(self, extra_env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.pop("VIRTUAL_ENV", None)
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            [
                str(self.venv_bin / "python"),
                "-I",
                str(self.runtime / "hermes-core-launcher.py"),
                "--profile",
                "default",
                "gateway",
                "run",
                "--replace",
            ],
            cwd=self.release,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_uses_release_interpreter_and_core_before_site_packages(self) -> None:
        result = self.run_launcher()
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["release"], COMMIT)
        self.assertEqual(payload["argv"], ["--profile", "default", "gateway", "run", "--replace"])
        self.assertEqual(Path(payload["module"]).resolve(), (self.core / "main.py").resolve())

    def test_rejects_virtualenv_override(self) -> None:
        result = self.run_launcher({"VIRTUAL_ENV": "/tmp/old-hermes-venv"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("hermes_core_runtime_error=runtime_virtualenv_override", result.stderr)


if __name__ == "__main__":
    unittest.main()
