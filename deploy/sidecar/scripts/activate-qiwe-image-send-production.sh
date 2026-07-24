#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:-}" != "approved-production-qiwe-image-send" ]]; then
  echo "QiWe image-send production activation requires explicit owner approval" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/qiwe-image-send-production-observation-smoke.sh"
ENV_FILE="/etc/qintopia/message-sidecar.env"
RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SHA256SUM="/usr/bin/sha256sum"
PREFLIGHT_SERVICE="qintopia-agentos-qiwe-image-send-preflight.service"
WORKER_TIMER="qintopia-agentos-qiwe-image-send-worker.timer"
WORKER_SERVICE="qintopia-agentos-qiwe-image-send-worker.service"

if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "QiWe image-send production activation requires the release-local observation script" >&2
  exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
  echo "QiWe image-send production activation requires the persistent sidecar env file" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for QiWe image-send production activation" >&2
  exit 1
fi

if [[ ! -x "$SHA256SUM" ]]; then
  echo "sha256sum is required for QiWe image-send production activation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "QiWe image-send production activation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "QiWe image-send production activation requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_sha256_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "QiWe image-send production activation requires exactly one ${key}" >&2
    exit 1
  fi
  count="$(grep -Ec "^${key}=[0-9a-f]{64}$" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "QiWe image-send production activation requires exactly one canonical ${key}" >&2
    exit 1
  fi
}

env_line_value() {
  local key="$1"
  local count
  local line
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "QiWe image-send production activation requires exactly one ${key}" >&2
    exit 1
  fi
  line="$(grep -E "^${key}=" "$ENV_FILE")"
  printf '%s' "${line#*=}"
}

require_database_hash_match() {
  local expected_hash
  local database_url
  local actual_hash
  expected_hash="$(env_line_value "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256")"
  database_url="$(env_line_value "QINTOPIA_SIDECAR_DATABASE_URL")"
  actual_hash="$(printf '%s' "$database_url" | "$SHA256SUM")"
  actual_hash="${actual_hash%% *}"
  if [[ "$actual_hash" != "$expected_hash" ]]; then
    echo "QiWe image-send production activation database URL hash does not match the approved production hash" >&2
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

sidecar_dir = os.path.join(current_real, "sidecar")
sidecar_bin = os.path.join(sidecar_dir, "qintopia-message-sidecar")
manifest_path = os.path.join(sidecar_dir, "artifact-manifest.json")

if not os.path.isfile(sidecar_bin) or not os.access(sidecar_bin, os.X_OK):
    raise SystemExit(1)
if not os.path.isfile(manifest_path):
    raise SystemExit(1)

for candidate in (current_real, sidecar_dir, sidecar_bin, manifest_path):
    mode = os.stat(candidate).st_mode
    if mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)

with open(manifest_path, encoding="utf-8") as fh:
    manifest = json.load(fh)

if manifest.get("commit_sha") != release_sha:
    raise SystemExit(1)
validation = manifest.get("validation", {})
if validation.get("artifact_profile") != "qiwe-production":
    raise SystemExit(1)
if validation.get("cargo_features") != ["qiwe-production-adapter"]:
    raise SystemExit(1)
PY
  then
    echo "QiWe image-send production activation requires a separate reviewed QiWe production artifact" >&2
    exit 1
  fi
}

run_observation() {
  env -i \
    PATH="$PATH" \
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1 \
    QINTOPIA_QIWE_IMAGE_SEND_EXPECTED_STATE=enabled \
    "$OBSERVATION_SCRIPT" >/dev/null
}

cleanup_failed_activation() {
  "$SYSTEMCTL" disable --now "$WORKER_TIMER" >/dev/null 2>&1 || true
  "$SYSTEMCTL" stop "$WORKER_SERVICE" >/dev/null 2>&1 || true
  "$SYSTEMCTL" reset-failed "$WORKER_SERVICE" >/dev/null 2>&1 || true
}

require_env_line "QINTOPIA_QIWE_IMAGE_SEND_ENABLED" "1"
require_env_line "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-image-send"
require_sha256_env_line "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256"
require_database_hash_match
require_qiwe_production_artifact

"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
"$SYSTEMCTL" enable --now "$WORKER_TIMER"
if ! "$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"; then
  cleanup_failed_activation
  exit 1
fi
if ! "$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"; then
  cleanup_failed_activation
  exit 1
fi
if ! run_observation; then
  cleanup_failed_activation
  exit 1
fi

echo "QiWe image-send production timer activated"
