"""Server-local evidence backup; never restore state or print its contents."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import stat
import subprocess
import sys
import time

PROFILES = ("default", "erhua", "guanerye", "huabaosi", "silaoshi", "wenyuange", "xiaoman")
HERMES = Path("/home/ubuntu/.hermes")
DESTINATION = Path("/var/lib/qintopia-hermes-recovery")


def backup_sqlite(source: Path, target: Path) -> None:
    if source.is_symlink() or not source.is_file():
        raise ValueError("database_source_invalid")
    fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    os.close(fd)
    deadline = time.monotonic() + 120
    def progress(status, remaining, total):
        if time.monotonic() > deadline:
            raise TimeoutError("database_backup_deadline")
    source_db = sqlite3.connect(source.as_uri() + "?mode=ro", uri=True, timeout=0)
    destination_db = sqlite3.connect(target)
    try:
        source_db.backup(destination_db, pages=256, progress=progress, sleep=0.1)
        if destination_db.execute("PRAGMA quick_check").fetchall() != [("ok",)]:
            raise ValueError("database_backup_integrity_failed")
    finally:
        destination_db.close()
        source_db.close()


def capture_file(source: Path, target: Path) -> dict:
    info = source.lstat()
    metadata = {"uid": info.st_uid, "gid": info.st_gid, "mode": stat.S_IMODE(info.st_mode)}
    if stat.S_ISLNK(info.st_mode):
        # Inventory links, never follow them out of the selected runtime tree.
        return {**metadata, "symlink": os.readlink(source), "copied": False}
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise ValueError("unsupported_state_file")
    target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if source.suffix == ".db":
        backup_sqlite(source, target)
    else:
        with source.open("rb") as src, target.open("xb") as dst:
            os.fchmod(dst.fileno(), 0o600)
            shutil.copyfileobj(src, dst)
            dst.flush()
            os.fsync(dst.fileno())
        after = source.lstat()
        if (info.st_ino, info.st_size, info.st_mtime_ns) != (after.st_ino, after.st_size, after.st_mtime_ns):
            raise ValueError("state_changed_during_backup")
    return {**metadata, "copied": True, "sha256": hashlib.sha256(target.read_bytes()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    release = Path(__file__).resolve().parents[2]
    if release.parent != Path("/home/ubuntu/qintopia-agent-os-releases") or not re.fullmatch(r"[0-9a-f]{40}", release.name):
        raise ValueError("immutable_release_required")
    if os.geteuid() != 0:
        raise ValueError("root_required")
    selected = []
    roots = [HERMES if name == "default" else HERMES / "profiles" / name for name in PROFILES]
    for profile, home in zip(PROFILES, roots):
        if home.is_symlink() or not home.is_dir():
            raise ValueError("profile_home_requires_review")
        if not (home / "config.yaml").is_file() or (home / "config.yaml").is_symlink():
            raise ValueError("profile_config_requires_review")
        paths = [home / name for name in ("config.yaml", ".env", "auth.json", "gateway_state.json")]
        paths += list(home.glob("*.db"))
        for directory in ("cron", "sessions", "memories", "memory", "scripts", "plugins"):
            folder = home / directory
            if folder.is_symlink():
                paths.append(folder)
            elif folder.is_dir():
                paths += [p for p in folder.rglob("*") if p.is_symlink() or p.is_file()]
        for path in sorted(set(paths)):
            if path.name in (".env", "auth.json") and path.is_symlink():
                raise ValueError("credential_link_requires_review")
            if path.is_symlink() or path.is_file():
                if path.name.endswith((".db-wal", ".db-shm", ".db-journal", ".pyc")):
                    continue
                selected.append((path, Path("profiles") / profile / path.relative_to(home)))
    bridge_config = Path("/etc/qintopia/silaoshi-script-action.json")
    if bridge_config.is_file() and not bridge_config.is_symlink():
        selected.append((bridge_config, Path("etc/qintopia/silaoshi-script-action.json")))
        secret = Path(json.loads(bridge_config.read_text())["secret_file"])
        if not secret.is_absolute() or secret.resolve().parent != Path("/etc/qintopia"):
            raise ValueError("bridge_secret_location_requires_review")
        selected.append((secret, Path("etc/qintopia") / secret.name))
    selected += [(p, Path("systemd") / p.name) for p in Path("/etc/systemd/system").glob("hermes-dashboard.service*") if p.is_file()]
    for base in (Path("/etc/systemd/system/hermes-dashboard.service.d"), Path("/home/ubuntu/.config/systemd/user")):
        if base.is_dir():
            selected += [(p, Path("units") / p.relative_to(Path("/"))) for p in base.rglob("*") if p.is_file() and ("hermes" in str(p) or "silaoshi" in str(p))]
    estimated = sum(p.stat().st_size for p, _ in selected if not p.is_symlink())
    if shutil.disk_usage("/var/lib").free < estimated * 2 + 1024**3:
        raise ValueError("insufficient_backup_space")
    if not args.apply:
        print(json.dumps({"hermes_recovery_backup": "ready", "profiles": 7, "selected_files": len(selected)}))
        return
    DESTINATION.mkdir(mode=0o700, exist_ok=True)
    info = DESTINATION.lstat()
    if DESTINATION.is_symlink() or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o700:
        raise ValueError("backup_root_unsafe")
    output = DESTINATION / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    output.mkdir(mode=0o700)
    records = {}
    for source, relative in selected:
        records[str(source)] = capture_file(source, output / relative)
    manifest = {"schema_version": 1, "complete": True, "scope": "selected_profile_state_and_unit_files", "release": release.name, "profiles": list(PROFILES),
                "official_core_commit": subprocess.check_output(["git", "-C", str(HERMES / "hermes-agent"), "rev-parse", "HEAD"], text=True, timeout=10).strip(),
                "current_release": str(Path("/home/ubuntu/qintopia-agent-os-releases/current").resolve()),
                "files": records, "consistency": "per_database_online_backup_not_global_transaction"}
    with (output / "manifest.json").open("x") as stream:
        os.fchmod(stream.fileno(), 0o600)
        json.dump(manifest, stream, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    print(json.dumps({"hermes_recovery_backup": "complete", "profiles": 7, "files": len(records), "database_snapshots": sum(p.suffix == ".db" for p, _ in selected)}))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, sqlite3.Error, subprocess.SubprocessError) as exc:
        # File paths and exception payloads can contain private state; keep them local.
        print("hermes_recovery_backup=blocked reason=" + type(exc).__name__, file=sys.stderr)
        raise SystemExit(1)
