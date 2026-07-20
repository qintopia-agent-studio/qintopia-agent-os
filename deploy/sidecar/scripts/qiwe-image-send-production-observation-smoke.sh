#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "QiWe image-send production observation skipped: set QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
RELEASE_CURRENT_DIR="${QINTOPIA_RELEASE_CURRENT_DIR:-/home/ubuntu/qintopia-agent-os-releases/current}"
WORKER_SERVICE_NAME="qintopia-agentos-qiwe-image-send-worker.service"
WORKER_TIMER_NAME="qintopia-agentos-qiwe-image-send-worker.timer"
WORKER_PREFLIGHT_NAME="qintopia-agentos-qiwe-image-send-preflight.service"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
EXPECTED_STATE="${QINTOPIA_QIWE_IMAGE_SEND_EXPECTED_STATE:-auto}"

cd "$MONOREPO_ROOT"

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
  EXPECTED_STATE="disabled"
fi
if [[ "$EXPECTED_STATE" != "disabled" ]]; then
  echo "QiWe image-send production observation currently supports only disabled state" >&2
  exit 1
fi
if [[ "$SEND_ENABLED" != "0" ]]; then
  echo "QiWe image-send production send enablement is not approved in this observation boundary" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for QiWe image-send production observation" >&2
  exit 1
fi

for unit in "$WORKER_PREFLIGHT_NAME" "$WORKER_SERVICE_NAME" "$WORKER_TIMER_NAME"; do
  if "$SYSTEMCTL" cat "$unit" >/dev/null 2>&1; then
    echo "QiWe image-send production apply unit is installed but not approved" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-active --quiet "$unit" >/dev/null 2>&1; then
    echo "QiWe image-send production apply unit is active but not approved" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-enabled --quiet "$unit" >/dev/null 2>&1; then
    echo "QiWe image-send production apply unit is enabled but not approved" >&2
    exit 1
  fi
done

echo "qiwe_image_send_production_observation_state=disabled"
echo "qiwe_image_send_production_release_sha=${RELEASE_SHA}"
echo "QiWe image-send production observation passed"
