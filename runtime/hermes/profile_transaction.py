#!/usr/bin/env python3
"""Atomic backup, activation, and restore for the fixed Erhua profile files."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import tempfile
from pathlib import Path

from render_profile_overlay import require_regular_input


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def file_record(path: Path) -> dict[str, str | int]:
    file_stat = path.stat()
    return {
        "sha256": digest(path),
        "mode": format(stat.S_IMODE(file_stat.st_mode), "04o"),
        "uid": file_stat.st_uid,
        "gid": file_stat.st_gid,
    }


def atomic_copy(
    source: Path,
    target: Path,
    mode: int,
    owner: tuple[int, int] | None = None,
) -> None:
    require_regular_input(source)
    if target.is_symlink() or target.parent.is_symlink():
        raise ValueError(f"symlinked transaction path is not allowed: {target}")
    fd, temporary = tempfile.mkstemp(prefix=f".{target.name}.", dir=target.parent)
    temporary_path = Path(temporary)
    try:
        if owner is not None:
            temporary_stat = os.fstat(fd)
            if (temporary_stat.st_uid, temporary_stat.st_gid) != owner:
                os.fchown(fd, *owner)
        with source.open("rb") as source_stream, os.fdopen(fd, "wb") as target_stream:
            shutil.copyfileobj(source_stream, target_stream)
            target_stream.flush()
            os.fsync(target_stream.fileno())
        os.chmod(temporary_path, mode)
        os.replace(temporary_path, target)
        fsync_directory(target.parent)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def atomic_json_write(path: Path, value: dict) -> None:
    if path.is_symlink() or path.parent.is_symlink():
        raise ValueError(f"symlinked transaction path is not allowed: {path}")
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary_path = Path(temporary)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(value, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
        fsync_directory(path.parent)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def fsync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def durable_mkdir(path: Path, mode: int) -> None:
    missing: list[Path] = []
    cursor = path
    while not cursor.exists():
        missing.append(cursor)
        cursor = cursor.parent
    path.mkdir(parents=True, mode=mode)
    os.chmod(path, mode)
    for created in reversed(missing):
        fsync_directory(created.parent)


def paths(args: argparse.Namespace) -> tuple[Path, Path, Path, Path]:
    return Path(args.config), Path(args.env), Path(args.backup_dir), Path(args.metadata)


def backup(args: argparse.Namespace) -> None:
    config, env, backup_dir, metadata = paths(args)
    require_regular_input(config)
    require_regular_input(env)
    if backup_dir.exists() or backup_dir.is_symlink():
        raise ValueError("request backup directory must not already exist")
    durable_mkdir(backup_dir, 0o700)
    try:
        config_backup = backup_dir / "config.yaml"
        env_backup = backup_dir / "erhua.env"
        originals = {"config": file_record(config), "env": file_record(env)}
        if originals["config"]["sha256"] != args.expected_config_sha:
            raise ValueError("config changed after the reviewed dry run")
        if originals["env"]["sha256"] != args.expected_env_sha:
            raise ValueError("env changed after the reviewed dry run")
        atomic_copy(config, config_backup, 0o600)
        atomic_copy(env, env_backup, 0o600)
        if digest(config_backup) != originals["config"]["sha256"]:
            raise ValueError("config changed while its backup was created")
        if digest(env_backup) != originals["env"]["sha256"]:
            raise ValueError("env changed while its backup was created")
        record = {
            "schema_version": 1,
            "agent_id": "erhua",
            "files": {
                "config": {
                    "path": str(config),
                    "backup": "config.yaml",
                    **originals["config"],
                },
                "env": {"path": str(env), "backup": "erhua.env", **originals["env"]},
            },
        }
        atomic_json_write(metadata, record)
        verify_originals(config, env, record)
    except BaseException:
        shutil.rmtree(backup_dir, ignore_errors=True)
        raise


def load_metadata(metadata: Path) -> dict:
    require_regular_input(metadata)
    data = json.loads(metadata.read_text(encoding="utf-8"))
    if data.get("schema_version") != 1 or data.get("agent_id") != "erhua":
        raise ValueError("profile backup metadata is invalid")
    return data


def verify_originals(config: Path, env: Path, data: dict) -> None:
    for key, path in (("config", config), ("env", env)):
        if str(path) != data["files"][key]["path"]:
            raise ValueError(f"{key} path does not match backup metadata")
        if file_record(path) != {
            "sha256": data["files"][key]["sha256"],
            "mode": data["files"][key]["mode"],
            "uid": data["files"][key]["uid"],
            "gid": data["files"][key]["gid"],
        }:
            raise ValueError(f"{key} changed after preflight")


def restore_files(config: Path, env: Path, backup_dir: Path, data: dict) -> None:
    for key, path in (("config", config), ("env", env)):
        if str(path) != data["files"][key]["path"]:
            raise ValueError(f"{key} path does not match backup metadata")
    for _, path in (("config", config), ("env", env)):
        if path.is_symlink() or path.parent.is_symlink():
            raise ValueError(f"symlinked restore target is not allowed: {path}")
    for key, target in (("config", config), ("env", env)):
        source = backup_dir / data["files"][key]["backup"]
        require_regular_input(source)
        if digest(source) != data["files"][key]["sha256"]:
            raise ValueError(f"{key} backup hash mismatch")
        atomic_copy(
            source,
            target,
            int(data["files"][key]["mode"], 8),
            (data["files"][key]["uid"], data["files"][key]["gid"]),
        )
    verify_originals(config, env, data)


def activate(args: argparse.Namespace) -> None:
    config, env, backup_dir, metadata = paths(args)
    candidate_config = Path(args.candidate_config)
    candidate_env = Path(args.candidate_env)
    data = load_metadata(metadata)
    verify_originals(config, env, data)
    require_regular_input(candidate_config)
    require_regular_input(candidate_env)
    try:
        atomic_copy(
            candidate_env,
            env,
            0o600,
            (data["files"]["env"]["uid"], data["files"]["env"]["gid"]),
        )
        atomic_copy(
            candidate_config,
            config,
            0o600,
            (data["files"]["config"]["uid"], data["files"]["config"]["gid"]),
        )
        data["activated"] = {"config": file_record(config), "env": file_record(env)}
        atomic_json_write(metadata, data)
    except BaseException:
        restore_files(config, env, backup_dir, data)
        raise


def restore(args: argparse.Namespace) -> None:
    config, env, backup_dir, metadata = paths(args)
    data = load_metadata(metadata)
    restore_files(config, env, backup_dir, data)


def verify_activated(args: argparse.Namespace) -> None:
    config, env, _, metadata = paths(args)
    data = load_metadata(metadata)
    activated = data.get("activated")
    if not isinstance(activated, dict):
        raise ValueError("profile activation record is missing")
    for key, path in (("config", config), ("env", env)):
        if file_record(path) != activated.get(key):
            raise ValueError(f"activated {key} no longer matches transaction metadata")


def add_common(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--config", required=True)
    parser.add_argument("--env", required=True)
    parser.add_argument("--backup-dir", required=True)
    parser.add_argument("--metadata", required=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    backup_parser = commands.add_parser("backup")
    add_common(backup_parser)
    backup_parser.add_argument("--expected-config-sha", required=True)
    backup_parser.add_argument("--expected-env-sha", required=True)
    backup_parser.set_defaults(func=backup)
    activate_parser = commands.add_parser("activate")
    add_common(activate_parser)
    activate_parser.add_argument("--candidate-config", required=True)
    activate_parser.add_argument("--candidate-env", required=True)
    activate_parser.set_defaults(func=activate)
    restore_parser = commands.add_parser("restore")
    add_common(restore_parser)
    restore_parser.set_defaults(func=restore)
    verify_parser = commands.add_parser("verify-activated")
    add_common(verify_parser)
    verify_parser.set_defaults(func=verify_activated)
    args = parser.parse_args()
    try:
        args.func(args)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"profile transaction failed: {exc}", file=os.sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
