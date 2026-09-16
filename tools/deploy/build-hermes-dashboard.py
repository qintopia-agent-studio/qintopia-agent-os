#!/usr/bin/env python3
"""Build immutable dashboard assets without editing the official core checkout.

Input is a git archive from a caller-pinned official commit. The generated pnpm lock
and all output hashes travel with the artifact. This script never contacts production.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "runtime/hermes"))
from dashboard_artifact import ArtifactError, digest, inventory, validate_entry


def build(source: Path, commit: str, output: Path) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or output.exists():
        raise ArtifactError("build_inputs_invalid")
    with tempfile.TemporaryDirectory(prefix="hermes-dashboard-build-") as temporary:
        root = Path(temporary).resolve()
        with tarfile.open(source, "r:gz") as archive:
            if archive.pax_headers.get("comment") != commit:
                raise ArtifactError("source_archive_commit_mismatch")
            selected = []
            for item in archive.getmembers():
                name = item.name
                if name in {"package.json", "package-lock.json"} or name.startswith(("web/", "apps/shared/", "hermes_cli/")):
                    if not (item.isfile() or item.isdir()) or ".." in Path(name).parts or name.startswith("/"):
                        raise ArtifactError("source_archive_entry_invalid")
                    selected.append(item)
            archive.extractall(root, members=selected, filter="data")
        core_files = {rel: digest(root / rel) for rel in (
            "hermes_cli/web_server.py", "hermes_cli/main_dashboard.py", "web/package.json", "package-lock.json",
        )}
        package = json.loads((root / "package.json").read_text())
        package["workspaces"] = ["web", "apps/shared"]
        (root / "package.json").write_text(json.dumps(package, indent=2) + "\n")
        (root / "pnpm-workspace.yaml").write_text(
            "packages:\n  - web\n  - apps/shared\nonlyBuiltDependencies:\n  - esbuild\n"
        )
        subprocess.run(["pnpm", "import"], cwd=root, check=True)
        subprocess.run(["pnpm", "install", "--frozen-lockfile", "--ignore-scripts"], cwd=root, check=True)
        # Keep the upstream typecheck result separate from the Vite production build.
        # The pinned release includes shared test imports and an optional-filter typing
        # error in its Vite config. Never report this check as passed when it is not.
        types = subprocess.run(["pnpm", "--filter", "web", "exec", "tsc", "-b"], cwd=root)
        command = ["pnpm", "--filter", "web", "exec", "vite", "build"]
        subprocess.run(command, cwd=root, check=True)
        dist = root / "hermes_cli/web_dist"
        files = inventory(dist)
        validate_entry(dist, files)
        output.mkdir(parents=True)
        shutil.copytree(dist, output / "web_dist")
        shutil.copyfile(root / "pnpm-lock.yaml", output / "pnpm-lock.yaml")
        runtime = {}
        for rel in ["dashboard_launcher.py", "dashboard_artifact.py"]:
            shutil.copyfile(REPO / "runtime/hermes" / rel, output / rel)
            runtime[rel] = digest(output / rel)
        manifest = {
            "schema_version": 1, "core_commit": commit, "core_files": core_files,
            "files": files, "runtime_files": runtime,
            "source_archive_sha256": digest(source),
            "pnpm_lock_sha256": digest(output / "pnpm-lock.yaml"),
            "build_command": command,
            "upstream_typecheck": "passed" if types.returncode == 0 else "failed",
        }
        (output / "dashboard-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        print("dashboard_manifest_sha256=" + digest(output / "dashboard-manifest.json"))
        print("dashboard_upstream_typecheck=" + manifest["upstream_typecheck"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", required=True, type=Path)
    parser.add_argument("--core-commit", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    build(args.source_archive.resolve(strict=True), args.core_commit, args.output.absolute())
