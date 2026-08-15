#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION:-}" != "approved-production-space-automation-runtime" ]]; then
  echo "Space automation runtime production activation requires explicit owner approval" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/space-automation-runtime-production-observation-smoke.sh"
NATS_ACL_PREFLIGHT="${SCRIPT_DIR}/space-automation-nats-acl-preflight.py"
ENV_FILE="/etc/qintopia/message-sidecar.env"
RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SHA256SUM="/usr/bin/sha256sum"
DISPATCHER_TIMER="qintopia-agentos-automation-dispatcher.timer"
DISPATCHER_SERVICE="qintopia-agentos-automation-dispatcher.service"
EXECUTION_WORKER="qintopia-agentos-space-automation-execution-worker.service"

if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "Space automation runtime production activation requires the release-local observation script" >&2
  exit 1
fi
if [[ ! -x "$NATS_ACL_PREFLIGHT" ]]; then
  echo "Space automation runtime production activation requires the release-local NATS ACL preflight" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "Space automation runtime production activation requires the persistent sidecar env file" >&2
  exit 1
fi
if [[ ! -x "$SYSTEMCTL" ]]; then
  echo "systemctl is required for Space automation runtime production activation" >&2
  exit 1
fi
if [[ ! -x "$SHA256SUM" ]]; then
  echo "sha256sum is required for Space automation runtime production activation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "Space automation runtime production activation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "Space automation runtime production activation requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_sha256_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "Space automation runtime production activation requires exactly one ${key}" >&2
    exit 1
  fi
  count="$(grep -Ec "^${key}=[0-9a-f]{64}$" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "Space automation runtime production activation requires exactly one canonical ${key}" >&2
    exit 1
  fi
}

env_line_value() {
  local key="$1"
  local value
  if ! value="$(python3 - "$ENV_FILE" "$key" <<'PY'
import re
import sys

path, expected_key = sys.argv[1:3]
assignment = re.compile(
    r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$"
)
seen = False

with open(path, encoding="utf-8") as fh:
    for lineno, raw in enumerate(fh, 1):
        line = raw.rstrip("\r\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        if not match:
            raise SystemExit(f"invalid env line {lineno}")
        candidate_key, value = match.groups()
        if candidate_key != expected_key:
            continue
        if seen:
            raise SystemExit(f"duplicate env key {expected_key}")
        seen = True
        value = value.strip()
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        has_control = any(ord(ch) < 32 or ord(ch) == 127 for ch in value)
        if "$" + "(" in value or chr(96) in value or has_control:
            raise SystemExit(f"unsafe env value for {expected_key}")
        print(value)

if not seen:
    raise SystemExit(f"missing env key {expected_key}")
PY
)"; then
    echo "Space automation runtime production activation requires exactly one ${key}" >&2
    exit 1
  fi
  printf '%s' "$value"
}

require_database_hash_match() {
  local expected_hash
  local database_url
  local actual_hash
  expected_hash="$(env_line_value "QINTOPIA_SPACE_AUTOMATION_EXECUTION_DATABASE_URL_SHA256")"
  database_url="$(env_line_value "QINTOPIA_SIDECAR_DATABASE_URL")"
  actual_hash="$(printf '%s' "$database_url" | "$SHA256SUM")"
  actual_hash="${actual_hash%% *}"
  if [[ "$actual_hash" != "$expected_hash" ]]; then
    echo "Space automation runtime production activation database URL hash does not match the approved production hash" >&2
    exit 1
  fi
}

require_qiwe_production_artifact() {
  if ! python3 - "$RELEASE_CURRENT_DIR" <<'PY'
import json
import os
import re
import stat
import sys

current_path = sys.argv[1]
if not os.path.isabs(current_path) or not os.path.exists(current_path):
    raise SystemExit(1)
current_real = os.path.realpath(current_path)
release_sha = os.path.basename(current_real)
if not re.fullmatch(r"[0-9a-f]{40}", release_sha):
    raise SystemExit(1)

primary_bin = os.path.join(current_real, "sidecar", "qintopia-message-sidecar")
companion_dir = os.path.join(current_real, "sidecar-profiles", "qiwe-production")
companion_bin = os.path.join(companion_dir, "qintopia-message-sidecar")
manifest_path = os.path.join(companion_dir, "artifact-manifest.json")
for binary in (primary_bin, companion_bin):
    if os.path.islink(binary) or not os.path.isfile(binary) or not os.access(binary, os.X_OK):
        raise SystemExit(1)
if os.path.islink(manifest_path) or not os.path.isfile(manifest_path):
    raise SystemExit(1)
for candidate in (
    current_real,
    os.path.join(current_real, "sidecar"),
    primary_bin,
    os.path.join(current_real, "sidecar-profiles"),
    companion_dir,
    companion_bin,
    manifest_path,
):
    if os.stat(candidate).st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)

with open(manifest_path, encoding="utf-8") as fh:
    manifest = json.load(fh)
if manifest.get("commit_sha") != release_sha:
    raise SystemExit(1)
validation = manifest.get("validation", {})
if validation.get("artifact_profile") != "qiwe-production":
    raise SystemExit(1)
if validation.get("cargo_features") != [
    "qiwe-production-adapter",
    "huabaosi-feishu-mirror-adapter",
]:
    raise SystemExit(1)
PY
  then
    echo "Space automation runtime production activation requires the reviewed Qiwe companion artifact" >&2
    exit 1
  fi
}

cleanup_runtime() {
  local cleanup_status=0
  local observed
  local unit
  if ! "$SYSTEMCTL" disable --now "$DISPATCHER_TIMER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if ! "$SYSTEMCTL" disable --now "$EXECUTION_WORKER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if ! "$SYSTEMCTL" stop "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if ! "$SYSTEMCTL" stop "$EXECUTION_WORKER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if ! "$SYSTEMCTL" reset-failed "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if ! "$SYSTEMCTL" reset-failed "$EXECUTION_WORKER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if ! "$SYSTEMCTL" reset-failed "$DISPATCHER_TIMER" >/dev/null 2>&1; then
    cleanup_status=1
  fi

  if "$SYSTEMCTL" is-enabled --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if "$SYSTEMCTL" is-enabled --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if "$SYSTEMCTL" is-active --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
    cleanup_status=1
  fi
  if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
    cleanup_status=1
  fi

  for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
    if ! observed="$("$SYSTEMCTL" show --property=LoadState --value "$unit" 2>/dev/null)" || [[ "$observed" != "loaded" ]]; then
      cleanup_status=1
    fi
  done
  for unit in "$DISPATCHER_TIMER" "$EXECUTION_WORKER"; do
    if ! observed="$("$SYSTEMCTL" show --property=UnitFileState --value "$unit" 2>/dev/null)" || [[ "$observed" != "disabled" ]]; then
      cleanup_status=1
    fi
  done
  for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
    if ! observed="$("$SYSTEMCTL" show --property=ActiveState --value "$unit" 2>/dev/null)" || [[ "$observed" != "inactive" ]]; then
      cleanup_status=1
    fi
  done

  return "$cleanup_status"
}

activate_runtime() {
  "$SYSTEMCTL" enable "$DISPATCHER_TIMER" || return $?
  "$SYSTEMCTL" restart "$DISPATCHER_TIMER" || return $?
  "$SYSTEMCTL" enable "$EXECUTION_WORKER" || return $?
  "$SYSTEMCTL" restart "$EXECUTION_WORKER" || return $?
  "$SYSTEMCTL" is-enabled --quiet "$DISPATCHER_TIMER" || return $?
  "$SYSTEMCTL" is-active --quiet "$DISPATCHER_TIMER" || return $?
  "$SYSTEMCTL" is-enabled --quiet "$EXECUTION_WORKER" || return $?
  "$SYSTEMCTL" is-active --quiet "$EXECUTION_WORKER" || return $?
  local timer_next_elapse
  timer_next_elapse="$("$SYSTEMCTL" show --property=NextElapseUSecMonotonic --value "$DISPATCHER_TIMER")" || return $?
  if [[ -z "$timer_next_elapse" || "$timer_next_elapse" == "0" || "$timer_next_elapse" == "infinity" || "$timer_next_elapse" == "n/a" ]]; then
    echo "Space automation dispatcher timer must expose a scheduled trigger" >&2
    return 1
  fi
  env -i PATH="$PATH" \
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_ENABLE=1 \
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_EXPECTED_STATE=enabled \
    "$OBSERVATION_SCRIPT" >/dev/null || return $?
}

require_env_line "QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED" "1"
require_env_line "QINTOPIA_SPACE_AUTOMATION_EXECUTION_APPROVAL" "approved-production-space-automation-execution"
require_env_line "QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY" "0"
require_env_line "QINTOPIA_SPACE_AUTOMATION_QIWE_ALLOWED_HOSTS" "manager.qiweapi.com"
require_env_line "QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED" "1"
require_env_line "QIWE_NATS_CAPTURE_ENABLED" "1"
require_env_line "QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED" "1"
require_env_line "QIWE_NATS_URL" "nats://127.0.0.1:4222"
require_env_line "QIWE_NATS_AUTH_FILE" "/etc/qintopia/nats/qiwe-adapter.json"
require_env_line "QIWE_NATS_AUTHENTICATED_RAW_SUBJECT" "qintopia.qiwe.raw.authenticated"
require_env_line "QINTOPIA_SIDECAR_NATS_URL" "nats://127.0.0.1:4222"
require_env_line "QINTOPIA_SIDECAR_NATS_AUTH_FILE" "/etc/qintopia/nats/message-sidecar.json"
require_env_line "QINTOPIA_SIDECAR_RAW_SUBJECT" "qintopia.qiwe.raw"
require_env_line "QINTOPIA_SIDECAR_AUTHENTICATED_RAW_SUBJECT" "qintopia.qiwe.raw.authenticated"
require_env_line "QINTOPIA_SIDECAR_MESSAGE_SUBJECT" "qintopia.qiwe.message"
require_env_line "QINTOPIA_SIDECAR_TRUST_AUTHENTICATED_RAW_SUBJECT" "true"
require_env_line "QINTOPIA_SIDECAR_NATS_STREAM" "QINTOPIA_QIWE_MESSAGES"
require_env_line "QINTOPIA_SIDECAR_CONSUMER" "qintopia-message-sidecar"
require_sha256_env_line "QINTOPIA_SPACE_AUTOMATION_EXECUTION_DATABASE_URL_SHA256"
require_database_hash_match
require_qiwe_production_artifact
if ! env -i PATH="$PATH" "$NATS_ACL_PREFLIGHT" >/dev/null; then
  echo "Space automation runtime production activation requires the trusted NATS subject ACL" >&2
  exit 1
fi

activation_status=0
activate_runtime || activation_status=$?
if [[ "$activation_status" != "0" ]]; then
  cleanup_status=0
  cleanup_runtime || cleanup_status=$?
  if [[ "$cleanup_status" != "0" ]]; then
    echo "Space automation runtime production activation failed and shutdown could not be proven" >&2
    exit 1
  fi
  echo "Space automation runtime production activation failed and runtime units were disabled" >&2
  exit "$activation_status"
fi

echo "Space automation runtime production activation passed"
