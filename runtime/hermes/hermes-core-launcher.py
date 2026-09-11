#!/usr/bin/env python3
"""Run Hermes from the immutable release-local interpreter and source tree."""

from __future__ import annotations

import json
import os
import re
import runpy
import sys
from pathlib import Path


PROFILE_PATTERN = re.compile(r"^[a-z][a-z0-9]*$")
PROFILES = frozenset(
    {"default", "erhua", "guanerye", "huabaosi", "silaoshi", "wenyuange", "xiaoman"}
)
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")


def fail(code: str) -> "NoReturn":
    print("hermes_core_runtime=blocked", file=sys.stderr)
    print(f"hermes_core_runtime_error={code}", file=sys.stderr)
    raise SystemExit(1)


def regular_file(path: Path, executable: bool = False) -> Path:
    try:
        metadata = path.stat()
    except OSError:
        fail("runtime_entry_missing")
    if not path.is_file() or path.is_symlink() or metadata.st_nlink != 1:
        fail("runtime_entry_invalid")
    if executable and not os.access(path, os.X_OK):
        fail("runtime_entry_not_executable")
    return path.resolve(strict=True)


def release_root() -> Path:
    launcher = regular_file(Path(__file__), executable=True)
    if launcher.name != "hermes-core-launcher.py" or launcher.parent.name != "runtime":
        fail("runtime_launcher_location_invalid")
    release = launcher.parent.parent
    if release.is_symlink() or release.resolve(strict=True) != release:
        fail("runtime_release_alias_invalid")
    if not COMMIT_PATTERN.fullmatch(release.name):
        fail("runtime_release_identity_invalid")
    return release


def load_runtime(release: Path) -> tuple[Path, Path]:
    manifest_path = regular_file(release / "artifact-manifest.json")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        runtime = manifest["runtime"]
        interpreter_relative = runtime["interpreter_path"]
        site_packages_relative = runtime["site_packages_path"]
        launcher_relative = runtime["launcher_path"]
    except (KeyError, TypeError, ValueError, OSError, UnicodeError):
        fail("runtime_manifest_invalid")
    if (
        runtime.get("kind") != "release-local-venv"
        or runtime.get("platform") != "linux-x86_64"
        or interpreter_relative != "runtime/venv/bin/python"
        or launcher_relative != "runtime/hermes-core-launcher.py"
        or not isinstance(site_packages_relative, str)
        or not re.fullmatch(
            r"runtime/venv/lib/python[0-9]+\.[0-9]+/site-packages",
            site_packages_relative,
        )
    ):
        fail("runtime_manifest_invalid")

    interpreter = regular_file(release / interpreter_relative, executable=True)
    launcher = regular_file(release / launcher_relative, executable=True)
    site_packages = release / site_packages_relative
    if (
        site_packages.is_symlink()
        or not site_packages.is_dir()
        or site_packages.resolve(strict=True) != site_packages
    ):
        fail("runtime_site_packages_invalid")
    try:
        interpreter.relative_to(release / "runtime" / "venv")
        launcher.relative_to(release)
        site_packages.relative_to(release / "runtime" / "venv")
    except ValueError:
        fail("runtime_path_escape")
    if Path(sys.executable).resolve(strict=True) != interpreter:
        fail("runtime_interpreter_mismatch")
    if os.environ.get("VIRTUAL_ENV"):
        fail("runtime_virtualenv_override")
    return interpreter, site_packages


def main() -> int:
    args = sys.argv[1:]
    if (
        len(args) != 5
        or args[0] != "--profile"
        or not PROFILE_PATTERN.fullmatch(args[1])
        or args[1] not in PROFILES
        or args[2:] != ["gateway", "run", "--replace"]
    ):
        fail("runtime_invocation_invalid")

    release = release_root()
    interpreter, site_packages = load_runtime(release)
    core = release / "core"
    if core.is_symlink() or not core.is_dir() or core.resolve(strict=True) != core:
        fail("runtime_core_invalid")
    sys.path.insert(0, str(site_packages))
    sys.path.insert(0, str(core))
    os.chdir(release)
    os.environ["QINTOPIA_HERMES_CORE_RELEASE"] = release.name
    sys.argv = [str(core / "hermes_cli" / "main.py"), *args]
    runpy.run_module("hermes_cli.main", run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
