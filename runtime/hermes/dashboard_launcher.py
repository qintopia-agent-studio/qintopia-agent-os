#!/usr/bin/env python3
"""Start only the dashboard using a pinned immutable core and prebuilt assets."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import runpy
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dashboard_artifact import ArtifactError, regular, validate

CORE_ROOT = Path("/var/lib/qintopia-hermes-core/releases")
DASHBOARD_ROOT = Path("/var/lib/qintopia-hermes-dashboard/releases")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", required=True, type=Path)
    parser.add_argument("--manifest-sha256", required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    artifact = args.artifact
    if artifact.parent != DASHBOARD_ROOT or artifact.name != args.manifest_sha256:
        raise ArtifactError("artifact_location_invalid")
    regular(artifact / "dashboard-manifest.json")
    raw = json.loads((artifact / "dashboard-manifest.json").read_text())
    core = CORE_ROOT / str(raw.get("core_commit", ""))
    validate(artifact, core, args.manifest_sha256)
    runtime = json.loads((core / "artifact-manifest.json").read_text())["runtime"]
    if runtime.get("interpreter_path") != "runtime/venv/bin/python":
        raise ArtifactError("interpreter_manifest_invalid")
    interpreter = core / "runtime/venv/bin/python"
    regular(interpreter)
    if Path(sys.executable).resolve() != interpreter:
        raise ArtifactError("interpreter_mismatch")
    site_packages = runtime.get("site_packages_path", "")
    if not re.fullmatch(r"runtime/venv/lib/python3\.\d+/site-packages", site_packages):
        raise ArtifactError("site_packages_invalid")
    if args.check:
        print("hermes_dashboard_preflight=passed")
        return
    os.environ["HERMES_WEB_DIST"] = str(artifact / "web_dist")
    os.environ["HERMES_HOME"] = "/home/ubuntu/.hermes"
    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"
    for key in ("HERMES_SERVE_HEADLESS", "HERMES_DESKTOP", "PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV"):
        os.environ.pop(key, None)
    os.chdir(core / "core")
    sys.path.insert(0, str(core / site_packages))
    sys.path.insert(0, str(core / "core"))
    sys.argv = ["hermes", "--profile", "default", "dashboard", "--host", "127.0.0.1", "--port", "9120", "--no-open", "--skip-build"]
    runpy.run_module("hermes_cli.main", run_name="__main__")


if __name__ == "__main__":
    try:
        main()
    except (ArtifactError, OSError, ValueError, KeyError) as exc:
        code = str(exc) if isinstance(exc, ArtifactError) else type(exc).__name__
        print(f"hermes_dashboard=blocked reason={code}", file=sys.stderr)
        raise SystemExit(1)
