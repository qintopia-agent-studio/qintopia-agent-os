#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import re
import stat
import sys
import tempfile
from pathlib import Path
from urllib.parse import unquote, urlparse


OUTPUT_PREFIX = "staging_runtime_env_render="
DEFAULT_OUTPUT = "/etc/qintopia/message-sidecar-staging.env"
DEFAULT_VALUES = "/etc/qintopia/message-sidecar-staging-values.json"
APPLY_APPROVAL = "approved-staging-runtime-env-provision"

ORDERED_KEYS = [
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_PROVIDER",
    "QINTOPIA_HUABAOSI_IMAGE_MODEL",
    "QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
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
    "QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
]

KEY_SET = set(ORDERED_KEYS)
CONTROL_RE = re.compile(r"[\x00-\x1f\x7f]")
HOST_RE = re.compile(r"^[A-Za-z0-9.-]+(?::[0-9]{1,5})?$")


class ValidationError(Exception):
    pass


def load_json_object(path: Path):
    def reject_duplicates(pairs):
        seen = set()
        result = {}
        for key, value in pairs:
            if key in seen:
                raise ValidationError(f"duplicate key: {key}")
            seen.add(key)
            result[key] = value
        return result

    try:
        with path.open("r", encoding="utf-8") as handle:
            data = json.load(handle, object_pairs_hook=reject_duplicates)
    except json.JSONDecodeError as exc:
        raise ValidationError(f"values file is not valid JSON: {exc}") from exc
    if not isinstance(data, dict):
        raise ValidationError("values file must contain one JSON object")
    return data


def require_exact_keys(data):
    keys = set(data.keys())
    missing = [key for key in ORDERED_KEYS if key not in keys]
    extra = sorted(keys - KEY_SET)
    if missing:
        raise ValidationError(f"missing required keys: {', '.join(missing)}")
    if extra:
        raise ValidationError(f"unsupported keys: {', '.join(extra)}")


def validate_value(key, value):
    if not isinstance(value, str):
        raise ValidationError(f"{key} must be a string")
    if value == "":
        raise ValidationError(f"{key} must not be empty")
    if CONTROL_RE.search(value):
        raise ValidationError(f"{key} contains a control character")
    if value != value.strip():
        raise ValidationError(f"{key} must not have surrounding whitespace")
    if "$(" in value or "`" in value:
        raise ValidationError(f"{key} contains shell command syntax")
    return value


def parse_hosts(key, value):
    hosts = [part.strip() for part in value.split(",") if part.strip()]
    if not hosts:
        raise ValidationError(f"{key} must contain at least one host")
    for host in hosts:
        if not HOST_RE.match(host):
            raise ValidationError(f"{key} contains an invalid host entry")
    return hosts


def require_https_url(key, value):
    parsed = urlparse(value)
    if parsed.scheme != "https" or not parsed.hostname:
        raise ValidationError(f"{key} must be an HTTPS URL with a hostname")
    if parsed.username or parsed.password:
        raise ValidationError(f"{key} must not contain credentials")
    return parsed


def require_sha_env(key, value):
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValidationError(f"{key} must be a 40-character lowercase commit SHA")


def validate_values(data, expected_database_hash):
    require_exact_keys(data)
    values = {key: validate_value(key, data[key]) for key in ORDERED_KEYS}

    if values["QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED"] != "1":
        raise ValidationError("QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED must be 1")
    if values["QINTOPIA_QIWE_IMAGE_SEND_ENABLED"] != "1":
        raise ValidationError("QINTOPIA_QIWE_IMAGE_SEND_ENABLED must be 1")
    if values["QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY"] != "1":
        raise ValidationError("QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY must be 1")
    if values["QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND"] != "feishu-base":
        raise ValidationError("QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND must be feishu-base")
    if values["QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED"] != "1":
        raise ValidationError("QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED must be 1")
    if values["QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL"] != "approved-huabaosi-feishu-artifact-mirror":
        raise ValidationError("QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL must match the reviewed staging phrase")
    if values["QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION"] != "huabaosi-generated-image-v1":
        raise ValidationError("QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION must be huabaosi-generated-image-v1")
    if values["QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH"] != "/home/ubuntu/.hermes/profiles/huabaosi/.env":
        raise ValidationError("QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH must be the reviewed Huabaosi profile path")
    require_sha_env(
        "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA",
        values["QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA"],
    )
    require_sha_env("QINTOPIA_DEPLOYED_COMMIT_SHA", values["QINTOPIA_DEPLOYED_COMMIT_SHA"])
    if (
        values["QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA"]
        != values["QINTOPIA_DEPLOYED_COMMIT_SHA"]
    ):
        raise ValidationError(
            "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA must match QINTOPIA_DEPLOYED_COMMIT_SHA"
        )

    database_url = values["QINTOPIA_SIDECAR_DATABASE_URL"]
    actual_database_hash = hashlib.sha256(database_url.encode("utf-8")).hexdigest()
    if actual_database_hash != expected_database_hash:
        raise ValidationError("staging database URL hash does not match approved hash")
    database_name = unquote(urlparse(database_url).path).lstrip("/").lower()
    if "staging" not in database_name:
        raise ValidationError("staging database name must contain staging")

    api_base = require_https_url(
        "QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL",
        values["QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL"],
    )
    qiwe_api = require_https_url("QIWE_API_URL", values["QIWE_API_URL"])

    qiwe_hosts = parse_hosts(
        "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
        values["QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS"],
    )
    media_hosts = parse_hosts(
        "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
        values["QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS"],
    )
    group_ids = [
        part.strip()
        for part in values["QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS"].split(",")
        if part.strip()
    ]
    if len(group_ids) != 1:
        raise ValidationError("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS must contain exactly one isolated group")

    if qiwe_api.hostname not in [host.split(":")[0] for host in qiwe_hosts]:
        raise ValidationError("QIWE_API_URL host must be present in QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS")
    if values["QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256"] != actual_database_hash:
        raise ValidationError("QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256 must match the approved staging database hash")
    if values["QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS"] != values["QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN"]:
        raise ValidationError("QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS must exactly match the reviewed Base token")
    if (
        values["QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS"]
        != values["QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID"]
    ):
        raise ValidationError(
            "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS must exactly match the reviewed artifact table id"
        )

    try:
        media_max_bytes = int(values["QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES"], 10)
    except ValueError as exc:
        raise ValidationError("QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES must be an integer") from exc
    if media_max_bytes <= 0 or media_max_bytes > 25_000_000:
        raise ValidationError("QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES must be between 1 and 25000000")

    return {
        "values": values,
        "qiwe_host_count": len(qiwe_hosts),
        "media_host_count": len(media_hosts),
        "group_count": len(group_ids),
        "database_url_sha256": actual_database_hash,
        "huabaosi_api_host": api_base.hostname,
    }


def render_env(values):
    lines = [
        "# Generated by render-staging-runtime-env.py from owner-reviewed server-local values.",
        "# Do not commit this file or copy its contents into reports, logs, PRs, or chat.",
    ]
    lines.extend(f"{key}={values[key]}" for key in ORDERED_KEYS)
    return "\n".join(lines) + "\n"


def reject_existing_output(path: Path):
    try:
        os.lstat(path)
    except FileNotFoundError:
        return
    raise ValidationError("output file already exists; remove it through the controlled rollback path before retrying")


def validate_test_output_parent(parent: Path):
    try:
        parent_stat = os.lstat(parent)
    except FileNotFoundError as exc:
        raise ValidationError("output parent directory must already exist") from exc
    if stat.S_ISLNK(parent_stat.st_mode):
        raise ValidationError("output parent directory must not be a symlink")
    if not stat.S_ISDIR(parent_stat.st_mode):
        raise ValidationError("output parent path must be a directory")


def validate_protected_output_boundary(path: Path):
    expected = Path(DEFAULT_OUTPUT)
    if path != expected:
        raise ValidationError(f"non-test apply may write only {DEFAULT_OUTPUT}")

    checked = [Path("/"), Path("/etc"), Path("/etc/qintopia")]
    for component in checked:
        try:
            component_stat = os.lstat(component)
        except FileNotFoundError as exc:
            raise ValidationError(f"protected output path component is missing: {component}") from exc
        if stat.S_ISLNK(component_stat.st_mode):
            raise ValidationError(f"protected output path component must not be a symlink: {component}")
        if not stat.S_ISDIR(component_stat.st_mode):
            raise ValidationError(f"protected output path component must be a directory: {component}")
        if component_stat.st_uid != 0:
            raise ValidationError(f"protected output path component must be root-owned: {component}")
        if component_stat.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
            raise ValidationError(
                f"protected output path component must not be group- or world-writable: {component}"
            )


def validate_output_boundary(path: Path, test_mode: bool):
    if not path.is_absolute() or "staging" not in str(path):
        raise ValidationError("output path must be absolute and contain staging")
    reject_existing_output(path)
    if test_mode:
        validate_test_output_parent(path.parent)
        return
    validate_protected_output_boundary(path)


def write_env(path: Path, content: str, test_mode: bool):
    validate_output_boundary(path, test_mode)
    if not test_mode and os.geteuid() != 0:
        raise ValidationError("apply requires root")

    parent = path.parent
    fd, tmp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(parent), text=True)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(tmp_name, 0o600)
        os.replace(tmp_name, path)
    except Exception:
        try:
            os.unlink(tmp_name)
        except FileNotFoundError:
            pass
        raise


def emit(payload):
    print(OUTPUT_PREFIX + json.dumps(payload, sort_keys=True, separators=(",", ":")))


def main():
    parser = argparse.ArgumentParser(
        description="Validate and render the fixed staging sidecar env file without printing secrets."
    )
    parser.add_argument(
        "--values",
        default=DEFAULT_VALUES,
        help=f"server-local JSON values file, default: {DEFAULT_VALUES}",
    )
    parser.add_argument("--expected-database-url-sha256", required=True)
    parser.add_argument("--output", default=DEFAULT_OUTPUT)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval", default="")
    parser.add_argument("--test-mode", action="store_true")
    args = parser.parse_args()

    if not re.fullmatch(r"[0-9a-f]{64}", args.expected_database_url_sha256):
        raise ValidationError("--expected-database-url-sha256 must be a canonical SHA-256")

    data = load_json_object(Path(args.values))
    validated = validate_values(data, args.expected_database_url_sha256)
    content = render_env(validated["values"])
    output = Path(args.output)

    action_status = "staging_env_render_ready"
    if args.apply:
        if args.approval != APPLY_APPROVAL:
            raise ValidationError(f"--approval must be {APPLY_APPROVAL} for apply")
        write_env(output, content, args.test_mode)
        action_status = "staging_env_written"

    emit(
        {
            "success": True,
            "worker": "staging-runtime-env-render",
            "action_status": action_status,
            "apply_requested": args.apply,
            "output_path": str(output),
            "key_count": len(ORDERED_KEYS),
            "database_url_sha256": validated["database_url_sha256"],
            "qiwe_host_count": validated["qiwe_host_count"],
            "media_host_count": validated["media_host_count"],
            "isolated_group_count": validated["group_count"],
            "safe_for_review": True,
            "guardrails": [
                "server-local values file is never printed",
                "only reviewed staging env keys are rendered",
                "staging database URL is represented only by sha256",
                "output mode is 0600 on apply",
                "no provider, media, Postgres, Feishu, QiWe, service, timer, or release action",
            ],
        }
    )


if __name__ == "__main__":
    try:
        main()
    except ValidationError as exc:
        emit(
            {
                "success": False,
                "worker": "staging-runtime-env-render",
                "action_status": "validation_failed",
                "safe_for_review": True,
                "error": str(exc),
            }
        )
        sys.exit(1)
