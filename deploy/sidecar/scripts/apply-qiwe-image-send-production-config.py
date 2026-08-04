#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fcntl
import grp
import hashlib
import json
import os
import re
import shlex
import stat
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, BinaryIO
from urllib.parse import urlparse


SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")
RELEASE_ROOT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases")
RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")
LOCK_PATH = Path("/run/qintopia-qiwe-image-send-config.lock")
APPLY_APPROVAL = "approved-production-qiwe-image-send-config-v1"
SEND_APPROVAL = "approved-production-qiwe-image-send"
FEISHU_MIRROR_APPROVAL = "approved-huabaosi-feishu-artifact-mirror"
FEISHU_SCHEMA_VERSION = "huabaosi-generated-image-v1"
HUABAOSI_PROFILE_ENV_PATH = "/home/ubuntu/.hermes/profiles/huabaosi/.env"
MAX_INPUT_BYTES = 64 * 1024
MAX_ENV_BYTES = 1024 * 1024
MANAGED_COMMENT = "# Managed by apply-qiwe-image-send-production-config.py"

SHA_RE = re.compile(r"[0-9a-f]{40}")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
ASSIGNMENT_RE = re.compile(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$")
CONTROL_RE = re.compile(r"[\x00-\x1f\x7f]")

ACTIVE_KEYS = {
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
}
REQUIRED_ENABLE_KEYS = {
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA",
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
}
TRACKED_KEYS = ACTIVE_KEYS | REQUIRED_ENABLE_KEYS


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


def _reject_duplicate_json_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ConfigError("configuration input contains a duplicate key")
        result[key] = value
    return result


def load_request(stream: BinaryIO) -> dict[str, Any]:
    data = stream.read(MAX_INPUT_BYTES + 1)
    if not data or len(data) > MAX_INPUT_BYTES:
        raise ConfigError("configuration input length is invalid")
    try:
        value = json.loads(data, object_pairs_hook=_reject_duplicate_json_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ConfigError("configuration input is not valid JSON") from exc
    if not isinstance(value, dict):
        raise ConfigError("configuration input must be one JSON object")
    return value


def validate_request(value: dict[str, Any]) -> dict[str, Any]:
    allowed = {"schema_version", "desired_state", "release_sha", "database_url_sha256"}
    if set(value) - allowed:
        raise ConfigError("configuration input contains unsupported fields")
    if value.get("schema_version") != 1:
        raise ConfigError("configuration schema_version must be 1")
    desired_state = value.get("desired_state")
    if desired_state not in {"enabled", "disabled"}:
        raise ConfigError("desired_state must be enabled or disabled")
    release_sha = value.get("release_sha")
    if not isinstance(release_sha, str) or not SHA_RE.fullmatch(release_sha):
        raise ConfigError("release_sha must be a lowercase 40-character Git SHA")
    normalized: dict[str, Any] = {
        "schema_version": 1,
        "desired_state": desired_state,
        "release_sha": release_sha,
    }
    if desired_state == "enabled":
        database_hash = value.get("database_url_sha256")
        if not isinstance(database_hash, str) or not SHA256_RE.fullmatch(database_hash):
            raise ConfigError("database_url_sha256 must be a lowercase SHA-256")
        normalized["database_url_sha256"] = database_hash
    elif "database_url_sha256" in value:
        raise ConfigError("disabled state does not accept database_url_sha256")
    return normalized


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
            raise ConfigError("sidecar env contains duplicate tracked keys")
        value = value.strip()
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        if "$" + "(" in value or "`" in value or CONTROL_RE.search(value):
            raise ConfigError("sidecar env contains unsafe tracked values")
        values[key] = value
    return values


def read_env_document(
    path: Path,
    *,
    expected_uid: int | None,
    expected_gid: int | None,
    expected_mode: int | None,
) -> EnvDocument:
    try:
        metadata = path.lstat()
    except FileNotFoundError as exc:
        raise ConfigError("sidecar env file is missing") from exc
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise ConfigError("sidecar env file must be a regular non-symlink file")
    if metadata.st_nlink != 1:
        raise ConfigError("sidecar env file must not have hard links")
    if metadata.st_size <= 0 or metadata.st_size > MAX_ENV_BYTES:
        raise ConfigError("sidecar env file size is invalid")
    mode = stat.S_IMODE(metadata.st_mode)
    if mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise ConfigError("sidecar env file must not be group/world writable")
    if expected_uid is not None and metadata.st_uid != expected_uid:
        raise ConfigError("sidecar env file owner is not approved")
    if expected_gid is not None and metadata.st_gid != expected_gid:
        raise ConfigError("sidecar env file group is not approved")
    if expected_mode is not None and mode != expected_mode:
        raise ConfigError("sidecar env file mode is not approved")
    text = path.read_text(encoding="utf-8")
    return EnvDocument(
        path=path,
        text=text,
        values=parse_env_text(text, TRACKED_KEYS),
        mode=mode,
        uid=metadata.st_uid,
        gid=metadata.st_gid,
    )


def validate_release_current(path: Path, release_root: Path, expected_sha: str) -> None:
    if not path.is_symlink():
        raise ConfigError("release/current must be a symlink")
    try:
        resolved_root = release_root.resolve(strict=True)
        resolved_target = path.resolve(strict=True)
    except FileNotFoundError as exc:
        raise ConfigError("release/current target is missing") from exc
    if resolved_target.parent != resolved_root:
        raise ConfigError("release/current must resolve to the fixed release root")
    if not resolved_target.is_dir():
        raise ConfigError("release/current target must be a release directory")
    if resolved_target.name != expected_sha:
        raise ConfigError("release/current does not match the approved release SHA")


def validate_database_url(value: str, expected_hash: str) -> None:
    if not value or value != value.strip() or CONTROL_RE.search(value) or "'" in value:
        raise ConfigError("database URL shape is invalid")
    parsed = urlparse(value)
    if (
        parsed.scheme not in {"postgres", "postgresql"}
        or not parsed.hostname
        or not parsed.username
        or parsed.password is None
        or not parsed.path.strip("/")
    ):
        raise ConfigError("database URL shape is invalid")
    if hashlib.sha256(value.encode("utf-8")).hexdigest() != expected_hash:
        raise ConfigError("database URL hash does not match the approved production hash")


def require_singleton_allowlist(value: str, expected: str, label: str) -> None:
    items = [item.strip() for item in value.split(",") if item.strip()]
    if items != [expected]:
        if label == "Feishu Base token":
            raise ConfigError("Feishu Base token allowlist is not exact")
        if label == "Feishu artifact table":
            raise ConfigError("Feishu artifact table allowlist is not exact")
        raise ConfigError(f"{label} allowlist is not exact")


def require_enabled_boundaries(
    values: dict[str, str], database_hash: str, release_sha: str
) -> None:
    missing = sorted(key for key in REQUIRED_ENABLE_KEYS if not values.get(key, "").strip())
    if missing:
        raise ConfigError("sidecar env is missing required QiWe send boundaries")
    if values.get("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL") != SEND_APPROVAL:
        raise ConfigError("QiWe send production owner approval is not present")
    if values.get("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256") != database_hash:
        raise ConfigError("QiWe send production database hash is not approved")
    if values.get("QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY") != "1":
        raise ConfigError("QiWe send webhook readiness is not approved")
    if values.get("QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED") != "1":
        raise ConfigError("Feishu primary-storage delivery is not enabled")
    if values.get("QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL") != FEISHU_MIRROR_APPROVAL:
        raise ConfigError("Feishu primary-storage approval is not approved")
    if values.get("QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA") != release_sha:
        raise ConfigError("Feishu primary-storage release SHA is not approved")
    if values.get("QINTOPIA_DEPLOYED_COMMIT_SHA") != release_sha:
        raise ConfigError("deployed release SHA is not approved")
    if values.get("QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256") != database_hash:
        raise ConfigError("Feishu primary-storage database hash is not approved")
    if values.get("QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH") != HUABAOSI_PROFILE_ENV_PATH:
        raise ConfigError("Feishu primary-storage profile path is not approved")
    if values.get("QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION") != FEISHU_SCHEMA_VERSION:
        raise ConfigError("Feishu primary-storage schema is not approved")
    require_singleton_allowlist(
        values["QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS"],
        values["QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN"],
        "Feishu Base token",
    )
    require_singleton_allowlist(
        values["QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS"],
        values["QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID"],
        "Feishu artifact table",
    )
    validate_database_url(values["QINTOPIA_SIDECAR_DATABASE_URL"], database_hash)


def desired_values(request: dict[str, Any]) -> dict[str, str]:
    if request["desired_state"] == "disabled":
        return {"QINTOPIA_QIWE_IMAGE_SEND_ENABLED": "0"}
    return {
        "QINTOPIA_QIWE_IMAGE_SEND_ENABLED": "1",
        "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL": SEND_APPROVAL,
        "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256": request[
            "database_url_sha256"
        ],
        "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY": "1",
    }


def render_env_text(text: str, replacements: dict[str, str]) -> str:
    retained = []
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
        f"{key}={shlex.quote(value)}" for key, value in sorted(replacements.items())
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
    request: dict[str, Any],
    sidecar_path: Path,
    release_current_path: Path,
    lock_path: Path,
    release_root_path: Path | None = None,
    apply: bool,
    approval: str,
    effective_uid: int,
    expected_uid: int | None = None,
    expected_gid: int | None = None,
    expected_mode: int | None = None,
) -> dict[str, Any]:
    normalized = validate_request(request)
    if apply and approval != APPLY_APPROVAL:
        raise ConfigError("exact owner approval is required for configuration apply")
    if apply and effective_uid != 0:
        raise ConfigError("QiWe send production configuration requires root")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    lock_descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        fcntl.flock(lock_descriptor, fcntl.LOCK_EX)
        validate_release_current(
            release_current_path,
            release_root_path or release_current_path.parent,
            normalized["release_sha"],
        )
        sidecar = read_env_document(
            sidecar_path,
            expected_uid=expected_uid,
            expected_gid=expected_gid,
            expected_mode=expected_mode,
        )
        if normalized["desired_state"] == "enabled":
            require_enabled_boundaries(
                sidecar.values,
                normalized["database_url_sha256"],
                normalized["release_sha"],
            )
        replacements = desired_values(normalized)
        next_text = render_env_text(sidecar.text, replacements)
        change_required = next_text.encode("utf-8") != sidecar.text.encode("utf-8")
        if apply and change_required:
            commit_env(sidecar, next_text)
        return {
            "success": True,
            "apply_requested": apply,
            "action_status": "qiwe_image_send_config_applied"
            if apply
            else "qiwe_image_send_config_ready",
            "desired_state": normalized["desired_state"],
            "send_enabled": normalized["desired_state"] == "enabled",
            "release_sha_matched": True,
            "database_url_sha256_matched": normalized["desired_state"] == "enabled",
            "required_boundary_count": len(REQUIRED_ENABLE_KEYS)
            if normalized["desired_state"] == "enabled"
            else 0,
            "sidecar_change_required": change_required,
            "deduped": not change_required,
            "sensitive_values_redacted": True,
            "external_calls_executed": False,
            "database_writes_executed": False,
            "service_changes_executed": False,
        }
    finally:
        os.close(lock_descriptor)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Preview or apply the fixed QiWe image-send production enablement"
    )
    parser.add_argument("--stdin", action="store_true")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval", default="")
    return parser.parse_args()


def emit(report: dict[str, Any]) -> None:
    print(
        "qiwe_image_send_production_config="
        + json.dumps(report, sort_keys=True, separators=(",", ":"))
    )


def ubuntu_group_id() -> int:
    try:
        return grp.getgrnam("ubuntu").gr_gid
    except KeyError as exc:
        raise ConfigError("ubuntu group is required for sidecar env ownership") from exc


def main() -> int:
    args = parse_args()
    try:
        if os.geteuid() != 0:
            raise ConfigError("QiWe send production configuration requires root")
        if not args.stdin:
            raise ConfigError("--stdin is required")
        request = load_request(sys.stdin.buffer)
        report = configure(
            request=request,
            sidecar_path=SIDECAR_ENV_PATH,
            release_current_path=RELEASE_CURRENT_PATH,
            release_root_path=RELEASE_ROOT_PATH,
            lock_path=LOCK_PATH,
            apply=args.apply,
            approval=args.approval,
            effective_uid=os.geteuid(),
            expected_uid=0,
            expected_gid=ubuntu_group_id(),
            expected_mode=0o640,
        )
        emit(report)
        return 0
    except ConfigError as exc:
        emit(
            {
                "success": False,
                "action_status": "validation_failed",
                "error": str(exc),
                "sensitive_values_redacted": True,
                "external_calls_executed": False,
                "database_writes_executed": False,
                "service_changes_executed": False,
            }
        )
        return 1
    except Exception:
        emit(
            {
                "success": False,
                "action_status": "unexpected_failure",
                "error": "unexpected configuration failure",
                "sensitive_values_redacted": True,
                "external_calls_executed": False,
                "database_writes_executed": False,
                "service_changes_executed": False,
            }
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
