#!/usr/bin/env python3
"""Repair the reviewed legacy weekly-plan wrapper, never arbitrary Hermes files."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import stat
import sys

NAME = "qintopia_xiaoman_weekly_plan_confirmation.sh"
TARGET = Path("/home/ubuntu/.hermes/scripts") / NAME
RELEASES = Path("/home/ubuntu/qintopia-agent-os-releases")


class RepairError(ValueError):
    pass


def open_regular(path: Path, flags: int = os.O_RDONLY) -> int:
    """Walk with directory descriptors so a replaced parent cannot redirect root I/O."""
    if not path.is_absolute() or ".." in path.parts:
        raise RepairError("path_invalid")
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.parts[1:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        result = os.open(path.name, flags | os.O_NOFOLLOW, dir_fd=fd)
    finally:
        os.close(fd)
    info = os.fstat(result)
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        os.close(result)
        raise RepairError("regular_file_required")
    return result


def checksum(fd: int) -> str:
    os.lseek(fd, 0, os.SEEK_SET)
    digest = hashlib.sha256()
    while chunk := os.read(fd, 65536):
        digest.update(chunk)
    return digest.hexdigest()


def inspect_wrapper(source: Path, target: Path, runtime_gid: int, *, apply: bool = False) -> dict:
    source_fd = open_regular(source)
    try:
        target_fd = open_regular(target)
    except FileNotFoundError:
        os.close(source_fd)
        return {"status": "absent", "changed": False}
    try:
        source_hash = checksum(source_fd)
        if checksum(target_fd) != source_hash:
            raise RepairError("wrapper_content_drift")
        info = os.fstat(target_fd)
        before = {"uid": info.st_uid, "gid": info.st_gid, "mode": stat.S_IMODE(info.st_mode)}
        desired = {"uid": 0, "gid": runtime_gid, "mode": 0o750}
        if apply and before != desired:
            os.fchown(target_fd, 0, runtime_gid)
            os.fchmod(target_fd, 0o750)
            os.fsync(target_fd)
        return {"status": "applied" if apply else "ready", "changed": before != desired,
                "before": before, "after": desired, "content_sha256": source_hash}
    finally:
        os.close(source_fd)
        os.close(target_fd)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", required=True, type=Path)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--apply", action="store_true")
    action.add_argument("--rollback", action="store_true")
    args = parser.parse_args()
    release = args.release
    if release.parent != RELEASES or not re.fullmatch(r"[0-9a-f]{40}", release.name):
        raise RepairError("immutable_release_required")
    if (args.apply or args.rollback) and os.geteuid() != 0:
        raise RepairError("root_required")
    source = release / "runtime/hermes/scripts" / NAME
    account = pwd.getpwnam("ubuntu")
    gid = account.pw_gid
    for path in (source.parent, TARGET.parent):
        for parent in (path, *path.parents):
            info = parent.lstat()
            if parent.is_symlink() or not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, account.pw_uid) or info.st_mode & 0o022:
                raise RepairError("unsafe_parent_directory")
    info = source.lstat()
    if info.st_uid != 0 or info.st_mode & 0o022:
        raise RepairError("untrusted_release_source")
    preview = inspect_wrapper(source, TARGET, gid)
    if (args.apply or args.rollback) and preview["status"] != "absent":
        backup = Path("/var/lib/qintopia-agent-os-deploy/recovery")
        backup.mkdir(mode=0o700, parents=True, exist_ok=True)
        if backup.is_symlink() or backup.stat().st_uid != 0 or backup.stat().st_mode & 0o077:
            raise RepairError("backup_directory_invalid")
        record = backup / f"snapshot-wrapper-{release.name}.json"
        if args.rollback:
            fd = open_regular(record)
            try:
                saved = json.loads(os.read(fd, 16384))
            finally:
                os.close(fd)
            if saved["content_sha256"] != preview["content_sha256"]:
                raise RepairError("rollback_content_drift")
            fd = open_regular(TARGET)
            try:
                if checksum(fd) != saved["content_sha256"]:
                    raise RepairError("rollback_content_drift")
                before = saved["before"]
                os.fchown(fd, before["uid"], before["gid"])
                os.fchmod(fd, before["mode"])
                os.fsync(fd)
            finally:
                os.close(fd)
            print("hermes_snapshot_permissions=rolled_back acceptance=not_implied")
            return
        try:
            fd = os.open(record, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        except FileExistsError:
            fd = open_regular(record)
            try:
                previous = json.loads(os.read(fd, 16384))
                if previous.get("content_sha256") != preview["content_sha256"]:
                    raise RepairError("backup_identity_mismatch")
            finally:
                os.close(fd)
        else:
            with os.fdopen(fd, "w") as handle:
                json.dump(preview, handle, sort_keys=True)
                handle.flush()
                os.fsync(handle.fileno())
        preview = inspect_wrapper(source, TARGET, gid, apply=True)
    print(json.dumps({"hermes_snapshot_permissions": preview["status"], "changed": preview["changed"]}))


if __name__ == "__main__":
    try:
        main()
    except (RepairError, OSError, ValueError, KeyError) as exc:
        code = str(exc) if isinstance(exc, RepairError) else type(exc).__name__
        print(f"hermes_snapshot_permissions=blocked reason={code}", file=sys.stderr)
        raise SystemExit(1)
