#!/usr/bin/env python3
"""Migrate the one Silaoshi script_action route to the durable bridge."""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import os
import pwd
import secrets
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any


ROUTE_NAME = "silaoshi-base-notify"
ACTION_SCRIPT_NAME = "silaoshi_resident_webhook_action.py"
HANDLER_SCRIPT_NAME = "silaoshi_resident_card_from_message.py"


class MigrationError(RuntimeError):
    pass


def _load_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise MigrationError(f"{path.name} must contain a JSON object")
    return value


def _literal_assignments(path: Path) -> dict[str, Any]:
    tree = ast.parse(path.read_text(encoding="utf-8"))
    values: dict[str, Any] = {}
    for node in tree.body:
        if not isinstance(node, (ast.Assign, ast.AnnAssign)):
            continue
        targets = node.targets if isinstance(node, ast.Assign) else [node.target]
        for target in targets:
            if isinstance(target, ast.Name):
                try:
                    values[target.id] = ast.literal_eval(node.value)
                except (ValueError, TypeError):
                    if (
                        isinstance(node.value, ast.Call)
                        and isinstance(node.value.func, ast.Name)
                        and node.value.func.id == "Path"
                        and len(node.value.args) == 1
                        and isinstance(node.value.args[0], ast.Constant)
                        and isinstance(node.value.args[0].value, str)
                    ):
                        values[target.id] = node.value.args[0].value
    return values


def build_migration(
    subscriptions: dict[str, Any], transform_path: Path, python_path: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    route = subscriptions.get(ROUTE_NAME)
    if not isinstance(route, dict):
        raise MigrationError(f"required route {ROUTE_NAME} is missing")
    action = route.get("script_action")
    if not isinstance(action, dict):
        raise MigrationError("route does not contain script_action")
    script = Path(str(action.get("script", ""))).resolve()
    if script.name != ACTION_SCRIPT_NAME or not script.is_file():
        raise MigrationError("script_action does not reference the reviewed action script")
    action_constants = _literal_assignments(script)
    handler = Path(action_constants.get("HANDLER", script.with_name(HANDLER_SCRIPT_NAME))).resolve()
    if handler.name != HANDLER_SCRIPT_NAME or not handler.is_file():
        raise MigrationError("reviewed resident handler is unavailable")
    handler_constants = _literal_assignments(handler)
    send_text = Path(str(handler_constants.get("SEND_TEXT_SCRIPT", ""))).resolve()
    group_id = handler_constants.get("COMPANY_GROUP_ID")
    if not send_text.is_file() or not isinstance(group_id, str) or not group_id:
        raise MigrationError("server-local notification binding is incomplete")

    migrated = json.loads(json.dumps(subscriptions))
    migrated_route = migrated[ROUTE_NAME]
    migrated_route.pop("script_action")
    migrated_route["script"] = str(transform_path)

    timeout = action.get("timeout", 900)
    config = {
        "socket_path": "/run/qintopia-agentos/silaoshi-script-action.sock",
        "database_path": "/var/lib/qintopia-agentos/silaoshi-script-action/jobs.sqlite3",
        "secret_file": "/etc/qintopia/silaoshi-script-action.key",
        "action": {
            "name": ROUTE_NAME,
            "command": [python_path, str(script)],
            "script_sha256": hashlib.sha256(script.read_bytes()).hexdigest(),
            "timeout_seconds": timeout,
            "max_retries": 2,
            "notification_command": [
                python_path,
                str(send_text),
                "--chat-id",
                group_id,
                "--bypass-filter",
            ],
            "success_template": str(action.get("success_template", "")),
            "failure_template": str(action.get("failure_template", "")),
        },
    }
    return migrated, config


def _atomic_write(
    path: Path, data: bytes, mode: int, owner: tuple[int, int] | None = None
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.write(fd, data)
        os.fchmod(fd, mode)
        if owner is not None:
            os.fchown(fd, owner[0], owner[1])
        os.fsync(fd)
        os.close(fd)
        fd = -1
        os.replace(temporary, path)
    finally:
        if fd >= 0:
            os.close(fd)
        Path(temporary).unlink(missing_ok=True)


def fingerprint(value: Any) -> str:
    return hashlib.sha256(
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--subscriptions", required=True, type=Path)
    parser.add_argument("--transform-source", required=True, type=Path)
    parser.add_argument("--transform-target", required=True, type=Path)
    parser.add_argument("--config-target", required=True, type=Path)
    parser.add_argument("--secret-target", required=True, type=Path)
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument("--runtime-user", default=pwd.getpwuid(os.getuid()).pw_name)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--rollback", action="store_true")
    args = parser.parse_args()
    runtime_account = pwd.getpwnam(args.runtime_user)
    runtime_owner = (runtime_account.pw_uid, runtime_account.pw_gid)

    if args.apply and args.rollback:
        raise MigrationError("choose only one of --apply or --rollback")
    backup = args.subscriptions.with_suffix(args.subscriptions.suffix + ".pre-bridge")
    if args.rollback:
        if not backup.is_file():
            raise MigrationError("migration backup is unavailable")
        restored = backup.read_bytes()
        current = args.subscriptions.stat()
        _atomic_write(
            args.subscriptions,
            restored,
            current.st_mode & 0o777,
            (current.st_uid, current.st_gid),
        )
        print(
            json.dumps(
                {
                    "status": "rolled_back",
                    "route": ROUTE_NAME,
                    "restored_fingerprint": hashlib.sha256(restored).hexdigest(),
                },
                sort_keys=True,
            )
        )
        return 0

    subscriptions = _load_object(args.subscriptions)
    migrated, config = build_migration(subscriptions, args.transform_target, args.python)
    report = {
        "status": "ready" if not args.apply else "applied",
        "route": ROUTE_NAME,
        "before_fingerprint": fingerprint(subscriptions),
        "after_fingerprint": fingerprint(migrated),
        "action_script_fingerprint": config["action"]["script_sha256"],
        "config_field_count": len(config["action"]),
        "secret_present": args.secret_target.is_file(),
    }
    if not args.apply:
        print(json.dumps(report, sort_keys=True))
        return 0

    if not args.transform_source.is_file():
        raise MigrationError("transform source is unavailable")
    if backup.exists():
        raise MigrationError("migration backup already exists")
    subscription_metadata = args.subscriptions.stat()
    shutil.copy2(args.subscriptions, backup)
    os.chmod(backup, 0o600)
    if not args.secret_target.exists():
        _atomic_write(
            args.secret_target,
            (secrets.token_hex(32) + "\n").encode(),
            0o640,
            (0, runtime_account.pw_gid) if os.geteuid() == 0 else runtime_owner,
        )
    config_owner = (0, runtime_account.pw_gid) if os.geteuid() == 0 else runtime_owner
    _atomic_write(
        args.config_target,
        (json.dumps(config, ensure_ascii=False, indent=2) + "\n").encode(),
        0o640,
        config_owner,
    )
    _atomic_write(
        args.transform_target,
        args.transform_source.read_bytes(),
        0o750,
        config_owner,
    )
    _atomic_write(
        args.subscriptions,
        (json.dumps(migrated, ensure_ascii=False, indent=2) + "\n").encode(),
        subscription_metadata.st_mode & 0o777,
        (subscription_metadata.st_uid, subscription_metadata.st_gid),
    )
    report["secret_present"] = True
    report["backup_fingerprint"] = hashlib.sha256(backup.read_bytes()).hexdigest()
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (MigrationError, OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"silaoshi script action migration: {exc}", file=sys.stderr)
        raise SystemExit(1)
