#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fcntl
import grp
import json
import os
import pwd
import re
import shlex
import stat
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")
PROFILE_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/.env")
RELEASE_ROOT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases")
RELEASE_CURRENT_PATH = RELEASE_ROOT_PATH / "current"
LOCK_PATH = Path("/run/qintopia-xiaoman-activity-read-through-config.lock")
APPLY_APPROVAL = "approved-production-xiaoman-activity-read-through-config-v1"
MANAGED_COMMENT = "# Managed by apply-xiaoman-activity-read-through-production-config.py"
MAX_ENV_BYTES = 1024 * 1024
SHA_RE = re.compile(r"[0-9a-f]{40}")
ASSIGNMENT_RE = re.compile(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$")
CONTROL_RE = re.compile(r"[\x00-\x1f\x7f]")

READ_THROUGH_KEYS = (
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN",
    "QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS",
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID",
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID",
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PROFILE_ENV_PATH",
)


class ConfigError(ValueError):
    pass


@dataclass(frozen=True)
class EnvDocument:
    path: Path
    text: str
    values: dict[str, str]
    mode: int
    uid: int
    gid: int


def parse_shell_env_value(value: str) -> str:
    value = value.strip()
    if not value:
        return ""
    lexer = shlex.shlex(value, posix=True)
    lexer.whitespace_split = True
    lexer.commenters = ""
    try:
        parts = list(lexer)
    except ValueError as exc:
        raise ConfigError("env file contains invalid tracked shell quoting") from exc
    if len(parts) != 1:
        raise ConfigError("env file contains unsafe tracked values")
    return parts[0]


def parse_env_text(text: str, tracked_keys: set[str]) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw in text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = ASSIGNMENT_RE.match(raw)
        if not match:
            continue
        key, value = match.groups()
        if key not in tracked_keys:
            continue
        if key in values:
            raise ConfigError(f"env file contains duplicate {key}")
        parsed = parse_shell_env_value(value)
        if CONTROL_RE.search(parsed) or "$(" in parsed or "`" in parsed:
            raise ConfigError(f"env file contains unsafe {key}")
        values[key] = parsed
    return values


def read_env_document(
    path: Path,
    *,
    tracked_keys: set[str],
    expected_uid: int | None = None,
    expected_gid: int | None = None,
    expected_mode: int | None = None,
) -> EnvDocument:
    try:
        metadata = path.lstat()
    except FileNotFoundError as exc:
        raise ConfigError(f"env file is missing: {path}") from exc
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise ConfigError(f"env file must be a regular non-symlink file: {path}")
    if metadata.st_nlink != 1:
        raise ConfigError(f"env file hard links are forbidden: {path}")
    if metadata.st_size <= 0 or metadata.st_size > MAX_ENV_BYTES:
        raise ConfigError(f"env file size is invalid: {path}")
    mode = stat.S_IMODE(metadata.st_mode)
    if mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise ConfigError(f"env file must not be group/world writable: {path}")
    if expected_uid is not None and metadata.st_uid != expected_uid:
        raise ConfigError(f"env file owner is not approved: {path}")
    if expected_gid is not None and metadata.st_gid != expected_gid:
        raise ConfigError(f"env file group is not approved: {path}")
    if expected_mode is not None and mode != expected_mode:
        raise ConfigError(f"env file mode is not approved: {path}")
    text = path.read_text(encoding="utf-8")
    return EnvDocument(
        path=path,
        text=text,
        values=parse_env_text(text, tracked_keys),
        mode=mode,
        uid=metadata.st_uid,
        gid=metadata.st_gid,
    )


def validate_release_current(path: Path, release_root: Path, expected_sha: str) -> None:
    if not SHA_RE.fullmatch(expected_sha):
        raise ConfigError("release_sha must be a lowercase 40-character Git SHA")
    if not path.is_symlink():
        raise ConfigError("release/current must be a symlink")
    try:
        resolved_root = release_root.resolve(strict=True)
        resolved_target = path.resolve(strict=True)
    except FileNotFoundError as exc:
        raise ConfigError("release/current target is missing") from exc
    if resolved_target.parent != resolved_root:
        raise ConfigError("release/current must resolve to the fixed release root")
    if not resolved_target.is_dir() or resolved_target.name != expected_sha:
        raise ConfigError("release/current does not match the approved release SHA")


def validate_profile_values(values: dict[str, str], profile_env_path: Path) -> dict[str, str]:
    missing = [key for key in READ_THROUGH_KEYS if not values.get(key)]
    if missing:
        raise ConfigError("Xiaoman profile env is missing required read-through keys")
    if values["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PROFILE_ENV_PATH"] != str(profile_env_path):
        raise ConfigError("Xiaoman profile env path must be the fixed production path")
    token = values["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN"]
    allowed_tokens = {
        item.strip()
        for item in values["QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS"].split(",")
        if item.strip()
    }
    if not allowed_tokens or token not in allowed_tokens:
        raise ConfigError("Xiaoman activity Feishu Base token must be explicitly allowlisted")
    return {key: values[key] for key in READ_THROUGH_KEYS}


def render_env_text(text: str, replacements: dict[str, str]) -> str:
    retained: list[str] = []
    for raw in text.splitlines():
        match = ASSIGNMENT_RE.match(raw)
        if match and match.group(1) in replacements:
            continue
        if raw.strip() == MANAGED_COMMENT:
            continue
        retained.append(raw)
    while retained and not retained[-1].strip():
        retained.pop()
    managed = [MANAGED_COMMENT] + [
        f"{key}={shlex.quote(replacements[key])}" for key in READ_THROUGH_KEYS
    ]
    return "\n".join(retained + [""] + managed) + "\n"


def commit_env(document: EnvDocument, text: str) -> None:
    stage_path: Path | None = None
    replaced = False
    try:
        with tempfile.NamedTemporaryFile(
            "w",
            encoding="utf-8",
            dir=str(document.path.parent),
            prefix=f".{document.path.name}.",
            suffix=".qintopia-stage",
            delete=False,
        ) as handle:
            stage_path = Path(handle.name)
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(stage_path, document.mode)
        if os.geteuid() == 0:
            os.chown(stage_path, document.uid, document.gid)
        os.replace(stage_path, document.path)
        replaced = True
        directory = os.open(document.path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if not replaced and stage_path is not None:
            try:
                stage_path.unlink()
            except FileNotFoundError:
                pass


def configure(
    *,
    release_sha: str,
    apply: bool,
    approval: str,
    effective_uid: int,
    sidecar_env_path: Path = SIDECAR_ENV_PATH,
    profile_env_path: Path = PROFILE_ENV_PATH,
    release_current_path: Path = RELEASE_CURRENT_PATH,
    release_root_path: Path = RELEASE_ROOT_PATH,
    lock_path: Path = LOCK_PATH,
    expected_sidecar_uid: int | None = None,
    expected_sidecar_gid: int | None = None,
    expected_profile_uid: int | None = None,
    expected_profile_gid: int | None = None,
) -> dict[str, object]:
    if apply and approval != APPLY_APPROVAL:
        raise ConfigError("exact owner approval is required for configuration apply")
    if apply and effective_uid != 0:
        raise ConfigError("Xiaoman activity read-through configuration requires root")
    validate_release_current(release_current_path, release_root_path, release_sha)

    lock_path.parent.mkdir(parents=True, exist_ok=True)
    lock_descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        fcntl.flock(lock_descriptor, fcntl.LOCK_EX)
        sidecar = read_env_document(
            sidecar_env_path,
            tracked_keys=set(READ_THROUGH_KEYS),
            expected_uid=expected_sidecar_uid,
            expected_gid=expected_sidecar_gid,
            expected_mode=0o640,
        )
        profile = read_env_document(
            profile_env_path,
            tracked_keys=set(READ_THROUGH_KEYS),
            expected_uid=expected_profile_uid,
            expected_gid=expected_profile_gid,
            expected_mode=0o600,
        )
        replacements = validate_profile_values(profile.values, profile_env_path)
        next_text = render_env_text(sidecar.text, replacements)
        change_required = next_text.encode("utf-8") != sidecar.text.encode("utf-8")
        if apply and change_required:
            commit_env(sidecar, next_text)
        return {
            "success": True,
            "apply_requested": apply,
            "action_status": "xiaoman_activity_read_through_config_applied"
            if apply
            else "xiaoman_activity_read_through_config_ready",
            "release_sha_matched": True,
            "copied_key_count": len(READ_THROUGH_KEYS),
            "sidecar_change_required": change_required,
            "deduped": not change_required,
            "sensitive_values_redacted": True,
            "external_calls_executed": False,
            "service_changes_executed": False,
        }
    finally:
        os.close(lock_descriptor)


def ubuntu_ids() -> tuple[int, int]:
    try:
        ubuntu_user = pwd.getpwnam("ubuntu")
        ubuntu = grp.getgrnam("ubuntu")
    except KeyError as exc:
        raise ConfigError("ubuntu user and group are required") from exc
    return ubuntu_user.pw_uid, ubuntu.gr_gid


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Apply Xiaoman activity read-through production config"
    )
    parser.add_argument("--release-sha", required=True)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval", default="")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        ubuntu_uid, ubuntu_gid = ubuntu_ids()
        report = configure(
            release_sha=args.release_sha,
            apply=args.apply,
            approval=args.approval,
            effective_uid=os.geteuid(),
            expected_sidecar_uid=0,
            expected_sidecar_gid=ubuntu_gid,
            expected_profile_uid=ubuntu_uid,
            expected_profile_gid=ubuntu_gid,
        )
    except ConfigError as exc:
        print(f"xiaoman activity read-through production config failed: {exc}", file=sys.stderr)
        return 1
    print(
        "xiaoman_activity_read_through_production_config="
        + json.dumps(report, sort_keys=True, separators=(",", ":"))
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
