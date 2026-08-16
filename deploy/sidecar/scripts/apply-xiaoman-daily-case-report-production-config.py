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
RELEASE_CURRENT_PATH = RELEASE_ROOT_PATH / "current"
LOCK_PATH = Path("/run/qintopia-xiaoman-daily-case-report-config.lock")
APPLY_APPROVAL = "approved-production-xiaoman-daily-case-report-config-v1"
PUBLISH_APPROVAL = "approved-production-xiaoman-daily-case-report-auto-publish"
QIWE_SEND_APPROVAL = "approved-production-qiwe-image-send"
FEISHU_MIRROR_APPROVAL = "approved-huabaosi-feishu-artifact-mirror"
FEISHU_SCHEMA_VERSION = "huabaosi-generated-image-v1"
HUABAOSI_PROFILE_ENV_PATH = "/home/ubuntu/.hermes/profiles/huabaosi/.env"
HTTP_STORAGE_BACKEND = "https-public"
FEISHU_STORAGE_BACKEND = "feishu-base"
MANAGED_COMMENT = "# Managed by apply-xiaoman-daily-case-report-production-config.py"
MAX_INPUT_BYTES = 64 * 1024
MAX_ENV_BYTES = 1024 * 1024
MAX_MESSAGE_TEXT_CHARS = 200

SHA_RE = re.compile(r"[0-9a-f]{40}")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
ASSIGNMENT_RE = re.compile(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$")
CONTROL_RE = re.compile(r"[\x00-\x1f\x7f]")
HOST_RE = re.compile(r"^[a-z0-9.-]+$")

ACTIVE_KEYS = {
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MESSAGE_TEXT",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_WORKFLOW_PY",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_ALLOWED_CALLER",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_PYTHON_BIN",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_RENDER_TIMEOUT_SECONDS",
}
REQUIRED_SHARED_BOUNDARY_KEYS = {
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "QINTOPIA_XIAOMAN_ACTIVITY_TARGET_GROUP_ID",
}
REQUIRED_HTTP_BOUNDARY_KEYS = {
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
}
REQUIRED_FEISHU_BOUNDARY_KEYS = {
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
}
TRACKED_KEYS = (
    ACTIVE_KEYS
    | REQUIRED_SHARED_BOUNDARY_KEYS
    | REQUIRED_HTTP_BOUNDARY_KEYS
    | REQUIRED_FEISHU_BOUNDARY_KEYS
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


def require_string(value: dict[str, Any], key: str) -> str:
    item = value.get(key)
    if not isinstance(item, str):
        raise ConfigError(f"{key} is required")
    item = item.strip()
    if not item or CONTROL_RE.search(item) or "$(" in item or "`" in item:
        raise ConfigError(f"{key} value is invalid")
    return item


def normalized_host(value: str, label: str) -> str:
    host = value.strip().lower().rstrip(".")
    if not host or not HOST_RE.fullmatch(host) or ".." in host:
        raise ConfigError(f"{label} host is invalid")
    return host


def strict_https_url(value: str, label: str) -> tuple[str, str]:
    parsed = urlparse(value)
    if parsed.scheme != "https" or not parsed.hostname:
        raise ConfigError(f"{label} must be an HTTPS URL")
    if parsed.username or parsed.password or parsed.fragment:
        raise ConfigError(f"{label} must not contain credentials or fragments")
    host = normalized_host(parsed.hostname, label)
    if host in {"localhost", "127.0.0.1", "::1"} or host.endswith(".local"):
        raise ConfigError(f"{label} must use a public host")
    return value, host


def parse_csv_set(value: str, label: str) -> set[str]:
    items = {normalized_host(item, label) for item in value.split(",") if item.strip()}
    if not items:
        raise ConfigError(f"{label} must not be empty")
    return items


def parse_exact_csv_set(value: str, label: str) -> set[str]:
    items = {item.strip() for item in value.split(",") if item.strip()}
    if not items or any(CONTROL_RE.search(item) or "$(" in item or "`" in item for item in items):
        raise ConfigError(f"{label} must not be empty")
    return items


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
        raise ConfigError("sidecar env contains invalid tracked shell quoting") from exc
    if len(parts) != 1:
        raise ConfigError("sidecar env contains unsafe tracked values")
    return parts[0]


def validate_request(value: dict[str, Any]) -> dict[str, str]:
    allowed = {
        "schema_version",
        "desired_state",
        "release_sha",
        "database_url_sha256",
        "storage_backend",
        "chat_id",
        "target_group_id",
        "media_upload_endpoint",
        "media_public_base_url",
        "media_allowed_hosts",
        "message_text",
        "mcp_workflow_py",
        "mcp_allowed_caller",
        "mcp_python_bin",
        "mcp_render_timeout_seconds",
    }
    if set(value) - allowed:
        raise ConfigError("configuration input contains unsupported fields")
    if value.get("schema_version") != 1:
        raise ConfigError("configuration schema_version must be 1")
    desired_state = value.get("desired_state")
    if desired_state not in {"enabled", "disabled"}:
        raise ConfigError("desired_state must be enabled or disabled")
    release_sha = require_string(value, "release_sha")
    if not SHA_RE.fullmatch(release_sha):
        raise ConfigError("release_sha must be a lowercase 40-character Git SHA")
    normalized: dict[str, str] = {
        "desired_state": desired_state,
        "release_sha": release_sha,
    }
    if desired_state == "disabled":
        extra = set(value) - {"schema_version", "desired_state", "release_sha"}
        if extra:
            raise ConfigError("disabled state accepts only schema_version, desired_state, and release_sha")
        return normalized

    database_hash = require_string(value, "database_url_sha256")
    if not SHA256_RE.fullmatch(database_hash):
        raise ConfigError("database_url_sha256 must be a lowercase SHA-256")
    storage_backend = value.get("storage_backend", HTTP_STORAGE_BACKEND)
    if not isinstance(storage_backend, str):
        raise ConfigError("storage_backend must be a string")
    storage_backend = storage_backend.strip()
    if storage_backend not in {HTTP_STORAGE_BACKEND, FEISHU_STORAGE_BACKEND}:
        raise ConfigError("storage_backend must be https-public or feishu-base")
    chat_id = require_string(value, "chat_id")
    target_group_id = require_string(value, "target_group_id")
    message_text = value.get("message_text")
    if message_text is not None:
        if not isinstance(message_text, str):
            raise ConfigError("message_text must be a string")
        message_text = message_text.strip()
        if (
            not message_text
            or len(message_text) > MAX_MESSAGE_TEXT_CHARS
            or CONTROL_RE.search(message_text)
            or "$(" in message_text
            or "`" in message_text
        ):
            raise ConfigError("message_text value is invalid")

    normalized.update(
        {
            "database_url_sha256": database_hash,
            "storage_backend": storage_backend,
            "chat_id": chat_id,
            "target_group_id": target_group_id,
        }
    )
    if storage_backend == HTTP_STORAGE_BACKEND:
        upload_endpoint, upload_host = strict_https_url(
            require_string(value, "media_upload_endpoint"),
            "media upload endpoint",
        )
        public_base, public_host = strict_https_url(
            require_string(value, "media_public_base_url"),
            "media public base URL",
        )
        daily_hosts = parse_csv_set(
            require_string(value, "media_allowed_hosts"), "daily media allowed hosts"
        )
        if upload_host not in daily_hosts:
            raise ConfigError("media upload endpoint host must be in the daily media allowlist")
        if public_host not in daily_hosts:
            raise ConfigError("media public base host must be in the daily media allowlist")
        normalized.update(
            {
                "media_upload_endpoint": upload_endpoint,
                "media_upload_host": upload_host,
                "media_public_base_url": public_base,
                "media_public_host": public_host,
                "media_allowed_hosts": ",".join(sorted(daily_hosts)),
            }
        )
    else:
        forbidden = {
            "media_upload_endpoint",
            "media_public_base_url",
            "media_allowed_hosts",
        } & set(value)
        if forbidden:
            raise ConfigError("feishu-base storage must not carry HTTPS media fields")
    if message_text is not None:
        normalized["message_text"] = message_text

    mcp_workflow_py = value.get("mcp_workflow_py")
    if mcp_workflow_py is not None:
        if not isinstance(mcp_workflow_py, str):
            raise ConfigError("mcp_workflow_py must be a string")
        mcp_workflow_py = mcp_workflow_py.strip()
        if (
            not mcp_workflow_py
            or mcp_workflow_py.startswith("/")
            or mcp_workflow_py.startswith("~")
            or ".." in mcp_workflow_py
            or CONTROL_RE.search(mcp_workflow_py)
            or "$(" in mcp_workflow_py
            or "`" in mcp_workflow_py
            or not mcp_workflow_py.endswith("daily_case_report.py")
        ):
            raise ConfigError(
                "mcp_workflow_py must be a relative release workflow path ending in daily_case_report.py"
            )
        normalized["mcp_workflow_py"] = mcp_workflow_py

    mcp_allowed_caller = value.get("mcp_allowed_caller")
    if mcp_allowed_caller is not None:
        if not isinstance(mcp_allowed_caller, str):
            raise ConfigError("mcp_allowed_caller must be a string")
        mcp_allowed_caller = mcp_allowed_caller.strip()
        if (
            not mcp_allowed_caller
            or CONTROL_RE.search(mcp_allowed_caller)
            or "$(" in mcp_allowed_caller
            or "`" in mcp_allowed_caller
        ):
            raise ConfigError("mcp_allowed_caller value is invalid")
        normalized["mcp_allowed_caller"] = mcp_allowed_caller

    mcp_python_bin = value.get("mcp_python_bin")
    if mcp_python_bin is not None:
        if not isinstance(mcp_python_bin, str):
            raise ConfigError("mcp_python_bin must be a string")
        mcp_python_bin = mcp_python_bin.strip()
        if (
            not mcp_python_bin
            or CONTROL_RE.search(mcp_python_bin)
            or "$(" in mcp_python_bin
            or "`" in mcp_python_bin
        ):
            raise ConfigError("mcp_python_bin value is invalid")
        normalized["mcp_python_bin"] = mcp_python_bin

    mcp_timeout = value.get("mcp_render_timeout_seconds")
    if mcp_timeout is not None:
        if not isinstance(mcp_timeout, int) or mcp_timeout < 1 or mcp_timeout > 3600:
            raise ConfigError("mcp_render_timeout_seconds must be an integer between 1 and 3600")
        normalized["mcp_render_timeout_seconds"] = str(mcp_timeout)

    return normalized


def parse_env_text(text: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw in text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = ASSIGNMENT_RE.match(raw)
        if not match:
            continue
        key, value = match.groups()
        if key not in TRACKED_KEYS:
            continue
        if key in values:
            raise ConfigError("sidecar env contains duplicate tracked keys")
        value = parse_shell_env_value(value)
        if CONTROL_RE.search(value) or "$(" in value or "`" in value:
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
        values=parse_env_text(text),
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
    if not resolved_target.is_dir() or resolved_target.name != expected_sha:
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


def require_enabled_boundaries(values: dict[str, str], request: dict[str, str]) -> None:
    missing = sorted(
        key for key in REQUIRED_SHARED_BOUNDARY_KEYS if not values.get(key, "").strip()
    )
    if missing:
        raise ConfigError("sidecar env is missing required daily report boundaries")
    validate_database_url(values["QINTOPIA_SIDECAR_DATABASE_URL"], request["database_url_sha256"])
    if values["QINTOPIA_QIWE_IMAGE_SEND_ENABLED"] != "1":
        raise ConfigError("QiWe image-send production timer is not enabled")
    if values["QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY"] != "1":
        raise ConfigError("QiWe image-send webhook readiness is not approved")
    if values["QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL"] != QIWE_SEND_APPROVAL:
        raise ConfigError("QiWe image-send production approval is not present")
    if values["QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256"] != request["database_url_sha256"]:
        raise ConfigError("QiWe image-send database hash does not match daily report approval")
    if request["storage_backend"] == HTTP_STORAGE_BACKEND:
        missing = sorted(
            key for key in REQUIRED_HTTP_BOUNDARY_KEYS if not values.get(key, "").strip()
        )
        if missing:
            raise ConfigError("sidecar env is missing required daily HTTPS media boundaries")
        existing_media_hosts = parse_csv_set(
            values["QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS"],
            "existing generated-image media allowed hosts",
        )
        daily_media_hosts = parse_csv_set(
            request["media_allowed_hosts"],
            "daily media allowed hosts",
        )
        if not daily_media_hosts.issubset(existing_media_hosts):
            raise ConfigError("daily media hosts are outside the reviewed QiWe media boundary")
    else:
        require_feishu_storage_boundaries(values, request["database_url_sha256"])
    allowed_groups = parse_exact_csv_set(
        values["QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS"],
        "operations allowed group ids",
    )
    if request["target_group_id"] not in allowed_groups:
        raise ConfigError("daily report target group is not allowlisted for QiWe sends")
    if request["target_group_id"] != values["QINTOPIA_XIAOMAN_ACTIVITY_TARGET_GROUP_ID"]:
        raise ConfigError("daily report target group does not match the reviewed Xiaoman target")


def require_singleton_allowlist(value: str, expected: str, label: str) -> None:
    items = [item.strip() for item in value.split(",") if item.strip()]
    if items != [expected]:
        raise ConfigError(f"{label} allowlist is not exact")


def require_feishu_storage_boundaries(values: dict[str, str], database_hash: str) -> None:
    missing = sorted(
        key for key in REQUIRED_FEISHU_BOUNDARY_KEYS if not values.get(key, "").strip()
    )
    if missing:
        raise ConfigError("sidecar env is missing required Feishu storage boundaries")
    if values["QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED"] != "1":
        raise ConfigError("Feishu primary-storage delivery is not enabled")
    if values["QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL"] != FEISHU_MIRROR_APPROVAL:
        raise ConfigError("Feishu primary-storage approval is not approved")
    if values["QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256"] != database_hash:
        raise ConfigError("Feishu primary-storage database hash is not approved")
    if values["QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH"] != HUABAOSI_PROFILE_ENV_PATH:
        raise ConfigError("Feishu primary-storage profile path is not approved")
    if values["QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION"] != FEISHU_SCHEMA_VERSION:
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


def desired_values(request: dict[str, str]) -> dict[str, str]:
    if request["desired_state"] == "disabled":
        return {"QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED": "0"}
    values = {
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED": "1",
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL": PUBLISH_APPROVAL,
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE": "1",
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND": request["storage_backend"],
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID": request["chat_id"],
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID": request["target_group_id"],
    }
    if request["storage_backend"] == HTTP_STORAGE_BACKEND:
        values.update(
            {
                "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_UPLOAD_ENDPOINT": request[
                    "media_upload_endpoint"
                ],
                "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL": request[
                    "media_public_base_url"
                ],
                "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_ALLOWED_HOSTS": request[
                    "media_allowed_hosts"
                ],
            }
        )
    if "message_text" in request:
        values["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MESSAGE_TEXT"] = request["message_text"]
    if "mcp_workflow_py" in request:
        values["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_WORKFLOW_PY"] = request[
            "mcp_workflow_py"
        ]
    if "mcp_allowed_caller" in request:
        values["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_ALLOWED_CALLER"] = request[
            "mcp_allowed_caller"
        ]
    if "mcp_python_bin" in request:
        values["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_PYTHON_BIN"] = request[
            "mcp_python_bin"
        ]
    if "mcp_render_timeout_seconds" in request:
        values["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MCP_RENDER_TIMEOUT_SECONDS"] = request[
            "mcp_render_timeout_seconds"
        ]
    return values


def render_env_text(text: str, replacements: dict[str, str]) -> str:
    retained = []
    for raw in text.splitlines():
        match = ASSIGNMENT_RE.match(raw)
        if match and match.group(1) in ACTIVE_KEYS:
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
        raise ConfigError("Xiaoman daily report production configuration requires root")
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
            require_enabled_boundaries(sidecar.values, normalized)
        replacements = desired_values(normalized)
        if normalized["desired_state"] == "disabled":
            for key in sorted(ACTIVE_KEYS - {"QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED"}):
                value = sidecar.values.get(key, "").strip()
                if value:
                    replacements[key] = value
        next_text = render_env_text(sidecar.text, replacements)
        change_required = next_text.encode("utf-8") != sidecar.text.encode("utf-8")
        if apply and change_required:
            commit_env(sidecar, next_text)
        return {
            "success": True,
            "apply_requested": apply,
            "action_status": "xiaoman_daily_case_report_config_applied"
            if apply
            else "xiaoman_daily_case_report_config_ready",
            "desired_state": normalized["desired_state"],
            "storage_backend": normalized.get("storage_backend"),
            "auto_publish_enabled": normalized["desired_state"] == "enabled",
            "release_sha_matched": True,
            "database_url_sha256_matched": normalized["desired_state"] == "enabled",
            "target_group_allowlisted": normalized["desired_state"] == "enabled",
            "media_boundary_bound": normalized["desired_state"] == "enabled",
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
        description="Preview or apply Xiaoman daily case-report production configuration"
    )
    parser.add_argument("--stdin", action="store_true")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval", default="")
    return parser.parse_args()


def emit(report: dict[str, Any]) -> None:
    print(
        "xiaoman_daily_case_report_production_config="
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
            raise ConfigError("Xiaoman daily report production configuration requires root")
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
