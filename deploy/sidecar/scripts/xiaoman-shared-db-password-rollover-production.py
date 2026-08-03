#!/usr/bin/python3
from __future__ import annotations

import argparse
import base64
import fcntl
import hashlib
import hmac
import json
import os
import re
import secrets
import shlex
import stat
import subprocess
import sys
import tempfile
import time
import uuid
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Iterator
from urllib.parse import parse_qsl, quote, unquote_to_bytes, urlsplit, urlunsplit


RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")
SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")
HERMES_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/.env")
ERHUA_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/erhua/.env")
STATE_ROOT_PATH = Path("/var/lib/qintopia-xiaoman-db-password-rollover")
DEPLOY_STATE_ROOT_PATH = Path("/var/lib/qintopia-agent-os-deploy")
SCRIPT_RELATIVE_PATH = Path(
    "deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py"
)
CONFIG_SCRIPT_RELATIVE_PATH = Path(
    "deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py"
)
POLICY_SCRIPT_RELATIVE_PATH = Path(
    "deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py"
)
APPLY_APPROVAL = "approved-production-xiaoman-shared-db-password-rollover-v1"
CONFIG_APPROVAL = "approved-production-xiaoman-feishu-config-v1"
POLICY_APPROVAL = "approved-production-xiaoman-conversation-policy-v3"
MAX_INPUT_BYTES = 64 * 1024
MAX_STATE_BYTES = 64 * 1024
MAX_ENV_BYTES = 1024 * 1024
SHA_RE = re.compile(r"[0-9a-f]{40}")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
OPAQUE_REF_RE = re.compile(r"sha256:[0-9a-f]{64}")
DEPLOY_REQUEST_ID_RE = re.compile(
    r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}"
)
STATE_TEMP_NAME_RE = re.compile(
    r"^\.([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})"
    r"\.(state|receipt)\.json\.([a-z0-9_]{8})\.tmp$"
)
ASSIGNMENT_RE = re.compile(
    r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$"
)
AUTH_REJECTED_RE = re.compile(
    r"\bpassword authentication failed for user\b", re.IGNORECASE
)
TLS_ERROR_MARKERS = (
    "ssl error",
    "tls error",
    "certificate verify failed",
    "root certificate file",
    "server does not support ssl",
    "ssl connection has been closed unexpectedly",
)
TRANSPORT_ERROR_MARKERS = (
    "could not connect to server",
    "connection refused",
    "connection timed out",
    "timeout expired",
    "could not translate host name",
    "network is unreachable",
    "server closed the connection unexpectedly",
    "connection to server was lost",
)
DATABASE_HASH_KEYS = (
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256",
    "QINTOPIA_XIAOMAN_FEISHU_POSTER_DATABASE_URL_SHA256",
)
CORE_SERVICES = (
    "qintopia-message-sidecar.service",
    "qintopia-message-embedding-worker.service",
    "qintopia-message-identity-worker.service",
    "qintopia-agentos-raw-archive-worker.service",
    "qintopia-agentos-event-signal-worker.service",
    "qintopia-agentos-graph-projection-worker.service",
    "qintopia-agentos-member-profile-worker.service",
    "qintopia-agentos-daily-digest-worker.service",
    "qintopia-agentos-daily-digest-publisher.service",
)
ALLOWED_COMMANDS = {
    "prepare",
    "verify-reload",
    "apply-private-policy",
    "forward-verify",
    "rollback",
    "rollback-verify",
    "status",
}
NONTERMINAL_PHASES = {
    "escrowed",
    "preview_validated",
    "alter_in_flight",
    "credential_rotated",
    "direct_config_applied",
    "reload_verified",
    "private_policy_applied",
    "rollback_config_applied",
}
EXPECTED_RELEASE_SCOPE = ["sidecar-runtime", "deploy-bundle", "hermes-plugins"]
EXPECTED_RESTART_TARGETS = ["hermes-erhua", "qintopia-system-services"]


class RolloverError(RuntimeError):
    pass


@dataclass(frozen=True)
class ApprovedRequest:
    operation_id: str
    release_sha: str
    dry_run_request_id: str
    rollover_script_sha256: str
    config_script_sha256: str
    policy_script_sha256: str
    old_database_url_sha256: str
    role_ref: str
    conversation_ref: str
    actor_ref: str

    def public_identity(self) -> dict[str, str]:
        return {
            "operation_id": self.operation_id,
            "release_sha": self.release_sha,
            "dry_run_request_id": self.dry_run_request_id,
            "rollover_script_sha256": self.rollover_script_sha256,
            "config_script_sha256": self.config_script_sha256,
            "policy_script_sha256": self.policy_script_sha256,
            "old_database_url_sha256": self.old_database_url_sha256,
            "role_ref": self.role_ref,
            "conversation_ref": self.conversation_ref,
            "actor_ref": self.actor_ref,
        }


@dataclass(frozen=True)
class RuntimePaths:
    release_current: Path
    sidecar_env: Path
    hermes_env: Path
    erhua_env: Path
    state_root: Path
    self_path: Path


@dataclass(frozen=True)
class InitialContext:
    old_url: str
    role_name: str
    chat_id: str
    user_id: str


@dataclass(frozen=True)
class CredentialProbe:
    status: str


@dataclass(frozen=True)
class CredentialEvidence:
    state: str
    new_first: str
    old: str
    new_second: str

    def report(self) -> dict[str, str]:
        return {
            "credential_state": self.state,
            "new_probe_first": self.new_first,
            "old_probe": self.old,
            "new_probe_second": self.new_second,
        }


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def boot_id() -> str:
    value = Path("/proc/sys/kernel/random/boot_id").read_text(encoding="ascii").strip()
    try:
        return str(uuid.UUID(value))
    except ValueError as exc:
        raise RolloverError("boot_identity_invalid") from exc


def boot_monotonic_us() -> int:
    return int(time.clock_gettime(time.CLOCK_BOOTTIME) * 1_000_000)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def opaque_ref(parts: list[str]) -> str:
    digest = hashlib.sha256()
    for part in parts:
        digest.update(part.encode("utf-8"))
        digest.update(b"\x00")
    return "sha256:" + digest.hexdigest()


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise RolloverError("approved_request_duplicate_key")
        value[key] = item
    return value


def load_approved_request(data: bytes) -> ApprovedRequest:
    if not data or len(data) > MAX_INPUT_BYTES:
        raise RolloverError("approved_request_length_invalid")
    try:
        value = json.loads(data, object_pairs_hook=reject_duplicate_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RolloverError("approved_request_json_invalid") from exc
    expected_keys = {
        "schema_version",
        "operation_id",
        "release_sha",
        "dry_run_request_id",
        "rollover_script_sha256",
        "config_script_sha256",
        "policy_script_sha256",
        "old_database_url_sha256",
        "role_ref",
        "conversation_ref",
        "actor_ref",
    }
    if not isinstance(value, dict) or set(value) != expected_keys:
        raise RolloverError("approved_request_schema_invalid")
    if value.get("schema_version") != 1:
        raise RolloverError("approved_request_schema_invalid")
    try:
        operation_id = str(uuid.UUID(value["operation_id"]))
    except (AttributeError, TypeError, ValueError) as exc:
        raise RolloverError("operation_id_invalid") from exc
    if operation_id != value["operation_id"]:
        raise RolloverError("operation_id_invalid")
    release_sha = value["release_sha"]
    if not isinstance(release_sha, str) or not SHA_RE.fullmatch(release_sha):
        raise RolloverError("release_sha_invalid")
    dry_run_request_id = value["dry_run_request_id"]
    if (
        not isinstance(dry_run_request_id, str)
        or not DEPLOY_REQUEST_ID_RE.fullmatch(dry_run_request_id)
    ):
        raise RolloverError("dry_run_request_id_invalid")
    digest_names = (
        "rollover_script_sha256",
        "config_script_sha256",
        "policy_script_sha256",
        "old_database_url_sha256",
    )
    if any(
        not isinstance(value[name], str) or not SHA256_RE.fullmatch(value[name])
        for name in digest_names
    ):
        raise RolloverError("approved_digest_invalid")
    ref_names = ("role_ref", "conversation_ref", "actor_ref")
    if any(
        not isinstance(value[name], str) or not OPAQUE_REF_RE.fullmatch(value[name])
        for name in ref_names
    ):
        raise RolloverError("approved_reference_invalid")
    return ApprovedRequest(
        operation_id=operation_id,
        release_sha=release_sha,
        dry_run_request_id=dry_run_request_id,
        rollover_script_sha256=value["rollover_script_sha256"],
        config_script_sha256=value["config_script_sha256"],
        policy_script_sha256=value["policy_script_sha256"],
        old_database_url_sha256=value["old_database_url_sha256"],
        role_ref=value["role_ref"],
        conversation_ref=value["conversation_ref"],
        actor_ref=value["actor_ref"],
    )


def decode_uri_component(value: str) -> str:
    try:
        return unquote_to_bytes(value).decode("utf-8")
    except UnicodeDecodeError as exc:
        raise RolloverError("database_url_encoding_invalid") from exc


def database_url_parts(database_url: str) -> tuple[Any, str, str, str]:
    if any(ord(character) < 32 or ord(character) == 127 for character in database_url):
        raise RolloverError("database_url_shape_invalid")
    parsed = urlsplit(database_url)
    if (
        parsed.scheme not in {"postgres", "postgresql"}
        or not parsed.netloc
        or not parsed.path.strip("/")
        or parsed.fragment
        or "@" not in parsed.netloc
    ):
        raise RolloverError("database_url_shape_invalid")
    userinfo, authority = parsed.netloc.rsplit("@", 1)
    if ":" not in userinfo or not authority:
        raise RolloverError("database_url_shape_invalid")
    raw_user, raw_password = userinfo.split(":", 1)
    username = decode_uri_component(raw_user)
    password = decode_uri_component(raw_password)
    if not username or not password or "\x00" in username or "\x00" in password:
        raise RolloverError("database_url_shape_invalid")
    try:
        query_items = parse_qsl(parsed.query, keep_blank_values=True, strict_parsing=True)
    except ValueError as exc:
        raise RolloverError("database_url_query_invalid") from exc
    if any(name.casefold() in {"user", "password"} for name, _ in query_items):
        raise RolloverError("database_url_query_credential_invalid")
    return parsed, username, password, authority


def rotated_database_url(old_url: str, password: str) -> str:
    if len(password) < 48 or "\x00" in password:
        raise RolloverError("generated_password_strength_invalid")
    parsed, username, old_password, authority = database_url_parts(old_url)
    if password == old_password:
        raise RolloverError("database_password_not_rotated")
    netloc = f"{quote(username, safe='')}:{quote(password, safe='')}@{authority}"
    rotated = urlunsplit(
        (parsed.scheme, netloc, parsed.path, parsed.query, parsed.fragment)
    )
    assert_password_only_rotation(old_url, rotated)
    return rotated


def assert_password_only_rotation(old_url: str, new_url: str) -> None:
    old, old_user, old_password, old_authority = database_url_parts(old_url)
    new, new_user, new_password, new_authority = database_url_parts(new_url)
    if (
        old.scheme != new.scheme
        or old.path != new.path
        or old.query != new.query
        or old.fragment != new.fragment
        or old_authority != new_authority
        or old_user != new_user
        or old_password == new_password
    ):
        raise RolloverError("database_url_rotation_scope_invalid")


def sql_identifier(value: str) -> str:
    if not value or len(value.encode("utf-8")) > 63 or "\x00" in value:
        raise RolloverError("database_role_identifier_invalid")
    return '"' + value.replace('"', '""') + '"'


def sql_literal(value: str) -> str:
    if "\x00" in value:
        raise RolloverError("database_literal_invalid")
    return "'" + value.replace("'", "''") + "'"


def scram_verifier(password: str) -> str:
    if len(password) < 48:
        raise RolloverError("generated_password_strength_invalid")
    salt = secrets.token_bytes(16)
    iterations = 4096
    salted = hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt, iterations)
    client_key = hmac.new(salted, b"Client Key", hashlib.sha256).digest()
    stored_key = hashlib.sha256(client_key).digest()
    server_key = hmac.new(salted, b"Server Key", hashlib.sha256).digest()
    return (
        f"SCRAM-SHA-256${iterations}:{base64.b64encode(salt).decode('ascii')}$"
        f"{base64.b64encode(stored_key).decode('ascii')}:"
        f"{base64.b64encode(server_key).decode('ascii')}"
    )


def password_rotation_sql(role_name: str, verifier: str) -> str:
    if not verifier.startswith("SCRAM-SHA-256$"):
        raise RolloverError("scram_verifier_invalid")
    return (
        "BEGIN;\n"
        "SET LOCAL synchronous_commit = on;\n"
        f"ALTER ROLE {sql_identifier(role_name)} PASSWORD {sql_literal(verifier)};\n"
        "COMMIT;\n"
    )


def classify_psql_failure(stderr: str) -> str:
    normalized = stderr.casefold()
    if AUTH_REJECTED_RE.search(stderr):
        return "authentication_rejected"
    if any(marker in normalized for marker in TLS_ERROR_MARKERS):
        return "tls_error"
    if any(marker in normalized for marker in TRANSPORT_ERROR_MARKERS):
        return "transport_error"
    return "server_error"


class PsqlClient:
    def __init__(self, executable: Path = Path("/usr/bin/psql")) -> None:
        self.executable = executable

    def run(self, database_url: str, sql: str, *, timeout: int = 30) -> subprocess.CompletedProcess[str]:
        database_url_parts(database_url)
        try:
            return subprocess.run(
                [
                    str(self.executable),
                    "-XAt",
                    "--no-psqlrc",
                    "--set=ON_ERROR_STOP=1",
                    "-f",
                    "-",
                ],
                input=sql,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                timeout=timeout,
                env={
                    "PATH": "/usr/bin:/bin",
                    "LC_ALL": "C",
                    "PGDATABASE": database_url,
                    "PGCONNECT_TIMEOUT": "5",
                },
            )
        except subprocess.TimeoutExpired as exc:
            raise RolloverError("postgres_transport_timeout") from exc
        except OSError as exc:
            raise RolloverError("postgres_transport_failed") from exc

    def probe(self, database_url: str, role_name: str) -> CredentialProbe:
        try:
            result = self.run(database_url, "SELECT current_user;", timeout=10)
        except RolloverError as exc:
            if str(exc) in {"postgres_transport_timeout", "postgres_transport_failed"}:
                return CredentialProbe("transport_error")
            raise
        if result.returncode == 0:
            if result.stdout.strip() == role_name:
                return CredentialProbe("authenticated")
            return CredentialProbe("identity_mismatch")
        return CredentialProbe(classify_psql_failure(result.stderr))

    def alter_password(self, old_url: str, role_name: str, verifier: str) -> None:
        result = self.run(
            old_url,
            password_rotation_sql(role_name, verifier),
            timeout=30,
        )
        if result.returncode != 0:
            raise RolloverError("database_password_alter_result_unknown")

    def query(self, database_url: str, sql: str, *, timeout: int = 30) -> str:
        result = self.run(database_url, sql, timeout=timeout)
        if result.returncode != 0:
            raise RolloverError("postgres_query_failed")
        return result.stdout.strip()


def credential_evidence(
    client: PsqlClient, old_url: str, new_url: str, role_name: str
) -> CredentialEvidence:
    assert_password_only_rotation(old_url, new_url)
    new_first = client.probe(new_url, role_name).status
    old = client.probe(old_url, role_name).status
    new_second = client.probe(new_url, role_name).status
    if (
        new_first == "authenticated"
        and old == "authentication_rejected"
        and new_second == "authenticated"
    ):
        state = "rotated"
    elif (
        new_first == "authentication_rejected"
        and old == "authenticated"
        and new_second == "authentication_rejected"
    ):
        state = "unrotated"
    else:
        state = "ambiguous"
    return CredentialEvidence(state, new_first, old, new_second)


def secure_lstat(path: Path, *, owner_uid: int, kind: str) -> os.stat_result:
    try:
        metadata = os.lstat(path)
    except OSError as exc:
        raise RolloverError(f"{kind}_unavailable") from exc
    expected_type = stat.S_ISDIR if kind.endswith("directory") else stat.S_ISREG
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not expected_type(metadata.st_mode)
        or metadata.st_uid != owner_uid
        or metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH)
    ):
        raise RolloverError(f"{kind}_boundary_invalid")
    return metadata


def verify_release_boundary(
    paths: RuntimePaths, approved: ApprovedRequest, *, owner_uid: int
) -> tuple[Path, Path]:
    try:
        current_metadata = os.lstat(paths.release_current)
        releases_root = paths.release_current.parent.resolve(strict=True)
        release_root = paths.release_current.resolve(strict=True)
    except OSError as exc:
        raise RolloverError("release_current_unavailable") from exc
    if (
        not stat.S_ISLNK(current_metadata.st_mode)
        or current_metadata.st_uid != owner_uid
        or release_root.parent != releases_root
        or release_root.name != approved.release_sha
    ):
        raise RolloverError("release_current_identity_invalid")
    secure_lstat(releases_root, owner_uid=owner_uid, kind="release_parent_directory")
    secure_lstat(release_root, owner_uid=owner_uid, kind="release_root_directory")
    expected_files = (
        (SCRIPT_RELATIVE_PATH, approved.rollover_script_sha256),
        (CONFIG_SCRIPT_RELATIVE_PATH, approved.config_script_sha256),
        (POLICY_SCRIPT_RELATIVE_PATH, approved.policy_script_sha256),
    )
    for relative, expected_digest in expected_files:
        cursor = release_root
        for component in relative.parts[:-1]:
            cursor /= component
            secure_lstat(cursor, owner_uid=owner_uid, kind="release_script_directory")
        candidate = release_root / relative
        metadata = secure_lstat(
            candidate, owner_uid=owner_uid, kind="release_script_file"
        )
        if not metadata.st_mode & stat.S_IXUSR:
            raise RolloverError("release_script_not_executable")
        try:
            digest = sha256_bytes(candidate.read_bytes())
        except OSError as exc:
            raise RolloverError("release_script_read_failed") from exc
        if digest != expected_digest:
            raise RolloverError("release_script_digest_mismatch")
    expected_self = release_root / SCRIPT_RELATIVE_PATH
    if paths.self_path.resolve(strict=True) != expected_self:
        raise RolloverError("rollover_script_path_invalid")
    return release_root / CONFIG_SCRIPT_RELATIVE_PATH, release_root / POLICY_SCRIPT_RELATIVE_PATH


def reject_duplicate_evidence_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise RolloverError("deploy_evidence_duplicate_key")
        value[key] = item
    return value


def read_protected_json(path: Path, *, owner_uid: int) -> dict[str, Any]:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise RolloverError("deploy_evidence_unavailable") from exc
    try:
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != owner_uid
            or metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH)
            or not 0 < metadata.st_size <= MAX_STATE_BYTES
        ):
            raise RolloverError("deploy_evidence_boundary_invalid")
        payload = b""
        while len(payload) <= MAX_STATE_BYTES:
            chunk = os.read(
                descriptor, min(16384, MAX_STATE_BYTES + 1 - len(payload))
            )
            if not chunk:
                break
            payload += chunk
        if len(payload) != metadata.st_size:
            raise RolloverError("deploy_evidence_boundary_invalid")
    finally:
        os.close(descriptor)
    try:
        value = json.loads(payload, object_pairs_hook=reject_duplicate_evidence_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RolloverError("deploy_evidence_json_invalid") from exc
    if not isinstance(value, dict):
        raise RolloverError("deploy_evidence_json_invalid")
    return value


def verify_pre_rotation_dry_run(
    *,
    release_current: Path,
    deploy_state_root: Path,
    approved: ApprovedRequest,
    owner_uid: int,
) -> None:
    try:
        release_root = release_current.resolve(strict=True)
    except OSError as exc:
        raise RolloverError("release_current_unavailable") from exc
    if release_root.name != approved.release_sha:
        raise RolloverError("pre_rotation_release_identity_invalid")

    for directory in (
        deploy_state_root,
        deploy_state_root / "requests",
        deploy_state_root / "requests/processed",
        deploy_state_root / "results",
    ):
        secure_lstat(
            directory, owner_uid=owner_uid, kind="deploy_evidence_directory"
        )

    manifest = read_protected_json(
        release_root / "manifest.json", owner_uid=owner_uid
    )
    expected_release_identity: dict[str, Any] = {
        "schema_version": 2,
        "release_sha": approved.release_sha,
        "runtime_sha": approved.release_sha,
        "runtime_artifact_profile": "huabaosi-production",
        "deploy_bundle_sha": approved.release_sha,
        "commit_sha": approved.release_sha,
        "release_scope": EXPECTED_RELEASE_SCOPE,
        "restart_targets": EXPECTED_RESTART_TARGETS,
        "dry_run": False,
    }
    if any(
        manifest.get(name) != expected
        for name, expected in expected_release_identity.items()
    ):
        raise RolloverError("pre_rotation_release_manifest_mismatch")

    request = read_protected_json(
        deploy_state_root
        / "requests/processed"
        / f"{approved.dry_run_request_id}.json",
        owner_uid=owner_uid,
    )
    expected_request_identity: dict[str, Any] = {
        "schema_version": 1,
        "request_id": approved.dry_run_request_id,
        "environment": "production",
        "repository": "qintopia-agent-studio/qintopia-agent-os",
        "commit_sha": approved.release_sha,
        "runtime_sha": approved.release_sha,
        "runtime_artifact_profile": "huabaosi-production",
        "deploy_bundle_sha": approved.release_sha,
        "release_sha": approved.release_sha,
        "release_scope": EXPECTED_RELEASE_SCOPE,
        "restart_targets": EXPECTED_RESTART_TARGETS,
        "rollback_on_smoke_failure": True,
        "dry_run": True,
    }
    if any(
        request.get(name) != expected
        for name, expected in expected_request_identity.items()
    ):
        raise RolloverError("pre_rotation_dry_run_request_mismatch")

    result = read_protected_json(
        deploy_state_root / "results" / f"{approved.dry_run_request_id}.json",
        owner_uid=owner_uid,
    )
    expected_result_identity: dict[str, Any] = {
        "schema_version": 1,
        "request_id": approved.dry_run_request_id,
        "environment": "production",
        "status": "dry_run_succeeded",
        "release_sha": approved.release_sha,
        "commit_sha": approved.release_sha,
        "runtime_sha": approved.release_sha,
        "runtime_artifact_profile": "huabaosi-production",
        "deploy_bundle_sha": approved.release_sha,
        "release_scope": EXPECTED_RELEASE_SCOPE,
        "current_target": str(release_root),
        "restart_targets": EXPECTED_RESTART_TARGETS,
    }
    if any(
        result.get(name) != expected
        for name, expected in expected_result_identity.items()
    ):
        raise RolloverError("pre_rotation_dry_run_result_mismatch")
    checks = result.get("checks")
    rollback = result.get("rollback")
    if (
        not isinstance(checks, list)
        or not checks
        or any(
            not isinstance(check, dict)
            or not isinstance(check.get("name"), str)
            or not check["name"]
            or check.get("status") != "passed"
            for check in checks
        )
        or sum(
            check.get("name") == "deploy-runner" and check.get("status") == "passed"
            for check in checks
            if isinstance(check, dict)
        )
        != 1
        or not isinstance(rollback, dict)
        or rollback.get("attempted") is not False
        or rollback.get("status") != "not_needed"
        or result.get("error") not in {None, ""}
    ):
        raise RolloverError("pre_rotation_dry_run_result_mismatch")


def fsync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


class StateStore:
    def __init__(self, root: Path, *, owner_uid: int) -> None:
        self.root = root
        self.owner_uid = owner_uid

    def ensure(self) -> None:
        try:
            self.root.mkdir(mode=0o700)
        except FileExistsError:
            pass
        metadata = secure_lstat(
            self.root, owner_uid=self.owner_uid, kind="rollover_state_directory"
        )
        if stat.S_IMODE(metadata.st_mode) != 0o700:
            raise RolloverError("rollover_state_directory_mode_invalid")

    def state_path(self, operation_id: str) -> Path:
        return self.root / f"{operation_id}.state.json"

    def receipt_path(self, operation_id: str) -> Path:
        return self.root / f"{operation_id}.receipt.json"

    @contextmanager
    def lock(self) -> Iterator[None]:
        self.ensure()
        flags = os.O_RDWR | os.O_CREAT | getattr(os, "O_CLOEXEC", 0) | getattr(
            os, "O_NOFOLLOW", 0
        )
        descriptor = os.open(self.root / "transition.lock", flags, 0o600)
        try:
            os.fchmod(descriptor, 0o600)
            metadata = os.fstat(descriptor)
            if (
                not stat.S_ISREG(metadata.st_mode)
                or metadata.st_uid != self.owner_uid
                or stat.S_IMODE(metadata.st_mode) != 0o600
            ):
                raise RolloverError("rollover_lock_boundary_invalid")
            try:
                fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise RolloverError("rollover_command_already_running") from exc
            yield
        finally:
            os.close(descriptor)

    def _write(self, path: Path, value: dict[str, Any]) -> None:
        self.ensure()
        descriptor, temporary_name = tempfile.mkstemp(
            prefix=f".{path.name}.", suffix=".tmp", dir=self.root
        )
        temporary = Path(temporary_name)
        try:
            os.fchmod(descriptor, 0o600)
            with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
                descriptor = -1
                json.dump(value, handle, sort_keys=True, separators=(",", ":"))
                handle.write("\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary, path)
            fsync_directory(self.root)
        except BaseException:
            if descriptor >= 0:
                os.close(descriptor)
            temporary.unlink(missing_ok=True)
            raise

    def _read(self, path: Path) -> dict[str, Any]:
        flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
        try:
            descriptor = os.open(path, flags)
        except OSError as exc:
            raise RolloverError("rollover_record_unavailable") from exc
        try:
            metadata = os.fstat(descriptor)
            if (
                not stat.S_ISREG(metadata.st_mode)
                or metadata.st_uid != self.owner_uid
                or stat.S_IMODE(metadata.st_mode) != 0o600
                or metadata.st_size > MAX_STATE_BYTES
            ):
                raise RolloverError("rollover_record_boundary_invalid")
            payload = b""
            while len(payload) <= metadata.st_size:
                chunk = os.read(descriptor, metadata.st_size + 1 - len(payload))
                if not chunk:
                    break
                payload += chunk
        finally:
            os.close(descriptor)
        try:
            value = json.loads(payload)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise RolloverError("rollover_record_payload_invalid") from exc
        if not isinstance(value, dict):
            raise RolloverError("rollover_record_payload_invalid")
        return value

    def write_state(self, value: dict[str, Any]) -> None:
        self._write(self.state_path(str(value["operation_id"])), value)

    def read_state(self, operation_id: str) -> dict[str, Any] | None:
        path = self.state_path(operation_id)
        return self._read(path) if os.path.lexists(path) else None

    def write_receipt(self, value: dict[str, Any]) -> None:
        self._write(self.receipt_path(str(value["operation_id"])), value)

    def read_receipt(self, operation_id: str) -> dict[str, Any] | None:
        path = self.receipt_path(operation_id)
        return self._read(path) if os.path.lexists(path) else None

    def delete_state(self, operation_id: str) -> None:
        path = self.state_path(operation_id)
        if os.path.lexists(path):
            path.unlink()
            fsync_directory(self.root)

    def cleanup_temporary_records(self, operation_id: str) -> int:
        self.ensure()
        flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(
            os, "O_CLOEXEC", 0
        ) | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(self.root, flags)
        removed = 0
        try:
            for name in os.listdir(descriptor):
                match = STATE_TEMP_NAME_RE.fullmatch(name)
                if match is None or match.group(1) != operation_id:
                    continue
                try:
                    metadata = os.stat(
                        name, dir_fd=descriptor, follow_symlinks=False
                    )
                except OSError as exc:
                    raise RolloverError(
                        "rollover_temporary_record_unavailable"
                    ) from exc
                if (
                    not stat.S_ISREG(metadata.st_mode)
                    or metadata.st_uid != self.owner_uid
                    or stat.S_IMODE(metadata.st_mode) != 0o600
                    or metadata.st_size > MAX_STATE_BYTES
                ):
                    raise RolloverError(
                        "rollover_temporary_record_boundary_invalid"
                    )
                try:
                    os.unlink(name, dir_fd=descriptor)
                except OSError as exc:
                    raise RolloverError(
                        "rollover_temporary_record_cleanup_failed"
                    ) from exc
                removed += 1
            if removed:
                os.fsync(descriptor)
        finally:
            os.close(descriptor)
        return removed

    def assert_secret_state_removed(self, operation_id: str) -> None:
        if os.path.lexists(self.state_path(operation_id)):
            raise RolloverError("rollover_secret_state_cleanup_incomplete")
        for path in self.root.iterdir():
            match = STATE_TEMP_NAME_RE.fullmatch(path.name)
            if match is not None and match.group(1) == operation_id:
                raise RolloverError("rollover_secret_state_cleanup_incomplete")

    def assert_no_other_active_state(self, operation_id: str) -> None:
        active = [
            path
            for path in self.root.glob("*.state.json")
            if path.name != f"{operation_id}.state.json"
        ]
        if active:
            raise RolloverError("another_rollover_operation_is_active")
        for path in self.root.iterdir():
            match = STATE_TEMP_NAME_RE.fullmatch(path.name)
            if match is not None and match.group(1) != operation_id:
                raise RolloverError("another_rollover_operation_is_active")


def validate_record_identity(record: dict[str, Any], approved: ApprovedRequest) -> None:
    if record.get("schema_version") != 1:
        raise RolloverError("rollover_record_schema_invalid")
    for name, expected in approved.public_identity().items():
        if record.get(name) != expected:
            raise RolloverError("rollover_record_identity_mismatch")
    previous_hash = record.get("previous_database_url_sha256")
    new_hash = record.get("new_database_url_sha256")
    if (
        previous_hash != approved.old_database_url_sha256
        or not isinstance(new_hash, str)
        or not SHA256_RE.fullmatch(new_hash)
        or new_hash == previous_hash
    ):
        raise RolloverError("rollover_record_database_identity_mismatch")
    old_url = record.get("old_url")
    if old_url is not None and (
        not isinstance(old_url, str)
        or sha256_bytes(old_url.encode("utf-8")) != previous_hash
    ):
        raise RolloverError("rollover_record_database_identity_mismatch")
    new_url = record.get("new_url")
    if new_url is not None and (
        not isinstance(new_url, str)
        or sha256_bytes(new_url.encode("utf-8")) != new_hash
    ):
        raise RolloverError("rollover_record_database_identity_mismatch")
    if "phase" in record:
        role_name = record.get("role_name")
        chat_id = record.get("chat_id")
        user_id = record.get("user_id")
        if any(
            not isinstance(value, str) or not value
            for value in (old_url, new_url, role_name, chat_id, user_id)
        ):
            raise RolloverError("rollover_state_target_identity_invalid")
        assert_password_only_rotation(old_url, new_url)
        expected_refs = {
            "role_ref": opaque_ref(["postgres-role-v1", role_name]),
            "conversation_ref": opaque_ref(
                ["conversation-ref-v3", "feishu", chat_id]
            ),
            "actor_ref": opaque_ref(["poster-actor-v1", "feishu", user_id]),
        }
        if any(record.get(name) != value for name, value in expected_refs.items()):
            raise RolloverError("rollover_state_target_identity_invalid")


def parse_env_values(path: Path, wanted: set[str]) -> dict[str, str]:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise RolloverError("runtime_env_unavailable") from exc
    try:
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH)
            or metadata.st_size > MAX_ENV_BYTES
        ):
            raise RolloverError("runtime_env_boundary_invalid")
        chunks: list[bytes] = []
        total = 0
        while True:
            chunk = os.read(descriptor, min(16384, MAX_ENV_BYTES + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > MAX_ENV_BYTES:
                raise RolloverError("runtime_env_boundary_invalid")
    finally:
        os.close(descriptor)
    try:
        text = b"".join(chunks).decode("utf-8")
    except UnicodeDecodeError as exc:
        raise RolloverError("runtime_env_assignment_invalid") from exc
    values: dict[str, str] = {}
    for raw in text.splitlines():
        match = ASSIGNMENT_RE.fullmatch(raw)
        if not match or match.group(1) not in wanted:
            continue
        if match.group(1) in values:
            raise RolloverError("runtime_env_assignment_duplicate")
        try:
            parts = shlex.split(match.group(2), comments=True, posix=True)
        except ValueError as exc:
            raise RolloverError("runtime_env_assignment_invalid") from exc
        if len(parts) != 1:
            raise RolloverError("runtime_env_assignment_invalid")
        values[match.group(1)] = parts[0]
    return values


def one_identifier(value: str, *, code: str) -> str:
    items = [item.strip() for item in value.split(",") if item.strip()]
    if len(items) != 1 or len(set(items)) != 1:
        raise RolloverError(code)
    return items[0]


def parse_report_json(output: str, *, prefix: str = "") -> dict[str, Any]:
    value = output.strip()
    if prefix:
        if not value.startswith(prefix):
            raise RolloverError("protected_command_evidence_invalid")
        value = value[len(prefix) :]
    try:
        report = json.loads(value)
    except json.JSONDecodeError as exc:
        raise RolloverError("protected_command_evidence_invalid") from exc
    if not isinstance(report, dict):
        raise RolloverError("protected_command_evidence_invalid")
    return report


class ProductionOperations:
    def __init__(
        self,
        *,
        paths: RuntimePaths,
        approved: ApprovedRequest,
        config_script: Path,
        policy_script: Path,
        psql: PsqlClient | None = None,
        deploy_state_root: Path = DEPLOY_STATE_ROOT_PATH,
    ) -> None:
        self.paths = paths
        self.approved = approved
        self.config_script = config_script
        self.policy_script = policy_script
        self.psql = psql or PsqlClient()
        self.deploy_state_root = deploy_state_root

    def verify_pre_rotation_gate(self) -> None:
        verify_pre_rotation_dry_run(
            release_current=self.paths.release_current,
            deploy_state_root=self.deploy_state_root,
            approved=self.approved,
            owner_uid=os.geteuid(),
        )

    def initial_context(self) -> InitialContext:
        values = parse_env_values(
            self.paths.sidecar_env,
            {
                "QINTOPIA_SIDECAR_DATABASE_URL",
                "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS",
                "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS",
            },
        )
        if set(values) != {
            "QINTOPIA_SIDECAR_DATABASE_URL",
            "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS",
            "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS",
        }:
            raise RolloverError("runtime_identity_binding_incomplete")
        old_url = values["QINTOPIA_SIDECAR_DATABASE_URL"]
        _, role_name, _, _ = database_url_parts(old_url)
        chat_id = one_identifier(
            values["QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS"],
            code="direct_chat_ceiling_invalid",
        )
        user_id = one_identifier(
            values["QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS"],
            code="direct_user_ceiling_invalid",
        )
        expected = {
            "old_database_url_sha256": sha256_bytes(old_url.encode("utf-8")),
            "role_ref": opaque_ref(["postgres-role-v1", role_name]),
            "conversation_ref": opaque_ref(["conversation-ref-v3", "feishu", chat_id]),
            "actor_ref": opaque_ref(["poster-actor-v1", "feishu", user_id]),
        }
        if any(getattr(self.approved, name) != value for name, value in expected.items()):
            raise RolloverError("operator_approved_target_binding_mismatch")
        return InitialContext(old_url, role_name, chat_id, user_id)

    def credential_evidence(self, state: dict[str, Any]) -> CredentialEvidence:
        return credential_evidence(
            self.psql,
            state_text(state, "old_url"),
            state_text(state, "new_url"),
            state_text(state, "role_name"),
        )

    def alter_password(self, state: dict[str, Any]) -> None:
        password = database_url_parts(state_text(state, "new_url"))[2]
        verifier = scram_verifier(password)
        self.psql.alter_password(
            state_text(state, "old_url"), state_text(state, "role_name"), verifier
        )

    def _config_payload(self, state: dict[str, Any], database_url: str | None) -> bytes:
        payload: dict[str, Any] = {
            "schema_version": 1,
            "desired_state": "direct" if database_url is not None else "disabled",
            "release_sha": self.approved.release_sha,
        }
        if database_url is not None:
            old_url = state_text(state, "old_url")
            new_url = state_text(state, "new_url")
            if database_url == new_url:
                previous_url = old_url
            elif database_url == old_url:
                previous_url = new_url
            else:
                raise RolloverError("protected_config_database_target_invalid")
            payload.update(
                {
                    "database_url_sha256": sha256_bytes(database_url.encode("utf-8")),
                    "database_url": database_url,
                    "previous_database_url_sha256": sha256_bytes(
                        previous_url.encode("utf-8")
                    ),
                    "rotate_ingress_hmac": True,
                    "allowed_chat_ids": [state_text(state, "chat_id")],
                    "allowed_user_ids": [state_text(state, "user_id")],
                    "reviewer_user_ids": [state_text(state, "user_id")],
                }
            )
        return json.dumps(payload, separators=(",", ":")).encode("utf-8")

    def _run_protected(
        self, script: Path, arguments: list[str], payload: bytes, *, timeout: int
    ) -> subprocess.CompletedProcess[bytes]:
        try:
            return subprocess.run(
                ["/usr/bin/python3", str(script), *arguments],
                input=payload,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                timeout=timeout,
                env={
                    "PATH": "/usr/bin:/bin",
                    "PYTHONDONTWRITEBYTECODE": "1",
                },
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise RolloverError("protected_command_result_unknown") from exc

    def run_config(
        self, state: dict[str, Any], database_url: str | None, *, apply: bool
    ) -> dict[str, Any]:
        arguments = ["--stdin"]
        if apply:
            arguments.extend(["--apply", "--approval", CONFIG_APPROVAL])
        result = self._run_protected(
            self.config_script,
            arguments,
            self._config_payload(state, database_url),
            timeout=90,
        )
        if result.returncode != 0:
            raise RolloverError("protected_config_command_failed")
        try:
            output = result.stdout.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise RolloverError("protected_config_evidence_invalid") from exc
        report = parse_report_json(
            output, prefix="xiaoman_feishu_production_config="
        )
        expected = {
            "success": True,
            "action_status": "production_config_applied" if apply else "production_config_ready",
            "desired_state": "direct" if database_url is not None else "disabled",
            "release_sha": self.approved.release_sha,
            "external_calls_executed": False,
            "database_writes_executed": False,
            "service_changes_executed": False,
            "sensitive_values_redacted": True,
        }
        if any(report.get(name) != value for name, value in expected.items()):
            raise RolloverError("protected_config_evidence_rejected")
        staged_removed = report.get("staged_secret_files_removed_count")
        if apply and (
            report.get("staged_secret_files_absent") is not True
            or type(staged_removed) is not int
            or staged_removed < 0
        ):
            raise RolloverError("protected_config_stage_evidence_rejected")
        if database_url is not None and (
            report.get("database_url_sha256_matched") is not True
            or report.get("chat_allowlist_count") != 1
            or report.get("user_allowlist_count") != 1
            or report.get("reviewer_allowlist_count") != 1
            or report.get("database_hash_binding_count") != len(DATABASE_HASH_KEYS)
            or report.get("ingress_hmac_action") != "rotated"
            or report.get("erhua_database_binding_checked") is not True
            or report.get("shared_database_env_count") != 2
            or type(report.get("erhua_change_required")) is not bool
            or type(report.get("database_url_rotated")) is not bool
            or type(report.get("previous_database_url_sha256_matched")) is not bool
        ):
            raise RolloverError("protected_direct_config_evidence_rejected")
        return report

    def cleanup_config_stage_files(self) -> dict[str, Any]:
        result = self._run_protected(
            self.config_script,
            [
                "--cleanup-staged-files",
                "--release-sha",
                self.approved.release_sha,
                "--approval",
                CONFIG_APPROVAL,
            ],
            b"",
            timeout=30,
        )
        if result.returncode != 0:
            raise RolloverError("protected_config_stage_cleanup_failed")
        try:
            output = result.stdout.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise RolloverError("protected_config_stage_cleanup_evidence_invalid") from exc
        report = parse_report_json(
            output, prefix="xiaoman_feishu_production_config="
        )
        expected = {
            "success": True,
            "action_status": "production_config_stage_cleanup_completed",
            "release_sha": self.approved.release_sha,
            "staged_secret_files_absent": True,
            "external_calls_executed": False,
            "database_writes_executed": False,
            "service_changes_executed": False,
            "sensitive_values_redacted": True,
        }
        removed = report.get("staged_secret_files_removed_count")
        if (
            any(report.get(name) != value for name, value in expected.items())
            or type(removed) is not int
            or removed < 0
        ):
            raise RolloverError("protected_config_stage_cleanup_evidence_rejected")
        return report

    def configuration_matches(
        self, state: dict[str, Any], database_url: str, desired_state: str
    ) -> bool:
        if desired_state not in {"direct", "disabled"}:
            return False
        wanted = set(DATABASE_HASH_KEYS) | {
            "QINTOPIA_SIDECAR_DATABASE_URL",
            "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
            "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED",
        }
        hermes_wanted = {
            "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE",
            "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
            "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED",
        }
        if desired_state == "direct":
            wanted.update(
                {
                    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
                    "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY",
                    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS",
                    "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS",
                    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS",
                    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS",
                }
            )
            hermes_wanted.update(
                {
                    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
                    "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY",
                }
            )
        try:
            sidecar = parse_env_values(self.paths.sidecar_env, wanted)
            hermes = parse_env_values(self.paths.hermes_env, hermes_wanted)
            erhua = parse_env_values(
                self.paths.erhua_env, {"QINTOPIA_SIDECAR_DATABASE_URL"}
            )
        except RolloverError:
            return False
        if (
            set(sidecar) != wanted
            or set(hermes) != hermes_wanted
            or set(erhua) != {"QINTOPIA_SIDECAR_DATABASE_URL"}
        ):
            return False
        enabled = "1" if desired_state == "direct" else "0"
        expected_hash = sha256_bytes(database_url.encode("utf-8"))
        base_matches = (
            sidecar["QINTOPIA_SIDECAR_DATABASE_URL"] == database_url
            and erhua["QINTOPIA_SIDECAR_DATABASE_URL"] == database_url
            and all(sidecar[name] == expected_hash for name in DATABASE_HASH_KEYS)
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED"] == enabled
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] == enabled
            and hermes["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"] == enabled
            and hermes["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] == enabled
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"] == "0"
            and hermes["QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"] == "0"
        )
        if not base_matches or desired_state != "direct":
            return base_matches
        chat_id = state_text(state, "chat_id")
        user_id = state_text(state, "user_id")
        return (
            sidecar["QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS"] == chat_id
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS"] == user_id
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS"] == chat_id
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS"] == user_id
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"]
            == hermes["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"]
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"]
            == hermes["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"]
            and sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"]
            != sidecar["QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"]
        )

    def persistent_database_binding(self, state: dict[str, Any]) -> str:
        wanted = set(DATABASE_HASH_KEYS) | {"QINTOPIA_SIDECAR_DATABASE_URL"}
        try:
            values = parse_env_values(self.paths.sidecar_env, wanted)
            erhua = parse_env_values(
                self.paths.erhua_env, {"QINTOPIA_SIDECAR_DATABASE_URL"}
            )
        except RolloverError:
            return "other"
        if (
            set(values) != wanted
            or set(erhua) != {"QINTOPIA_SIDECAR_DATABASE_URL"}
        ):
            return "other"
        old_hash = self.approved.old_database_url_sha256
        new_hash = state_text(state, "new_database_url_sha256")
        observed = {
            sha256_bytes(values["QINTOPIA_SIDECAR_DATABASE_URL"].encode("utf-8")),
            sha256_bytes(
                erhua["QINTOPIA_SIDECAR_DATABASE_URL"].encode("utf-8")
            ),
            *(values[name] for name in DATABASE_HASH_KEYS),
        }
        if observed == {old_hash}:
            return "old"
        if observed == {new_hash}:
            return "rotated"
        if observed.issubset({old_hash, new_hash}):
            return "mixed"
        return "other"

    def _policy_payload(self, state: dict[str, Any]) -> bytes:
        return json.dumps(
            {
                "schema_version": 3,
                "policies": [
                    {
                        "platform": "feishu",
                        "chat_id": state_text(state, "chat_id"),
                        "conversation_type": "direct",
                        "audience_class": "private",
                        "allowed_capabilities": [
                            "poster_production_request",
                            "poster_workflow_status",
                        ],
                        "return_mode": "direct_chat",
                        "initiation_rule": "direct_message",
                        "status_visibility": "requester",
                        "enabled": True,
                        "reviewer_user_ids": [],
                    }
                ],
            },
            separators=(",", ":"),
        ).encode("utf-8")

    def apply_private_policy(self, state: dict[str, Any]) -> dict[str, Any]:
        result = self._run_protected(
            self.policy_script,
            ["--stdin", "--apply", "--approval", POLICY_APPROVAL],
            self._policy_payload(state),
            timeout=90,
        )
        if result.returncode != 0:
            raise RolloverError("protected_policy_command_failed")
        try:
            report = parse_report_json(result.stdout.decode("utf-8"))
        except UnicodeDecodeError as exc:
            raise RolloverError("protected_policy_evidence_invalid") from exc
        expected = {
            "success": True,
            "action_status": "conversation_policies_applied",
            "input_count": 1,
            "database_url_sha256": sha256_bytes(
                state_text(state, "new_url").encode("utf-8")
            ),
            "approved_database_url_sha256_matched": True,
            "external_calls_executed": False,
            "sensitive_fields_redacted": True,
        }
        policies = report.get("policies")
        if (
            any(report.get(name) != value for name, value in expected.items())
            or not isinstance(policies, list)
            or len(policies) != 1
            or type(report.get("created_version_count")) is not int
            or type(report.get("deduped_count")) is not int
            or report["created_version_count"] + report["deduped_count"] != 1
        ):
            raise RolloverError("protected_policy_evidence_rejected")
        policy = policies[0]
        if (
            not isinstance(policy, dict)
            or policy.get("conversation_ref") != self.approved.conversation_ref
            or not isinstance(policy.get("policy_digest"), str)
            or not OPAQUE_REF_RE.fullmatch(policy["policy_digest"])
            or type(policy.get("policy_version")) is not int
            or policy["policy_version"] < 1
            or policy.get("enabled") is not True
            or policy.get("reviewer_count") != 0
            or type(policy.get("deduped")) is not bool
        ):
            raise RolloverError("protected_policy_target_mismatch")
        return policy

    def policy_matches(self, state: dict[str, Any]) -> bool:
        expected_ref = self.approved.conversation_ref
        policy_digest = state_text(state, "policy_digest")
        policy_version = state_int(state, "policy_version")
        output = self.psql.query(
            state_text(state, "new_url"),
            f"""
WITH policies AS (
  SELECT policy.*,
         (SELECT count(*) FROM qintopia_agent_os.conversation_policy_actors actor
          WHERE actor.policy_id = policy.id) AS reviewer_count
  FROM qintopia_agent_os.conversation_policies policy
  WHERE policy.enabled
)
SELECT json_build_object(
  'enabled_count', count(*),
  'exact_target_count', count(*) FILTER (
    WHERE platform = 'feishu'
      AND conversation_ref = {sql_literal(expected_ref)}
      AND policy_version = {policy_version}
      AND policy_digest = {sql_literal(policy_digest)}
      AND conversation_type = 'direct'
      AND audience_class = 'private'
      AND allowed_capabilities = ARRAY['poster_production_request','poster_workflow_status']::text[]
      AND return_mode = 'direct_chat'
      AND initiation_rule = 'direct_message'
      AND status_visibility = 'requester'
      AND reviewer_count = 0
  ),
  'enabled_group_count', count(*) FILTER (
    WHERE conversation_type = 'group' AND audience_class = 'internal_collaboration'
  )
)::text
FROM policies;
""",
        )
        try:
            report = json.loads(output.splitlines()[-1])
        except (IndexError, json.JSONDecodeError) as exc:
            raise RolloverError("policy_database_evidence_invalid") from exc
        return report == {
            "enabled_count": 1,
            "exact_target_count": 1,
            "enabled_group_count": 0,
        }

    def verify_runtime_reload(self, state: dict[str, Any], database_url: str) -> None:
        expected_hash = sha256_bytes(database_url.encode("utf-8"))
        config_boot_id = state_text(state, "config_applied_boot_id")
        config_monotonic = state_int(state, "config_applied_monotonic_us")
        current_boot_id = boot_id()
        for unit in CORE_SERVICES:
            properties = systemd_properties(unit)
            try:
                pid = int(properties.get("MainPID", "0"))
                started = int(
                    properties.get("ExecMainStartTimestampMonotonic", "0")
                )
            except ValueError as exc:
                raise RolloverError("core_service_evidence_invalid") from exc
            if (
                properties.get("ActiveState") != "active"
                or properties.get("Result") != "success"
                or properties.get("ExecMainStatus") != "0"
                or pid <= 0
                or (current_boot_id == config_boot_id and started <= config_monotonic)
            ):
                raise RolloverError("core_service_reload_gate_failed")
            database_hash, release_sha = process_runtime_binding(pid)
            if database_hash != expected_hash or release_sha != self.approved.release_sha:
                raise RolloverError("core_service_process_binding_failed")
        old_url = state_text(state, "old_url")
        new_url = state_text(state, "new_url")
        if database_url == new_url:
            retired_url = old_url
        elif database_url == old_url:
            retired_url = new_url
        else:
            raise RolloverError("runtime_database_target_invalid")
        verify_retired_process_credential(
            retired_url, database_url, minimum_new=len(CORE_SERVICES)
        )


def systemd_properties(unit: str) -> dict[str, str]:
    try:
        result = subprocess.run(
            [
                "/usr/bin/systemctl",
                "show",
                unit,
                "--property=ActiveState",
                "--property=MainPID",
                "--property=ExecMainStartTimestampMonotonic",
                "--property=Result",
                "--property=ExecMainStatus",
                "--no-pager",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
            timeout=15,
            env={"PATH": "/usr/bin:/bin:/usr/sbin:/sbin"},
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise RolloverError("systemd_state_query_failed") from exc
    if result.returncode != 0:
        raise RolloverError("systemd_state_query_failed")
    values: dict[str, str] = {}
    for line in result.stdout.splitlines():
        if "=" in line:
            name, value = line.split("=", 1)
            values[name] = value
    return values


def process_runtime_binding(pid: int) -> tuple[str, str]:
    try:
        payload = Path(f"/proc/{pid}/environ").read_bytes()
    except OSError as exc:
        raise RolloverError("service_environment_unavailable") from exc
    values: dict[bytes, bytes] = {}
    for item in payload.split(b"\x00"):
        if b"=" not in item:
            continue
        name, value = item.split(b"=", 1)
        if name in {b"QINTOPIA_SIDECAR_DATABASE_URL", b"QINTOPIA_DEPLOYED_COMMIT_SHA"}:
            if name in values:
                raise RolloverError("service_environment_duplicate")
            values[name] = value
    if set(values) != {
        b"QINTOPIA_SIDECAR_DATABASE_URL",
        b"QINTOPIA_DEPLOYED_COMMIT_SHA",
    }:
        raise RolloverError("service_environment_missing")
    try:
        release_sha = values[b"QINTOPIA_DEPLOYED_COMMIT_SHA"].decode("ascii")
    except UnicodeDecodeError as exc:
        raise RolloverError("service_release_binding_invalid") from exc
    return (
        sha256_bytes(values[b"QINTOPIA_SIDECAR_DATABASE_URL"]),
        release_sha,
    )


def verify_retired_process_credential(
    retired_url: str, current_url: str, *, minimum_new: int
) -> None:
    if retired_url == current_url:
        raise RolloverError("process_database_credential_target_invalid")
    retired = retired_url.encode("utf-8")
    current = current_url.encode("utf-8")
    retired_count = 0
    current_count = 0
    for process_dir in Path("/proc").iterdir():
        if not process_dir.name.isdigit():
            continue
        try:
            payload = (process_dir / "environ").read_bytes()
        except PermissionError as exc:
            raise RolloverError("process_environment_inventory_incomplete") from exc
        except (FileNotFoundError, ProcessLookupError):
            continue
        for item in payload.split(b"\x00"):
            if b"=" not in item:
                continue
            _, value = item.split(b"=", 1)
            if value == retired:
                retired_count += 1
            elif value == current:
                current_count += 1
    if retired_count != 0 or current_count < minimum_new:
        raise RolloverError("process_database_credential_retirement_failed")


def state_text(state: dict[str, Any], name: str) -> str:
    value = state.get(name)
    if not isinstance(value, str) or not value:
        raise RolloverError("rollover_state_field_invalid")
    return value


def state_int(state: dict[str, Any], name: str) -> int:
    value = state.get(name)
    if type(value) is not int or value <= 0:
        raise RolloverError("rollover_state_field_invalid")
    return value


class RolloverMachine:
    def __init__(
        self,
        *,
        approved: ApprovedRequest,
        store: StateStore,
        operations: Any,
        password_factory: Callable[[], str] | None = None,
        now: Callable[[], str] = utc_now,
        boot_id_factory: Callable[[], str] = boot_id,
        monotonic_factory: Callable[[], int] = boot_monotonic_us,
        terminal_hook: Callable[[], None] | None = None,
    ) -> None:
        self.approved = approved
        self.store = store
        self.operations = operations
        self.password_factory = password_factory or (lambda: secrets.token_urlsafe(48))
        self.now = now
        self.boot_id_factory = boot_id_factory
        self.monotonic_factory = monotonic_factory
        self.terminal_hook = terminal_hook

    def _new_state(self) -> dict[str, Any]:
        context = self.operations.initial_context()
        password = self.password_factory()
        new_url = rotated_database_url(context.old_url, password)
        state: dict[str, Any] = {
            "schema_version": 1,
            **self.approved.public_identity(),
            "phase": "escrowed",
            "old_url": context.old_url,
            "new_url": new_url,
            "previous_database_url_sha256": self.approved.old_database_url_sha256,
            "new_database_url_sha256": sha256_bytes(new_url.encode("utf-8")),
            "role_name": context.role_name,
            "chat_id": context.chat_id,
            "user_id": context.user_id,
            "created_at": self.now(),
            "updated_at": self.now(),
        }
        self.store.write_state(state)
        return state

    def _load_state(self, *, create: bool = False) -> dict[str, Any]:
        state = self.store.read_state(self.approved.operation_id)
        if state is None:
            if not create:
                raise RolloverError("rollover_state_missing")
            state = self._new_state()
        validate_record_identity(state, self.approved)
        if state.get("phase") not in NONTERMINAL_PHASES:
            raise RolloverError("rollover_phase_invalid")
        return state

    def _update(self, state: dict[str, Any], phase: str, **values: Any) -> None:
        state["phase"] = phase
        state.update(values)
        state["updated_at"] = self.now()
        self.store.write_state(state)

    def _terminal_report(self, receipt: dict[str, Any], *, deduped: bool) -> dict[str, Any]:
        return {
            "success": True,
            "action_status": receipt["outcome"],
            "operation_id": self.approved.operation_id,
            "release_sha": self.approved.release_sha,
            "credential_binding": receipt["credential_binding"],
            "active_database_url_sha256": receipt["active_database_url_sha256"],
            "poster_configuration": receipt["poster_configuration"],
            "policy_applied": receipt["policy_applied"],
            "terminal_receipt_persisted": True,
            "secret_state_removed": receipt.get("secret_cleanup_completed") is True,
            "deduped": deduped,
            "feishu_calls_executed": False,
            "service_changes_executed": False,
            "sensitive_values_redacted": True,
        }

    def _reconcile_receipt(self, receipt: dict[str, Any]) -> dict[str, Any]:
        validate_record_identity(receipt, self.approved)
        outcome = receipt.get("outcome")
        if outcome not in {
            "password_rollover_forward_completed",
            "password_rollover_rollback_completed",
            "password_rollover_aborted",
        }:
            raise RolloverError("rollover_receipt_invalid")
        credential_binding = receipt.get("credential_binding")
        active_hash = receipt.get("active_database_url_sha256")
        expected_active_hash = (
            receipt.get("new_database_url_sha256")
            if credential_binding == "rotated"
            else receipt.get("previous_database_url_sha256")
        )
        if (
            credential_binding not in {"old", "rotated"}
            or type(receipt.get("policy_applied")) is not bool
            or not isinstance(active_hash, str)
            or not SHA256_RE.fullmatch(active_hash)
            or active_hash != expected_active_hash
            or (
                outcome == "password_rollover_forward_completed"
                and (
                    credential_binding != "rotated"
                    or receipt.get("poster_configuration") != "direct"
                    or receipt.get("policy_applied") is not True
                )
            )
            or (
                outcome == "password_rollover_aborted"
                and (
                    credential_binding != "old"
                    or receipt.get("poster_configuration") != "disabled"
                )
            )
            or (
                outcome == "password_rollover_rollback_completed"
                and receipt.get("poster_configuration") != "disabled"
            )
        ):
            raise RolloverError("rollover_receipt_invalid")
        if receipt.get("secret_cleanup_completed") is not True:
            self.operations.cleanup_config_stage_files()
            state = self.store.read_state(self.approved.operation_id)
            if state is not None:
                validate_record_identity(state, self.approved)
                for name in (
                    "previous_database_url_sha256",
                    "new_database_url_sha256",
                ):
                    if receipt.get(name) != state.get(name):
                        raise RolloverError("rollover_receipt_state_identity_mismatch")
                self.store.delete_state(self.approved.operation_id)
            self.store.cleanup_temporary_records(self.approved.operation_id)
            self.store.assert_secret_state_removed(self.approved.operation_id)
            receipt["secret_cleanup_completed"] = True
            receipt["cleanup_completed_at"] = self.now()
            self.store.write_receipt(receipt)
        return self._terminal_report(receipt, deduped=True)

    def _complete(
        self,
        state: dict[str, Any],
        *,
        outcome: str,
        credential_binding: str,
        poster_configuration: str,
    ) -> dict[str, Any]:
        self.operations.cleanup_config_stage_files()
        receipt: dict[str, Any] = {
            "schema_version": 1,
            **self.approved.public_identity(),
            "previous_database_url_sha256": state_text(
                state, "previous_database_url_sha256"
            ),
            "new_database_url_sha256": state_text(
                state, "new_database_url_sha256"
            ),
            "outcome": outcome,
            "credential_binding": credential_binding,
            "active_database_url_sha256": (
                state_text(state, "new_database_url_sha256")
                if credential_binding == "rotated"
                else self.approved.old_database_url_sha256
            ),
            "poster_configuration": poster_configuration,
            "policy_applied": state.get("policy_digest") is not None,
            "terminal_committed_at": self.now(),
            "secret_cleanup_completed": False,
        }
        self.store.write_receipt(receipt)
        if self.terminal_hook is not None:
            self.terminal_hook()
        self.store.delete_state(self.approved.operation_id)
        self.store.cleanup_temporary_records(self.approved.operation_id)
        self.store.assert_secret_state_removed(self.approved.operation_id)
        receipt["secret_cleanup_completed"] = True
        receipt["cleanup_completed_at"] = self.now()
        self.store.write_receipt(receipt)
        return self._terminal_report(receipt, deduped=False)

    def _credential(self, state: dict[str, Any]) -> CredentialEvidence:
        evidence = self.operations.credential_evidence(state)
        if evidence.state not in {"rotated", "unrotated", "ambiguous"}:
            raise RolloverError("credential_evidence_invalid")
        return evidence

    def prepare(self) -> dict[str, Any]:
        if self.store.read_state(self.approved.operation_id) is None:
            self.operations.verify_pre_rotation_gate()
        state = self._load_state(create=True)
        phase = state_text(state, "phase")
        if phase in {
            "direct_config_applied",
            "reload_verified",
            "private_policy_applied",
        }:
            return self._phase_report(state, deduped=True)
        if phase == "rollback_config_applied":
            raise RolloverError("prepare_after_rollback_forbidden")
        if phase == "escrowed":
            if self.operations.persistent_database_binding(state) != "old":
                raise RolloverError("initial_database_configuration_binding_invalid")
            self.operations.run_config(state, state_text(state, "new_url"), apply=False)
            self._update(state, "preview_validated")
        evidence = self._credential(state)
        if evidence.state == "unrotated":
            self._update(state, "alter_in_flight")
            try:
                self.operations.alter_password(state)
            except RolloverError:
                pass
            evidence = self._credential(state)
        if evidence.state != "rotated":
            raise RolloverError("database_password_rotation_ambiguous")
        if state_text(state, "phase") != "credential_rotated":
            self._update(state, "credential_rotated")
        new_url = state_text(state, "new_url")
        persistent_binding = self.operations.persistent_database_binding(state)
        if persistent_binding not in {"old", "rotated", "mixed"}:
            raise RolloverError("unexpected_database_configuration_binding")
        if not self.operations.configuration_matches(state, new_url, "direct"):
            try:
                self.operations.run_config(state, new_url, apply=True)
            except RolloverError:
                if not self.operations.configuration_matches(state, new_url, "direct"):
                    raise
        if not self.operations.configuration_matches(state, new_url, "direct"):
            raise RolloverError("direct_configuration_unconfirmed")
        self._update(
            state,
            "direct_config_applied",
            config_applied_boot_id=self.boot_id_factory(),
            config_applied_monotonic_us=self.monotonic_factory(),
        )
        return self._phase_report(state, deduped=False, credential=evidence)

    def verify_reload(self) -> dict[str, Any]:
        state = self._load_state()
        phase = state_text(state, "phase")
        if phase in {"reload_verified", "private_policy_applied"}:
            return self._phase_report(state, deduped=True)
        if phase != "direct_config_applied":
            raise RolloverError("reload_verification_phase_invalid")
        evidence = self._credential(state)
        if evidence.state != "rotated":
            raise RolloverError("rotated_credential_unconfirmed")
        new_url = state_text(state, "new_url")
        if not self.operations.configuration_matches(state, new_url, "direct"):
            raise RolloverError("direct_configuration_unconfirmed")
        self.operations.verify_runtime_reload(state, new_url)
        self._update(state, "reload_verified")
        return self._phase_report(state, deduped=False, credential=evidence)

    def apply_private_policy(self) -> dict[str, Any]:
        state = self._load_state()
        phase = state_text(state, "phase")
        if phase == "private_policy_applied":
            return self._phase_report(state, deduped=True)
        if phase != "reload_verified":
            raise RolloverError("private_policy_phase_invalid")
        policy = self.operations.apply_private_policy(state)
        self._update(
            state,
            "private_policy_applied",
            policy_digest=policy["policy_digest"],
            policy_version=policy["policy_version"],
        )
        return self._phase_report(state, deduped=False)

    def forward_verify(self) -> dict[str, Any]:
        state = self._load_state()
        if state_text(state, "phase") != "private_policy_applied":
            raise RolloverError("forward_verification_phase_invalid")
        evidence = self._credential(state)
        if evidence.state != "rotated":
            raise RolloverError("rotated_credential_unconfirmed")
        new_url = state_text(state, "new_url")
        if not self.operations.configuration_matches(state, new_url, "direct"):
            raise RolloverError("direct_configuration_unconfirmed")
        self.operations.verify_runtime_reload(state, new_url)
        if not self.operations.policy_matches(state):
            raise RolloverError("private_policy_database_mismatch")
        return self._complete(
            state,
            outcome="password_rollover_forward_completed",
            credential_binding="rotated",
            poster_configuration="direct",
        )

    def rollback(self) -> dict[str, Any]:
        state = self._load_state()
        phase = state_text(state, "phase")
        if phase == "rollback_config_applied":
            return self._phase_report(state, deduped=True)
        evidence = self._credential(state)
        if evidence.state == "ambiguous":
            raise RolloverError("rollback_credential_state_ambiguous")
        binding = evidence.state
        database_url = state_text(
            state, "new_url" if binding == "rotated" else "old_url"
        )
        persistent_binding = self.operations.persistent_database_binding(state)
        if persistent_binding not in {"old", "rotated", "mixed"}:
            raise RolloverError("unexpected_database_configuration_binding")
        if (
            binding == "unrotated"
            and phase in {"escrowed", "preview_validated", "alter_in_flight"}
            and self.operations.configuration_matches(state, database_url, "disabled")
        ):
            return self._complete(
                state,
                outcome="password_rollover_aborted",
                credential_binding="old",
                poster_configuration="disabled",
            )
        if not self.operations.configuration_matches(state, database_url, "direct"):
            self.operations.run_config(state, database_url, apply=True)
        self.operations.run_config(state, None, apply=True)
        if not self.operations.configuration_matches(state, database_url, "disabled"):
            raise RolloverError("rollback_configuration_unconfirmed")
        self._update(
            state,
            "rollback_config_applied",
            rollback_credential_binding=("rotated" if binding == "rotated" else "old"),
            config_applied_boot_id=self.boot_id_factory(),
            config_applied_monotonic_us=self.monotonic_factory(),
        )
        return self._phase_report(state, deduped=False, credential=evidence)

    def rollback_verify(self) -> dict[str, Any]:
        state = self._load_state()
        if state_text(state, "phase") != "rollback_config_applied":
            raise RolloverError("rollback_verification_phase_invalid")
        binding = state_text(state, "rollback_credential_binding")
        evidence = self._credential(state)
        expected_state = "rotated" if binding == "rotated" else "unrotated"
        if evidence.state != expected_state:
            raise RolloverError("rollback_credential_binding_mismatch")
        database_url = state_text(
            state, "new_url" if binding == "rotated" else "old_url"
        )
        if not self.operations.configuration_matches(state, database_url, "disabled"):
            raise RolloverError("rollback_configuration_unconfirmed")
        self.operations.verify_runtime_reload(state, database_url)
        return self._complete(
            state,
            outcome="password_rollover_rollback_completed",
            credential_binding=binding,
            poster_configuration="disabled",
        )

    def status(self) -> dict[str, Any]:
        state = self._load_state()
        return self._phase_report(
            state, deduped=True, credential=self._credential(state)
        )

    def _phase_report(
        self,
        state: dict[str, Any],
        *,
        deduped: bool,
        credential: CredentialEvidence | None = None,
    ) -> dict[str, Any]:
        report: dict[str, Any] = {
            "success": True,
            "action_status": "password_rollover_in_progress",
            "operation_id": self.approved.operation_id,
            "release_sha": self.approved.release_sha,
            "phase": state_text(state, "phase"),
            "reload_required": state_text(state, "phase")
            in {"direct_config_applied", "rollback_config_applied"},
            "terminal_receipt_persisted": False,
            "previous_database_url_sha256_matched": state_text(
                state, "previous_database_url_sha256"
            )
            == self.approved.old_database_url_sha256,
            "successor_database_url_sha256": state_text(
                state, "new_database_url_sha256"
            ),
            "secret_state_present": True,
            "deduped": deduped,
            "feishu_calls_executed": False,
            "service_changes_executed": False,
            "sensitive_values_redacted": True,
        }
        if credential is not None:
            report.update(credential.report())
        return report

    def run(self, command: str) -> dict[str, Any]:
        self.store.cleanup_temporary_records(self.approved.operation_id)
        receipt = self.store.read_receipt(self.approved.operation_id)
        if receipt is not None:
            return self._reconcile_receipt(receipt)
        self.store.assert_no_other_active_state(self.approved.operation_id)
        handlers = {
            "prepare": self.prepare,
            "verify-reload": self.verify_reload,
            "apply-private-policy": self.apply_private_policy,
            "forward-verify": self.forward_verify,
            "rollback": self.rollback,
            "rollback-verify": self.rollback_verify,
            "status": self.status,
        }
        handler = handlers.get(command)
        if handler is None:
            raise RolloverError("rollover_command_invalid")
        return handler()


def emit(report: dict[str, Any]) -> None:
    print(
        "xiaoman_db_password_rollover="
        + json.dumps(report, sort_keys=True, separators=(",", ":")),
        flush=True,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Guard the Xiaoman shared Postgres role password rollover"
    )
    parser.add_argument("--stdin", action="store_true")
    parser.add_argument("--command", choices=sorted(ALLOWED_COMMANDS), required=True)
    parser.add_argument("--approval", default="")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    operation_id = "invalid"
    release_sha = "invalid"
    try:
        if os.geteuid() != 0:
            raise RolloverError("root_required")
        if not args.stdin:
            raise RolloverError("approved_request_stdin_required")
        if args.approval != APPLY_APPROVAL:
            raise RolloverError("exact_owner_approval_required")
        approved = load_approved_request(sys.stdin.buffer.read(MAX_INPUT_BYTES + 1))
        operation_id = approved.operation_id
        release_sha = approved.release_sha
        paths = RuntimePaths(
            release_current=RELEASE_CURRENT_PATH,
            sidecar_env=SIDECAR_ENV_PATH,
            hermes_env=HERMES_ENV_PATH,
            erhua_env=ERHUA_ENV_PATH,
            state_root=STATE_ROOT_PATH,
            self_path=Path(__file__),
        )
        config_script, policy_script = verify_release_boundary(
            paths, approved, owner_uid=0
        )
        store = StateStore(paths.state_root, owner_uid=0)
        operations = ProductionOperations(
            paths=paths,
            approved=approved,
            config_script=config_script,
            policy_script=policy_script,
        )
        machine = RolloverMachine(
            approved=approved, store=store, operations=operations
        )
        with store.lock():
            report = machine.run(args.command)
        emit(report)
        return 0
    except RolloverError as exc:
        reason = str(exc) if str(exc) else "rollover_failed"
    except Exception:
        reason = "unexpected_rollover_failure"
    emit(
        {
            "success": False,
            "action_status": "password_rollover_command_failed",
            "operation_id": operation_id,
            "release_sha": release_sha,
            "reason": reason,
            "feishu_calls_executed": False,
            "service_changes_executed": False,
            "sensitive_values_redacted": True,
        }
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
