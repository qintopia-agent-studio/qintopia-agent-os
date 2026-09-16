"""Validate a prebuilt dashboard against its immutable official core.

The manifest is trusted only after the deployment caller pins its SHA-256. No build,
network request, profile mutation, or gateway restart occurs in this module.
"""
from __future__ import annotations

import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path, PurePosixPath
import re
import stat
from urllib.parse import unquote, urlsplit


class ArtifactError(ValueError):
    pass


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def regular(path: Path) -> None:
    for parent in (path, *path.parents):
        if parent.is_symlink():
            raise ArtifactError("symlink_rejected")
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise ArtifactError("regular_file_required")


def inventory(root: Path) -> dict[str, str]:
    result = {}
    if not root.is_dir() or root.is_symlink():
        raise ArtifactError("dist_missing")
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ArtifactError("symlink_rejected")
        if path.is_dir():
            continue
        regular(path)
        result[path.relative_to(root).as_posix()] = digest(path)
    return result


class EntryParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.assets: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = dict(attrs)
        if tag == "script" and values.get("src"):
            self.assets.append(values["src"])
        if tag == "link" and values.get("href") and values.get("rel") in {
            "stylesheet", "modulepreload", "preload", "icon",
        }:
            self.assets.append(values["href"])


def validate_entry(dist: Path, files: dict[str, str]) -> None:
    if "index.html" not in files:
        raise ArtifactError("index_missing")
    parser = EntryParser()
    parser.feed((dist / "index.html").read_text(encoding="utf-8"))
    if not parser.assets:
        raise ArtifactError("entry_assets_missing")
    for value in parser.assets:
        url = urlsplit(value)
        if url.scheme == "data":
            continue
        if url.scheme or url.netloc:
            raise ArtifactError("external_entry_asset_rejected")
        name = unquote(url.path).lstrip("/")
        if ".." in PurePosixPath(name).parts or name not in files:
            raise ArtifactError("entry_asset_missing")


def validate_payload(artifact: Path, expected_manifest: str) -> dict:
    if not re.fullmatch(r"[0-9a-f]{64}", expected_manifest):
        raise ArtifactError("manifest_digest_invalid")
    manifest_path = artifact / "dashboard-manifest.json"
    regular(manifest_path)
    if digest(manifest_path) != expected_manifest:
        raise ArtifactError("manifest_digest_mismatch")
    manifest = json.loads(manifest_path.read_text())
    if {p.name for p in artifact.iterdir()} != {
        "dashboard-manifest.json", "pnpm-lock.yaml", "web_dist",
        "dashboard_launcher.py", "dashboard_artifact.py",
    }:
        raise ArtifactError("artifact_layout_invalid")
    if set(manifest) != {
        "schema_version", "core_commit", "core_files", "files", "pnpm_lock_sha256",
        "source_archive_sha256", "build_command", "upstream_typecheck", "runtime_files",
    } or manifest["schema_version"] != 1:
        raise ArtifactError("manifest_schema_invalid")
    commit = manifest["core_commit"]
    if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ArtifactError("core_commit_invalid")
    files = inventory(artifact / "web_dist")
    if files != manifest["files"]:
        raise ArtifactError("dist_inventory_mismatch")
    validate_entry(artifact / "web_dist", files)
    regular(artifact / "pnpm-lock.yaml")
    if digest(artifact / "pnpm-lock.yaml") != manifest["pnpm_lock_sha256"]:
        raise ArtifactError("lock_digest_mismatch")
    if set(manifest["runtime_files"]) != {"dashboard_launcher.py", "dashboard_artifact.py"}:
        raise ArtifactError("runtime_inventory_invalid")
    for rel, checksum in manifest["runtime_files"].items():
        regular(artifact / rel)
        if digest(artifact / rel) != checksum:
            raise ArtifactError("runtime_digest_mismatch")
    return manifest


def validate(artifact: Path, core_release: Path, expected_manifest: str) -> dict:
    manifest = validate_payload(artifact, expected_manifest)
    commit = manifest["core_commit"]
    if core_release.name != commit:
        raise ArtifactError("core_commit_mismatch")
    regular(core_release / "artifact-manifest.json")
    core_manifest = json.loads((core_release / "artifact-manifest.json").read_text())
    if core_manifest.get("commit_sha") != commit or core_manifest.get("repository") != "https://github.com/NousResearch/hermes-agent.git":
        raise ArtifactError("official_core_mismatch")
    expected_core_files = {"hermes_cli/web_server.py", "hermes_cli/main_dashboard.py", "web/package.json", "package-lock.json"}
    if not isinstance(manifest["core_files"], dict) or set(manifest["core_files"]) != expected_core_files:
        raise ArtifactError("core_inventory_invalid")
    for rel, checksum in manifest["core_files"].items():
        path = core_release / "core" / rel
        regular(path)
        if digest(path) != checksum:
            raise ArtifactError("core_file_mismatch")
    return manifest
