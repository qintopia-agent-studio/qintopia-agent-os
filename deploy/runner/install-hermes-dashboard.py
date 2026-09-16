#!/usr/bin/env python3
"""Activate a checksum-pinned dashboard; touch no gateway or profile configuration.

Run from a reviewed immutable Qintopia release. The artifact must already be staged
under the fixed incoming root by the operator, with its manifest digest pinned.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "runtime/hermes"))
from dashboard_artifact import ArtifactError, digest, validate

ROOT = Path("/var/lib/qintopia-hermes-dashboard")
CORES = Path("/var/lib/qintopia-hermes-core/releases")
DROPIN = Path("/etc/systemd/system/hermes-dashboard.service.d/qintopia-release.conf")


def checked_directory(path: Path) -> None:
    for component in (path, *path.parents):
        info = component.lstat()
        if component.is_symlink() or not component.is_dir() or info.st_uid != 0 or info.st_mode & 0o022:
            raise ArtifactError("untrusted_directory")


def atomic_write(path: Path, payload: bytes, mode: int) -> None:
    checked_directory(path.parent)
    if path.is_symlink():
        raise ArtifactError("symlink_rejected")
    fd, temporary = tempfile.mkstemp(prefix=".dashboard-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            os.fchmod(stream.fileno(), mode)
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def render_unit(artifact: Path, commit: str, checksum: str) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise ArtifactError("identity_invalid")
    if artifact != ROOT / "releases" / checksum:
        raise ArtifactError("artifact_location_invalid")
    core = CORES / commit
    command = f"{core}/runtime/venv/bin/python -I {artifact}/dashboard_launcher.py --artifact {artifact} --manifest-sha256 {checksum}"
    return (
        "[Service]\nUser=ubuntu\nGroup=ubuntu\n"
        f"WorkingDirectory={core}/core\n"
        "Environment=HOME=/home/ubuntu\nEnvironment=PYTHONDONTWRITEBYTECODE=1\n"
        "ExecStartPre=\n" + f"ExecStartPre={command} --check\n"
        "ExecStart=\n" + f"ExecStart={command}\n"
    )


def smoke() -> None:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    base = "http://127.0.0.1:9120"
    with opener.open(base + "/", timeout=10) as response:
        if response.status != 200:
            raise ArtifactError("homepage_failed")
        html = response.read(1024 * 1024).decode()
    token = re.search(r'window\.__HERMES_SESSION_TOKEN__\s*=\s*"([^"]+)"', html)
    if token is None:
        raise ArtifactError("dashboard_session_unavailable")
    # The token and response bodies stay in memory; output contains no runtime data.
    for route in ["/api/profiles", "/api/sessions?limit=1", "/api/cron/jobs"]:
        request = urllib.request.Request(base + route, headers={"X-Hermes-Session-Token": token[1]})
        with opener.open(request, timeout=15) as response:
            if response.status != 200 or "application/json" not in response.headers.get("Content-Type", ""):
                raise ArtifactError("dashboard_api_failed")
            json.loads(response.read(4 * 1024 * 1024))
    for asset in re.findall(r'(?:src|href)="(/assets/[^"?#]+)', html):
        with opener.open(base + asset, timeout=10) as response:
            if response.status != 200 or "text/html" in response.headers.get("Content-Type", ""):
                raise ArtifactError("dashboard_asset_failed")
    try:
        opener.open("http://127.0.0.1:9119/", timeout=10)
    except urllib.error.HTTPError as exc:
        if exc.code != 401:
            raise ArtifactError("proxy_auth_failed") from None
    else:
        raise ArtifactError("proxy_auth_missing")


def restart() -> None:
    subprocess.run(["/usr/bin/systemctl", "daemon-reload"], check=True, capture_output=True)
    subprocess.run(["/usr/bin/systemctl", "restart", "hermes-dashboard.service"], check=True, capture_output=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-sha256", required=True)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--apply", action="store_true")
    action.add_argument("--rollback", action="store_true")
    parser.add_argument("--accept-upstream-typecheck-failure", action="store_true")
    args = parser.parse_args()
    checksum = args.manifest_sha256
    if not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise ArtifactError("manifest_digest_invalid")
    if os.geteuid() != 0:
        raise ArtifactError("root_required")
    if REPO.parent != Path("/home/ubuntu/qintopia-agent-os-releases") or not re.fullmatch(r"[0-9a-f]{40}", REPO.name):
        raise ArtifactError("reviewed_release_required")
    incoming = ROOT / "incoming" / checksum
    installed = ROOT / "releases" / checksum
    state = ROOT / "state" / checksum
    source = installed if installed.exists() else incoming
    checked_directory(source)
    if digest(source / "dashboard-manifest.json") != checksum:
        raise ArtifactError("manifest_digest_mismatch")
    raw = json.loads((source / "dashboard-manifest.json").read_text())
    commit = raw.get("core_commit", "")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ArtifactError("core_commit_invalid")
    manifest = validate(source, CORES / commit, checksum)
    checked_directory(CORES / commit)
    if manifest["upstream_typecheck"] != "passed" and not args.rollback and not args.accept_upstream_typecheck_failure:
        raise ArtifactError("upstream_typecheck_acceptance_required")
    if not args.apply and not args.rollback:
        print("hermes_dashboard_activation=ready")
        return
    for directory in [ROOT / "releases", ROOT / "state", state, DROPIN.parent]:
        directory.mkdir(mode=0o755 if directory != state else 0o700, parents=True, exist_ok=True)
        checked_directory(directory)
    record = state / "previous.json"
    unit = render_unit(installed, commit, checksum).encode()
    if args.rollback:
        if not record.is_file() or not DROPIN.is_file() or DROPIN.read_bytes() != unit:
            raise ArtifactError("rollback_state_mismatch")
        saved = json.loads(record.read_text())
        if saved["dropin"] is None:
            DROPIN.unlink()
        else:
            atomic_write(DROPIN, saved["dropin"].encode(), 0o644)
        restart()
        print("hermes_dashboard_activation=rolled_back acceptance=not_implied")
        return
    if not installed.exists():
        # Validate before and after copying; never execute user-owned staged scripts.
        staging = Path(tempfile.mkdtemp(prefix=".dashboard-", dir=ROOT / "releases"))
        try:
            shutil.copytree(source, staging, dirs_exist_ok=True, symlinks=True)
            validate(staging, CORES / commit, checksum)
            for path in [staging, *staging.rglob("*")]:
                os.chown(path, 0, 0)
                path.chmod(0o755 if path.is_dir() else 0o444)
            os.rename(staging, installed)
        finally:
            if staging.exists():
                shutil.rmtree(staging)
    # Finish the user-context preflight before recording or changing service state.
    python = CORES / commit / "runtime/venv/bin/python"
    subprocess.run(["/usr/sbin/runuser", "-u", "ubuntu", "--", str(python), "-I", str(installed / "dashboard_launcher.py"), "--artifact", str(installed), "--manifest-sha256", checksum, "--check"], check=True, capture_output=True)
    if DROPIN.is_symlink() or record.is_symlink():
        raise ArtifactError("state_symlink_rejected")
    current = DROPIN.read_text() if DROPIN.exists() else None
    if record.exists():
        saved = json.loads(record.read_text())
        # Retrying a failed/rolled-back activation is safe only at the saved state.
        if current not in (unit.decode(), saved["dropin"]):
            raise ArtifactError("activation_state_drift")
        if current == unit.decode():
            smoke()
            print("hermes_dashboard_activation=already_active browser_acceptance=pending")
            return
    else:
        atomic_write(record, json.dumps({"dropin": current}).encode(), 0o600)
    atomic_write(DROPIN, unit, 0o644)
    try:
        restart()
        for attempt in range(12):
            try:
                smoke()
                break
            except (OSError, ValueError):
                if attempt == 11:
                    raise
                time.sleep(2)
    except Exception:
        saved = json.loads(record.read_text())
        if saved["dropin"] is None:
            DROPIN.unlink(missing_ok=True)
        else:
            atomic_write(DROPIN, saved["dropin"].encode(), 0o644)
        restart()
        raise ArtifactError("activation_failed_previous_restored") from None
    print("hermes_dashboard_activation=passed browser_acceptance=pending")


if __name__ == "__main__":
    try:
        main()
    except (ArtifactError, OSError, ValueError, KeyError, subprocess.CalledProcessError) as exc:
        code = str(exc) if isinstance(exc, ArtifactError) else type(exc).__name__
        print(f"hermes_dashboard_activation=blocked reason={code}", file=sys.stderr)
        raise SystemExit(1)
