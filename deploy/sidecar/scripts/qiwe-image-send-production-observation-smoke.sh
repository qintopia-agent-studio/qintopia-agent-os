#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "QiWe image-send production observation skipped: set QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
DEFAULT_ENV_FILE="/etc/qintopia/message-sidecar.env"
DEFAULT_RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-$DEFAULT_ENV_FILE}"
RELEASE_CURRENT_DIR="${QINTOPIA_RELEASE_CURRENT_DIR:-$DEFAULT_RELEASE_CURRENT_DIR}"
WORKER_SERVICE_NAME="qintopia-agentos-qiwe-image-send-worker.service"
WORKER_TIMER_NAME="qintopia-agentos-qiwe-image-send-worker.timer"
WORKER_PREFLIGHT_NAME="qintopia-agentos-qiwe-image-send-preflight.service"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
EXPECTED_STATE="${QINTOPIA_QIWE_IMAGE_SEND_EXPECTED_STATE:-auto}"
TEST_MODE="${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_TEST_MODE:-0}"
TEST_ROOT="${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_TEST_ROOT:-}"

cd "$MONOREPO_ROOT"

if [[ "$TEST_MODE" != "1" ]]; then
  if [[ "$ENV_FILE" != "$DEFAULT_ENV_FILE" ]]; then
    echo "QiWe image-send production observation requires the fixed production env file" >&2
    exit 1
  fi
  if [[ "$RELEASE_CURRENT_DIR" != "$DEFAULT_RELEASE_CURRENT_DIR" ]]; then
    echo "QiWe image-send production observation requires the fixed production release/current path" >&2
    exit 1
  fi
  if [[ "$SYSTEMCTL" != "systemctl" ]]; then
    echo "QiWe image-send production observation requires the real systemctl command" >&2
    exit 1
  fi
else
  if [[ "$TEST_ROOT" != /tmp/* && "$TEST_ROOT" != /private/tmp/* ]]; then
    echo "QiWe image-send production observation test mode requires a /tmp test root" >&2
    exit 1
  fi
  case "$ENV_FILE" in
    "$TEST_ROOT"/*) ;;
    *)
      echo "QiWe image-send production observation test env must stay under the test root" >&2
      exit 1
      ;;
  esac
  case "$RELEASE_CURRENT_DIR" in
    "$TEST_ROOT"/*) ;;
    *)
      echo "QiWe image-send production observation test release must stay under the test root" >&2
      exit 1
      ;;
  esac
  case "$SYSTEMCTL" in
    "$TEST_ROOT"/*) ;;
    *)
      echo "QiWe image-send production observation test systemctl must stay under the test root" >&2
      exit 1
      ;;
  esac
fi

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  SIDECAR_BIN="$QINTOPIA_SIDECAR_BIN"
else
  SIDECAR_BIN="${RELEASE_CURRENT_DIR}/sidecar/qintopia-message-sidecar"
fi

if ! RELEASE_SHA="$(python3 - "$SIDECAR_BIN" "$RELEASE_CURRENT_DIR" <<'PY'
import json
import os
import re
import stat
import sys

bin_path, current_path = sys.argv[1:3]
if not os.path.isabs(bin_path) or not os.path.exists(current_path):
    raise SystemExit(1)

current_real = os.path.realpath(current_path)
release_sha = os.path.basename(current_real)
if not re.fullmatch(r"[0-9a-f]{40}", release_sha):
    raise SystemExit(1)

expected_bin = os.path.join(current_real, "sidecar", "qintopia-message-sidecar")
if os.path.realpath(bin_path) != expected_bin:
    raise SystemExit(1)
if os.path.islink(bin_path) or not os.path.isfile(bin_path) or not os.access(bin_path, os.X_OK):
    raise SystemExit(1)

for path in (current_real, os.path.dirname(expected_bin), expected_bin):
    mode = os.stat(path).st_mode
    if mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)

manifest_path = os.path.join(current_real, "sidecar", "artifact-manifest.json")
with open(manifest_path, encoding="utf-8") as fh:
    manifest = json.load(fh)
if manifest.get("validation", {}).get("cargo_features") != [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "qiwe-production-adapter",
]:
    raise SystemExit(1)
if manifest.get("commit_sha") != release_sha:
    raise SystemExit(1)

print(release_sha)
PY
)"; then
  echo "QiWe image-send production observation requires the immutable release/current sidecar binary without QiWe live adapter features" >&2
  exit 1
fi

parse_send_enablement() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    return 0
  fi
  python3 - "$path" <<'PY'
import re
import sys

path = sys.argv[1]
seen = False
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")

with open(path, encoding="utf-8") as fh:
    for lineno, raw in enumerate(fh, 1):
        line = raw.rstrip("\r\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        if not match:
            raise SystemExit(f"invalid QiWe observation env line {lineno}")
        key, value = match.groups()
        if key != "QINTOPIA_QIWE_IMAGE_SEND_ENABLED":
            continue
        if seen:
            raise SystemExit("duplicate QiWe observation env key QINTOPIA_QIWE_IMAGE_SEND_ENABLED")
        seen = True
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        if value not in {"0", "1"}:
            raise SystemExit(f"invalid QiWe observation env value for {key}")
        print(value)
PY
}

SEND_ENABLED="$(parse_send_enablement "$ENV_FILE")" || {
  echo "QiWe image-send production observation env is invalid" >&2
  exit 1
}
SEND_ENABLED="${SEND_ENABLED:-0}"

if [[ "$EXPECTED_STATE" == "auto" ]]; then
  if [[ "$SEND_ENABLED" == "1" ]]; then
    EXPECTED_STATE="enabled"
  else
    EXPECTED_STATE="disabled"
  fi
fi
if [[ "$EXPECTED_STATE" != "disabled" && "$EXPECTED_STATE" != "enabled" ]]; then
  echo "QiWe image-send production expected state must be disabled, enabled, or auto" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" == "enabled" && "$SEND_ENABLED" != "1" ]]; then
  echo "QiWe image-send production enablement does not match expected state" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" == "disabled" && "$SEND_ENABLED" == "1" ]]; then
  echo "QiWe image-send production disablement does not match expected state" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for QiWe image-send production observation" >&2
  exit 1
fi

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  if [[ "$(grep -Ec '^QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=' "$ENV_FILE" || true)" != "1" ]]; then
    echo "QiWe image-send production approval flag is missing or duplicated" >&2
    exit 1
  fi
  if ! grep -Fxq "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send" "$ENV_FILE"; then
    echo "QiWe image-send production approval flag is invalid" >&2
    exit 1
  fi
  if [[ "$(grep -Ec '^QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=' "$ENV_FILE" || true)" != "1" ]]; then
    echo "QiWe image-send production database hash flag is missing or duplicated" >&2
    exit 1
  fi
  if [[ "$(grep -Ec '^QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=[0-9a-f]{64}$' "$ENV_FILE" || true)" != "1" ]]; then
    echo "QiWe image-send production database hash flag is invalid" >&2
    exit 1
  fi
  for unit in "$WORKER_PREFLIGHT_NAME" "$WORKER_SERVICE_NAME" "$WORKER_TIMER_NAME"; do
    if ! "$SYSTEMCTL" cat "$unit" >/dev/null 2>&1; then
      echo "QiWe image-send production unit is missing" >&2
      exit 1
    fi
  done
  if ! "$SYSTEMCTL" is-active --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer must be active" >&2
    exit 1
  fi
  if ! "$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer must be enabled" >&2
    exit 1
  fi
else
  if "$SYSTEMCTL" is-active --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer must not be active" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer must not be enabled" >&2
    exit 1
  fi
fi

echo "qiwe_image_send_production_observation_state=${EXPECTED_STATE}"
echo "qiwe_image_send_production_release_sha=${RELEASE_SHA}"
echo "QiWe image-send production observation passed"
