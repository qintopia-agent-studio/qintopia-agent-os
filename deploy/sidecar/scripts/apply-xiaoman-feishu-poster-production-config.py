#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import re
import secrets
import shlex
import stat
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO, Any
from urllib.parse import urlparse


SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")
HERMES_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/.env")
RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")
LOCK_PATH = Path("/run/qintopia-xiaoman-feishu-config.lock")
APPLY_APPROVAL = "approved-production-xiaoman-feishu-config-v1"
MAX_INPUT_BYTES = 64 * 1024
MAX_ENV_BYTES = 1024 * 1024
MANAGED_COMMENT = "# Managed by apply-xiaoman-feishu-poster-production-config.py"

IDENTIFIER_RE = re.compile(r"[A-Za-z0-9_.:-]{1,240}")
SAFE_SECRET_RE = re.compile(r"[A-Za-z0-9._~+/=-]{32,512}")
SHA_RE = re.compile(r"[0-9a-f]{40}")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
ASSIGNMENT_RE = re.compile(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$")
CONTROL_RE = re.compile(r"[\x00-\x1f\x7f]")

SIDECAR_ACTIVE_KEYS = [
    "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED",
    "QINTOPIA_XIAOMAN_FEISHU_POSTER_APPROVAL",
    "QINTOPIA_XIAOMAN_FEISHU_POSTER_RELEASE_SHA",
    "QINTOPIA_XIAOMAN_FEISHU_POSTER_DATABASE_URL_SHA256",
    "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
    "QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS",
    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS",
    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS",
    "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS",
    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED",
]
HERMES_ACTIVE_KEYS = [
    "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
    "QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID",
    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED",
]
PRODUCTION_DATABASE_HASH_KEYS = [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256",
    "QINTOPIA_XIAOMAN_FEISHU_POSTER_DATABASE_URL_SHA256",
]
SIDECAR_REQUIRED_EXISTING = [
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_XIAOMAN_FEISHU_APP_ID",
    "QINTOPIA_XIAOMAN_FEISHU_APP_SECRET",
    "QINTOPIA_XIAOMAN_POSTER_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY",
    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS",
    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS",
    "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS",
]
HERMES_REQUIRED_EXISTING = ["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"]
ALL_TRACKED_KEYS = set(
    SIDECAR_ACTIVE_KEYS
    + HERMES_ACTIVE_KEYS
    + PRODUCTION_DATABASE_HASH_KEYS
    + SIDECAR_REQUIRED_EXISTING
    + HERMES_REQUIRED_EXISTING
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


@dataclass(frozen=True)
class ConfigPlan:
    sidecar_text: str
    hermes_text: str
    report: dict[str, Any]


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
    allowed = {
        "schema_version",
        "desired_state",
        "release_sha",
        "database_url_sha256",
        "database_url",
        "rotate_ingress_hmac",
        "bot_open_id",
        "allowed_chat_ids",
        "allowed_user_ids",
        "reviewer_user_ids",
    }
    if set(value) - allowed:
        raise ConfigError("configuration input contains unsupported fields")
    if value.get("schema_version") != 1:
        raise ConfigError("configuration schema_version must be 1")
    desired_state = value.get("desired_state")
    if desired_state not in {"direct", "group", "disabled"}:
        raise ConfigError("desired_state must be direct, group, or disabled")
    release_sha = value.get("release_sha")
    if not isinstance(release_sha, str) or not SHA_RE.fullmatch(release_sha):
        raise ConfigError("release_sha must be a lowercase 40-character Git SHA")
    rotate_hmac = value.get("rotate_ingress_hmac", False)
    if not isinstance(rotate_hmac, bool):
        raise ConfigError("rotate_ingress_hmac must be a boolean")

    normalized: dict[str, Any] = {
        "schema_version": 1,
        "desired_state": desired_state,
        "release_sha": release_sha,
        "rotate_ingress_hmac": rotate_hmac,
    }
    if desired_state == "disabled":
        unexpected = set(value) - {
            "schema_version",
            "desired_state",
            "release_sha",
            "rotate_ingress_hmac",
        }
        if unexpected or rotate_hmac:
            raise ConfigError("disabled state accepts only the release identity")
        return normalized

    database_hash = value.get("database_url_sha256")
    if not isinstance(database_hash, str) or not SHA256_RE.fullmatch(database_hash):
        raise ConfigError("database_url_sha256 must be a lowercase SHA-256")
    normalized["database_url_sha256"] = database_hash

    if "database_url" in value:
        normalized["database_url"] = validate_database_url(
            value["database_url"], database_hash
        )

    bot_open_id = value.get("bot_open_id")
    if bot_open_id is not None:
        normalized["bot_open_id"] = validate_identifier("bot_open_id", bot_open_id)

    list_fields = ["allowed_chat_ids", "allowed_user_ids", "reviewer_user_ids"]
    supplied_lists = {name for name in list_fields if name in value}
    for name in supplied_lists:
        normalized[name] = validate_identifier_list(name, value[name])

    if desired_state == "group":
        required = {"allowed_chat_ids", "allowed_user_ids", "reviewer_user_ids"}
        if not isinstance(bot_open_id, str) or supplied_lists != required:
            raise ConfigError(
                "group state requires explicit Bot, chat, user, and reviewer ceilings"
            )
    elif supplied_lists and not {"allowed_chat_ids", "allowed_user_ids"}.issubset(
        supplied_lists
    ):
        raise ConfigError("direct chat and user ceilings must be supplied together")

    if "reviewer_user_ids" in normalized and "allowed_user_ids" in normalized:
        if not set(normalized["allowed_user_ids"]).issubset(
            normalized["reviewer_user_ids"]
        ):
            raise ConfigError("allowed users must be within the reviewer ceiling")
    return normalized


def validate_database_url(value: Any, expected_hash: str) -> str:
    if not isinstance(value, str) or not value or value != value.strip():
        raise ConfigError("database_url must be a non-empty string without padding")
    if CONTROL_RE.search(value) or "'" in value:
        raise ConfigError("database_url contains unsupported characters")
    parsed = urlparse(value)
    if (
        parsed.scheme not in {"postgres", "postgresql"}
        or not parsed.hostname
        or not parsed.username
        or parsed.password is None
        or not parsed.path.strip("/")
    ):
        raise ConfigError("database_url must identify one credentialed Postgres database")
    if sha256_hex(value.encode("utf-8")) != expected_hash:
        raise ConfigError("database_url does not match the approved SHA-256")
    return value


def validate_identifier(label: str, value: Any) -> str:
    if not isinstance(value, str) or not IDENTIFIER_RE.fullmatch(value):
        raise ConfigError(f"{label} contains an invalid identifier")
    return value


def validate_identifier_list(label: str, value: Any) -> list[str]:
    if not isinstance(value, list) or not 1 <= len(value) <= 256:
        raise ConfigError(f"{label} must contain between 1 and 256 identifiers")
    result = sorted({validate_identifier(label, item) for item in value})
    if len(result) != len(value):
        raise ConfigError(f"{label} contains duplicate identifiers")
    return result


def parse_identifier_csv(label: str, value: str) -> list[str]:
    items = [item.strip() for item in value.split(",") if item.strip()]
    return validate_identifier_list(label, items)


def parse_env_text(text: str, tracked_keys: set[str]) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw in text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = ASSIGNMENT_RE.fullmatch(raw)
        if not match:
            if any(re.match(rf"^(?:export[ \t]+)?{re.escape(name)}\b", stripped) for name in tracked_keys):
                raise ConfigError("managed environment assignment syntax is invalid")
            continue
        name, raw_value = match.groups()
        if name not in tracked_keys:
            continue
        if name in values:
            raise ConfigError("managed environment key is assigned more than once")
        try:
            parts = shlex.split(raw_value, comments=True, posix=True)
        except ValueError as exc:
            raise ConfigError("managed environment value syntax is invalid") from exc
        if len(parts) != 1:
            raise ConfigError("managed environment value must be one literal")
        values[name] = parts[0]
    return values


def read_env(path: Path) -> EnvDocument:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise ConfigError("required environment file is unavailable") from exc
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise ConfigError("required environment path is not a regular file")
        if metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
            raise ConfigError("required environment file is group- or world-writable")
        chunks: list[bytes] = []
        total = 0
        while True:
            chunk = os.read(descriptor, min(16384, MAX_ENV_BYTES + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > MAX_ENV_BYTES:
                raise ConfigError("required environment file exceeds the size limit")
    finally:
        os.close(descriptor)
    try:
        text = b"".join(chunks).decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ConfigError("required environment file is not UTF-8") from exc
    return EnvDocument(
        path=path,
        text=text,
        values=parse_env_text(text, ALL_TRACKED_KEYS),
        mode=stat.S_IMODE(metadata.st_mode),
        uid=metadata.st_uid,
        gid=metadata.st_gid,
    )


def require_value(values: dict[str, str], name: str) -> str:
    value = values.get(name, "").strip()
    if not value:
        raise ConfigError(f"required production setting is missing: {name}")
    return value


def require_safe_secret(label: str, value: str) -> str:
    if not SAFE_SECRET_RE.fullmatch(value):
        raise ConfigError(f"{label} must be a safe 32-to-512-character token")
    return value


def resolve_release_sha(path: Path) -> str:
    try:
        if not path.is_symlink():
            raise ConfigError("release/current must be a symbolic link")
        release_root = path.parent.resolve(strict=True)
        resolved = path.resolve(strict=True)
        metadata = resolved.stat()
    except OSError as exc:
        raise ConfigError("release/current is unavailable") from exc
    if resolved.parent != release_root:
        raise ConfigError("release/current must resolve within the fixed release root")
    if not resolved.is_dir() or metadata.st_mode & (
        stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH
    ):
        raise ConfigError("release/current target is not an immutable directory")
    if not SHA_RE.fullmatch(resolved.name):
        raise ConfigError("release/current target has an invalid identity")
    return resolved.name


def reject_symlinked_parents(path: Path) -> None:
    if not path.is_absolute():
        raise ConfigError("protected runtime path must be absolute")
    cursor = Path(path.anchor)
    for part in path.parts[1:-1]:
        cursor /= part
        try:
            metadata = os.lstat(cursor)
        except OSError as exc:
            raise ConfigError("protected runtime parent is unavailable") from exc
        if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
            raise ConfigError("protected runtime parent must be a regular directory")


def render_env(text: str, updates: dict[str, str]) -> str:
    kept: list[str] = []
    for raw in text.splitlines():
        if raw == MANAGED_COMMENT:
            continue
        match = ASSIGNMENT_RE.fullmatch(raw)
        if match and match.group(1) in updates:
            continue
        kept.append(raw)
    while kept and not kept[-1].strip():
        kept.pop()
    if kept:
        kept.append("")
    kept.append(MANAGED_COMMENT)
    for name, value in updates.items():
        if CONTROL_RE.search(value) or "'" in value:
            raise ConfigError("managed environment value cannot be rendered safely")
        kept.append(f"{name}='{value}'")
    return "\n".join(kept) + "\n"


def build_plan(
    request: dict[str, Any],
    sidecar: EnvDocument,
    hermes: EnvDocument,
    release_sha: str,
) -> ConfigPlan:
    if request["release_sha"] != release_sha:
        raise ConfigError("requested Release does not match release/current")
    desired_state = request["desired_state"]
    if desired_state == "disabled":
        sidecar_updates = {
            "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED": "0",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "0",
            "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
        }
        hermes_updates = {
            "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE": "0",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "0",
            "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": "0",
        }
        sidecar_text = render_env(sidecar.text, sidecar_updates)
        hermes_text = render_env(hermes.text, hermes_updates)
        return ConfigPlan(
            sidecar_text,
            hermes_text,
            base_report(
                desired_state,
                release_sha,
                sidecar_text != sidecar.text,
                hermes_text != hermes.text,
                0,
                0,
                0,
                "unchanged",
                False,
                0,
            ),
        )

    for name in SIDECAR_REQUIRED_EXISTING:
        require_value(sidecar.values, name)
    for name in HERMES_REQUIRED_EXISTING:
        require_value(hermes.values, name)
    callback_key = require_value(
        sidecar.values, "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"
    )
    if callback_key != require_value(
        hermes.values, "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"
    ):
        raise ConfigError("poster callback key binding is invalid")

    current_database_url = require_value(sidecar.values, "QINTOPIA_SIDECAR_DATABASE_URL")
    database_url = request.get("database_url", current_database_url)
    database_hash = sha256_hex(database_url.encode("utf-8"))
    if database_hash != request["database_url_sha256"]:
        raise ConfigError("configured database URL does not match the approved SHA-256")

    if "allowed_chat_ids" in request:
        chats = request["allowed_chat_ids"]
        users = request["allowed_user_ids"]
    else:
        chats = parse_identifier_csv(
            "allowed_chat_ids",
            require_value(sidecar.values, "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS"),
        )
        users = parse_identifier_csv(
            "allowed_user_ids",
            require_value(sidecar.values, "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS"),
        )
    reviewers = request.get("reviewer_user_ids")
    if reviewers is None:
        reviewers = parse_identifier_csv(
            "reviewer_user_ids",
            require_value(sidecar.values, "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS"),
        )
    if not set(users).issubset(reviewers):
        raise ConfigError("allowed users must be within the reviewer ceiling")

    ingress_sidecar = sidecar.values.get("QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY")
    ingress_hermes = hermes.values.get("QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY")
    rotate_hmac = request["rotate_ingress_hmac"]
    if rotate_hmac:
        ingress_key = secrets.token_urlsafe(48)
        ingress_action = "rotated"
    elif ingress_sidecar is None and ingress_hermes is None:
        ingress_key = secrets.token_urlsafe(48)
        ingress_action = "generated"
    elif ingress_sidecar is None or ingress_hermes is None or ingress_sidecar != ingress_hermes:
        raise ConfigError("ingress HMAC binding is incomplete; explicit rotation is required")
    else:
        ingress_key = require_safe_secret("ingress HMAC", ingress_sidecar)
        ingress_action = "preserved"
    require_safe_secret("ingress HMAC", ingress_key)
    if ingress_key == callback_key:
        raise ConfigError("ingress and callback keys must be distinct")

    bot_open_id = request.get("bot_open_id")
    existing_bot_sidecar = sidecar.values.get("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID")
    existing_bot_hermes = hermes.values.get("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID")
    if bot_open_id is None and (existing_bot_sidecar or existing_bot_hermes):
        if (
            existing_bot_sidecar != existing_bot_hermes
            or not existing_bot_sidecar
            or not IDENTIFIER_RE.fullmatch(existing_bot_sidecar)
        ):
            raise ConfigError("Bot identity binding is invalid")
        bot_open_id = existing_bot_sidecar
    if desired_state == "group" and bot_open_id is None:
        raise ConfigError("group state requires an exact Bot identity")

    hash_keys = {
        name
        for name in PRODUCTION_DATABASE_HASH_KEYS
        if name in sidecar.values
        or name
        in {
            "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256",
            "QINTOPIA_XIAOMAN_FEISHU_POSTER_DATABASE_URL_SHA256",
        }
    }
    sidecar_updates: dict[str, str] = {
        "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED": "1",
        "QINTOPIA_XIAOMAN_FEISHU_POSTER_APPROVAL":
            "approved-production-xiaoman-feishu-poster-return",
        "QINTOPIA_XIAOMAN_FEISHU_POSTER_RELEASE_SHA": release_sha,
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "1",
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY": ingress_key,
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS": ",".join(chats),
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS": ",".join(users),
        "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS": ",".join(chats),
        "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS": ",".join(users),
        "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS": ",".join(reviewers),
        "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": (
            "1" if desired_state == "group" else "0"
        ),
    }
    if database_url != current_database_url:
        sidecar_updates["QINTOPIA_SIDECAR_DATABASE_URL"] = database_url
    for name in sorted(hash_keys):
        sidecar_updates[name] = database_hash
    if bot_open_id is not None:
        sidecar_updates["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"] = bot_open_id

    hermes_updates: dict[str, str] = {
        "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE": "1",
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE": "1",
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY": ingress_key,
        "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED": (
            "1" if desired_state == "group" else "0"
        ),
    }
    if bot_open_id is not None:
        hermes_updates["QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"] = bot_open_id

    sidecar_text = render_env(sidecar.text, sidecar_updates)
    hermes_text = render_env(hermes.text, hermes_updates)
    validate_rendered_binding(sidecar_text, hermes_text, desired_state)
    return ConfigPlan(
        sidecar_text,
        hermes_text,
        base_report(
            desired_state,
            release_sha,
            sidecar_text != sidecar.text,
            hermes_text != hermes.text,
            len(chats),
            len(users),
            len(reviewers),
            ingress_action,
            database_url != current_database_url,
            len(hash_keys),
        ),
    )


def validate_rendered_binding(sidecar_text: str, hermes_text: str, desired_state: str) -> None:
    sidecar = parse_env_text(sidecar_text, ALL_TRACKED_KEYS)
    hermes = parse_env_text(hermes_text, ALL_TRACKED_KEYS)
    group_state = "1" if desired_state == "group" else "0"
    required_pairs = [
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
        "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED",
    ]
    if any(sidecar.get(name) != hermes.get(name) for name in required_pairs):
        raise ConfigError("rendered sidecar and Hermes ingress binding does not match")
    if (
        sidecar.get("QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED") != "1"
        or hermes.get("QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE") != "1"
        or sidecar.get("QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE") != "1"
        or sidecar.get("QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED") != group_state
    ):
        raise ConfigError("rendered Xiaoman poster enablement is invalid")
    if desired_state == "group" and (
        not sidecar.get("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID")
        or sidecar.get("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID")
        != hermes.get("QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID")
    ):
        raise ConfigError("rendered group Bot identity binding is invalid")


def base_report(
    desired_state: str,
    release_sha: str,
    sidecar_changed: bool,
    hermes_changed: bool,
    chat_count: int,
    user_count: int,
    reviewer_count: int,
    ingress_hmac_action: str,
    database_url_rotated: bool,
    database_hash_binding_count: int,
) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "worker": "xiaoman-feishu-production-config",
        "desired_state": desired_state,
        "release_sha": release_sha,
        "sidecar_change_required": sidecar_changed,
        "hermes_change_required": hermes_changed,
        "chat_allowlist_count": chat_count,
        "user_allowlist_count": user_count,
        "reviewer_allowlist_count": reviewer_count,
        "ingress_hmac_action": ingress_hmac_action,
        "database_url_rotated": database_url_rotated,
        "database_hash_binding_count": database_hash_binding_count,
        "database_url_sha256_matched": desired_state != "disabled",
        "external_calls_executed": False,
        "database_writes_executed": False,
        "service_changes_executed": False,
        "sensitive_values_redacted": True,
    }


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def stage_file(document: EnvDocument, text: str) -> Path:
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{document.path.name}.", dir=document.path.parent
    )
    temporary = Path(temporary_name)
    try:
        os.fchmod(descriptor, document.mode)
        temporary_metadata = os.fstat(descriptor)
        if (temporary_metadata.st_uid, temporary_metadata.st_gid) != (
            document.uid,
            document.gid,
        ):
            os.fchown(descriptor, document.uid, document.gid)
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            descriptor = -1
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        return temporary
    except BaseException:
        if descriptor >= 0:
            os.close(descriptor)
        temporary.unlink(missing_ok=True)
        raise


def fsync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def restore_document(document: EnvDocument) -> None:
    staged = stage_file(document, document.text)
    os.replace(staged, document.path)
    fsync_directory(document.path.parent)


def commit_pair(
    sidecar: EnvDocument,
    hermes: EnvDocument,
    sidecar_text: str,
    hermes_text: str,
) -> None:
    staged_sidecar = stage_file(sidecar, sidecar_text)
    try:
        staged_hermes = stage_file(hermes, hermes_text)
    except BaseException:
        staged_sidecar.unlink(missing_ok=True)
        raise
    replaced_sidecar = False
    replaced_hermes = False
    try:
        if sidecar_text != sidecar.text:
            os.replace(staged_sidecar, sidecar.path)
            replaced_sidecar = True
            fsync_directory(sidecar.path.parent)
        if hermes_text != hermes.text:
            os.replace(staged_hermes, hermes.path)
            replaced_hermes = True
            fsync_directory(hermes.path.parent)
    except BaseException:
        if replaced_sidecar:
            restore_document(sidecar)
        if replaced_hermes:
            restore_document(hermes)
        raise
    finally:
        staged_sidecar.unlink(missing_ok=True)
        staged_hermes.unlink(missing_ok=True)


def configure(
    *,
    request: dict[str, Any],
    sidecar_path: Path,
    hermes_path: Path,
    release_current_path: Path,
    lock_path: Path,
    apply: bool,
    approval: str,
    effective_uid: int,
) -> dict[str, Any]:
    if effective_uid != 0:
        raise ConfigError("Xiaoman Feishu production configuration requires root")
    if apply and approval != APPLY_APPROVAL:
        raise ConfigError("exact owner approval is required for configuration apply")
    normalized = validate_request(request)
    for path in (sidecar_path, hermes_path, lock_path):
        reject_symlinked_parents(path)
    reject_symlinked_parents(release_current_path)
    release_sha = resolve_release_sha(release_current_path)

    lock_descriptor = os.open(
        lock_path,
        os.O_RDWR | os.O_CREAT | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0),
        0o600,
    )
    try:
        os.fchmod(lock_descriptor, 0o600)
        fcntl.flock(lock_descriptor, fcntl.LOCK_EX)
        sidecar = read_env(sidecar_path)
        hermes = read_env(hermes_path)
        plan = build_plan(normalized, sidecar, hermes, release_sha)
        if apply:
            commit_pair(sidecar, hermes, plan.sidecar_text, plan.hermes_text)
        report = dict(plan.report)
        report["success"] = True
        report["apply_requested"] = apply
        report["action_status"] = (
            "production_config_applied" if apply else "production_config_ready"
        )
        report["deduped"] = not (
            report["sidecar_change_required"] or report["hermes_change_required"]
        )
        return report
    finally:
        os.close(lock_descriptor)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Preview or apply the fixed Xiaoman Feishu production configuration"
    )
    parser.add_argument("--stdin", action="store_true")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval", default="")
    return parser.parse_args()


def emit(report: dict[str, Any]) -> None:
    print(
        "xiaoman_feishu_production_config="
        + json.dumps(report, sort_keys=True, separators=(",", ":"))
    )


def main() -> int:
    args = parse_args()
    if not args.stdin:
        emit(
            {
                "success": False,
                "action_status": "validation_failed",
                "error": "--stdin is required",
                "sensitive_values_redacted": True,
            }
        )
        return 1
    try:
        if os.geteuid() != 0:
            raise ConfigError("Xiaoman Feishu production configuration requires root")
        if args.apply and args.approval != APPLY_APPROVAL:
            raise ConfigError("exact owner approval is required for configuration apply")
        request = load_request(sys.stdin.buffer)
        report = configure(
            request=request,
            sidecar_path=SIDECAR_ENV_PATH,
            hermes_path=HERMES_ENV_PATH,
            release_current_path=RELEASE_CURRENT_PATH,
            lock_path=LOCK_PATH,
            apply=args.apply,
            approval=args.approval,
            effective_uid=os.geteuid(),
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
