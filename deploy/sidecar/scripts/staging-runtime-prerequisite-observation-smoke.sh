#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "staging runtime prerequisite observation skipped: set QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE=1 to inspect fixed staging runtime inputs" >&2
  exit 0
fi

TEST_MODE="${QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_TEST_MODE:-0}"
if [[ "$TEST_MODE" != "0" && "$TEST_MODE" != "1" ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_TEST_MODE must be 0 or 1" >&2
  exit 1
fi

ENV_FILE="/etc/qintopia/message-sidecar-staging.env"
RELEASE_ROOT="/home/ubuntu/qintopia-agent-os-staging-releases"
if [[ "$TEST_MODE" == "1" ]]; then
  ENV_FILE="${QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENV_FILE:-$ENV_FILE}"
  RELEASE_ROOT="${QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_RELEASE_ROOT:-$RELEASE_ROOT}"
fi

EXPECTED_RELEASE_SHA="${QINTOPIA_STAGING_RUNTIME_PREREQUISITE_RELEASE_SHA:-}"
EXPECTED_SIDECAR_HASH="${QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256:-}"

if [[ -n "$EXPECTED_RELEASE_SHA" && ! "$EXPECTED_RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_RELEASE_SHA must be a 40-character lowercase hex SHA" >&2
  exit 1
fi

if [[ -n "$EXPECTED_SIDECAR_HASH" && ! "$EXPECTED_SIDECAR_HASH" =~ ^[0-9a-f]{64}$ ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256 must be a canonical SHA-256" >&2
  exit 1
fi

OBSERVATION_ENV_FILE="$ENV_FILE" \
OBSERVATION_RELEASE_ROOT="$RELEASE_ROOT" \
OBSERVATION_EXPECTED_RELEASE_SHA="$EXPECTED_RELEASE_SHA" \
OBSERVATION_EXPECTED_SIDECAR_HASH="$EXPECTED_SIDECAR_HASH" \
OBSERVATION_TEST_MODE="$TEST_MODE" \
python3 - <<'PY'
import hashlib
import json
import os
import stat


def add_limitation(report, value):
    if value not in report["limitations"]:
        report["limitations"].append(value)


def path_is_secure(
    path, *, require_regular=False, require_directory=False, reject_owner_writable=False
):
    if not os.path.isabs(path):
        return False, "path_not_absolute"
    if "staging" not in path:
        return False, "path_missing_staging_marker"
    try:
        path_stat = os.lstat(path)
    except FileNotFoundError:
        return False, "path_missing"
    if stat.S_ISLNK(path_stat.st_mode):
        return False, "path_is_symlink"
    if require_regular and not stat.S_ISREG(path_stat.st_mode):
        return False, "path_not_regular_file"
    if require_directory and not stat.S_ISDIR(path_stat.st_mode):
        return False, "path_not_directory"
    writable_mask = stat.S_IWGRP | stat.S_IWOTH
    if reject_owner_writable:
        writable_mask |= stat.S_IWUSR
    if path_stat.st_mode & writable_mask:
        if reject_owner_writable:
            return False, "path_owner_group_or_world_writable"
        return False, "path_group_or_world_writable"
    if path_stat.st_uid not in (0, os.geteuid()):
        return False, "path_unexpected_owner"
    return True, "ok"


def inspect_binary(path):
    for candidate in [os.path.dirname(os.path.dirname(path)), os.path.dirname(path), path]:
        ok, reason = path_is_secure(
            candidate,
            require_directory=candidate != path,
            require_regular=candidate == path,
            reject_owner_writable=True,
        )
        if not ok:
            return False, reason, None
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return True, "ok", digest.hexdigest()


env_file = os.environ["OBSERVATION_ENV_FILE"]
release_root = os.environ["OBSERVATION_RELEASE_ROOT"]
expected_release_sha = os.environ["OBSERVATION_EXPECTED_RELEASE_SHA"]
expected_sidecar_hash = os.environ["OBSERVATION_EXPECTED_SIDECAR_HASH"]

report = {
    "success": True,
    "worker": "staging-runtime-prerequisite-observation",
    "action_status": "not_ready",
    "ready_for_staging": False,
    "test_mode": os.environ["OBSERVATION_TEST_MODE"] == "1",
    "env_file_present": False,
    "env_file_secure": False,
    "release_root_present": False,
    "release_root_secure": False,
    "release_sha": expected_release_sha or None,
    "sidecar_binary_present": False,
    "sidecar_binary_secure": False,
    "sidecar_binary_sha256": None,
    "sidecar_hash_matches": False,
    "safe_for_chat": False,
    "limitations": [],
    "guardrails": [
        "read-only path and metadata check",
        "staging env file contents are not read",
        "sidecar binary is not executed",
        "no Postgres, Huabaosi, Feishu, QiWe, provider, media, service, timer, release, or network action",
    ],
}

env_ok, env_reason = path_is_secure(env_file, require_regular=True)
if env_reason != "path_missing":
    report["env_file_present"] = True
report["env_file_secure"] = env_ok
if not env_ok:
    add_limitation(report, f"env_file_{env_reason}")

root_ok, root_reason = path_is_secure(release_root, require_directory=True)
if root_reason != "path_missing":
    report["release_root_present"] = True
report["release_root_secure"] = root_ok
if not root_ok:
    add_limitation(report, f"release_root_{root_reason}")

if not expected_release_sha:
    add_limitation(report, "release_sha_not_supplied")
else:
    binary_path = os.path.join(
        release_root,
        expected_release_sha,
        "sidecar",
        "qintopia-message-sidecar",
    )
    binary_ok, binary_reason, binary_hash = inspect_binary(binary_path)
    if binary_reason != "path_missing":
        report["sidecar_binary_present"] = True
    report["sidecar_binary_secure"] = binary_ok
    report["sidecar_binary_sha256"] = binary_hash
    if not binary_ok:
        add_limitation(report, f"sidecar_binary_{binary_reason}")
    elif not expected_sidecar_hash:
        add_limitation(report, "sidecar_hash_not_supplied")
    else:
        report["sidecar_hash_matches"] = binary_hash == expected_sidecar_hash
        if not report["sidecar_hash_matches"]:
            add_limitation(report, "sidecar_hash_mismatch")

if (
    report["env_file_secure"]
    and report["release_root_secure"]
    and expected_release_sha
    and report["sidecar_binary_secure"]
    and report["sidecar_hash_matches"]
):
    report["ready_for_staging"] = True
    report["action_status"] = "ready_for_staging_readiness_smokes"

print(
    "staging_runtime_prerequisite_observation="
    + json.dumps(report, sort_keys=True, separators=(",", ":"))
)
PY
