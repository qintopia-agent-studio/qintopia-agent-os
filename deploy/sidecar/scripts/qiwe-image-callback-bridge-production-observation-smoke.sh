#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "QiWe image callback bridge production observation skipped: set QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
DEFAULT_RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
DEFAULT_SIDECAR_BIN="/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar"
DEFAULT_HERMES_ENV_FILE="/home/ubuntu/.hermes/profiles/erhua/.env"
DEFAULT_PLUGIN_PATH="/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform"
RELEASE_CURRENT_DIR="${QINTOPIA_RELEASE_CURRENT_DIR:-$DEFAULT_RELEASE_CURRENT_DIR}"
HERMES_ENV_FILE="${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE:-$DEFAULT_HERMES_ENV_FILE}"
PLUGIN_PATH="${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH:-$DEFAULT_PLUGIN_PATH}"
EXPECTED_STATE="${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE:-auto}"
TEST_MODE="${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE:-0}"
TEST_ROOT="${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT:-}"

cd "$MONOREPO_ROOT"

if [[ "$TEST_MODE" != "1" ]]; then
  if [[ "$RELEASE_CURRENT_DIR" != "$DEFAULT_RELEASE_CURRENT_DIR" ]]; then
    echo "QiWe image callback bridge production observation requires the fixed production release/current path" >&2
    exit 1
  fi
  if [[ "$HERMES_ENV_FILE" != "$DEFAULT_HERMES_ENV_FILE" ]]; then
    echo "QiWe image callback bridge production observation requires the fixed Erhua Hermes env file" >&2
    exit 1
  fi
  if [[ "$PLUGIN_PATH" != "$DEFAULT_PLUGIN_PATH" ]]; then
    echo "QiWe image callback bridge production observation requires the fixed Erhua QiWe plugin path" >&2
    exit 1
  fi
else
  if [[ "$TEST_ROOT" != /tmp/* && "$TEST_ROOT" != /private/tmp/* ]]; then
    echo "QiWe image callback bridge production observation test mode requires a /tmp test root" >&2
    exit 1
  fi
  for test_path in "$RELEASE_CURRENT_DIR" "$HERMES_ENV_FILE" "$PLUGIN_PATH"; do
    case "$test_path" in
      "$TEST_ROOT"/*) ;;
      *)
        echo "QiWe image callback bridge production observation test paths must stay under the test root" >&2
        exit 1
        ;;
    esac
  done
fi

if [[ "$RELEASE_CURRENT_DIR" == "$DEFAULT_RELEASE_CURRENT_DIR" ]]; then
  SIDECAR_BIN="$DEFAULT_SIDECAR_BIN"
else
  SIDECAR_BIN="${RELEASE_CURRENT_DIR}/sidecar/qintopia-message-sidecar"
fi
PLUGIN_BRIDGE="${RELEASE_CURRENT_DIR}/skills/qiwe/image_callback_bridge.py"

if ! RELEASE_FACTS="$(python3 - "$RELEASE_CURRENT_DIR" "$SIDECAR_BIN" "$PLUGIN_PATH" "$PLUGIN_BRIDGE" <<'PY'
import hashlib
import json
import os
import re
import stat
import sys

current_path, bin_path, plugin_path, bridge_path = sys.argv[1:5]
if not all(os.path.isabs(path) for path in (current_path, bin_path, plugin_path, bridge_path)):
    raise SystemExit(1)
if not os.path.islink(current_path):
    raise SystemExit(1)

current_real = os.path.realpath(current_path)
release_sha = os.path.basename(current_real)
if not re.fullmatch(r"[0-9a-f]{40}", release_sha):
    raise SystemExit(1)

expected_bin = os.path.join(current_real, "sidecar", "qintopia-message-sidecar")
expected_plugin = os.path.join(current_real, "skills", "qiwe")
expected_bridge = os.path.join(expected_plugin, "image_callback_bridge.py")
if os.path.realpath(bin_path) != expected_bin:
    raise SystemExit(1)
if os.path.realpath(plugin_path) != expected_plugin or not os.path.islink(plugin_path):
    raise SystemExit(1)
if os.path.realpath(bridge_path) != expected_bridge:
    raise SystemExit(1)
if os.path.islink(bin_path) or not os.path.isfile(bin_path) or not os.access(bin_path, os.X_OK):
    raise SystemExit(1)
if os.path.islink(bridge_path) or not os.path.isfile(bridge_path):
    raise SystemExit(1)

for path in (
    current_real,
    os.path.dirname(expected_bin),
    expected_bin,
    expected_plugin,
    expected_bridge,
):
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

digest = hashlib.sha256()
with open(expected_bin, "rb") as fh:
    for chunk in iter(lambda: fh.read(1024 * 1024), b""):
        digest.update(chunk)
print(f"{release_sha} {digest.hexdigest()}")
PY
)"; then
  echo "QiWe image callback bridge production observation requires release/current sidecar and Erhua plugin to resolve to the immutable production release" >&2
  exit 1
fi

RELEASE_SHA="${RELEASE_FACTS%% *}"
SIDECAR_SHA256="${RELEASE_FACTS#* }"

ENV_FACTS="$(python3 - "$HERMES_ENV_FILE" "$SIDECAR_BIN" "$RELEASE_CURRENT_DIR" "$SIDECAR_SHA256" <<'PY'
import hashlib
import json
import re
import sys

path, expected_bin, expected_root, expected_sidecar_sha = sys.argv[1:5]
allowlist = {
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_SHA256",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_TIMEOUT_SECONDS",
}
required_when_enabled = {
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
}
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")
values = {}

if path and not path.startswith("/dev/") and not path.endswith("/missing.env"):
    with open(path, encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\r\n")
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            match = assignment.fullmatch(line)
            if not match:
                if stripped.startswith("QINTOPIA_QIWE_IMAGE_") or stripped.startswith("QIWE_"):
                    raise SystemExit(f"invalid callback bridge env line {lineno}")
                continue
            key, value = match.groups()
            if key not in allowlist:
                continue
            if key in values:
                raise SystemExit(f"duplicate callback bridge env key {key}")
            if (value.startswith('"') and value.endswith('"')) or (
                value.startswith("'") and value.endswith("'")
            ):
                value = value[1:-1]
            if "$(" in value or "`" in value:
                raise SystemExit(f"unsafe callback bridge env value for {key}")
            values[key] = value

enabled = values.get("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED", "0")
if enabled not in {"0", "1"}:
    raise SystemExit("invalid callback bridge enable flag")

if enabled == "1":
    checks = {
        "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE": "production",
        "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN": expected_bin,
        "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT": expected_root,
        "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_SHA256": expected_sidecar_sha,
        "QINTOPIA_QIWE_IMAGE_SEND_ENABLED": "1",
        "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY": "1",
        "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL": "approved-production-qiwe-image-send",
    }
    for key, expected in checks.items():
        if values.get(key) != expected:
            raise SystemExit(f"invalid callback bridge env value for {key}")
    for key in required_when_enabled:
        if not values.get(key):
            raise SystemExit(f"missing callback bridge env value for {key}")
    database_hash = values.get("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256", "")
    if not re.fullmatch(r"[0-9a-f]{64}", database_hash):
        raise SystemExit("invalid callback bridge production database hash")
    actual_database_hash = hashlib.sha256(
        values["QINTOPIA_SIDECAR_DATABASE_URL"].encode("utf-8")
    ).hexdigest()
    if actual_database_hash != database_hash:
        raise SystemExit("callback bridge production database hash does not match runtime database URL")
    timeout = values.get("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_TIMEOUT_SECONDS")
    if timeout is not None:
        try:
            timeout_value = float(timeout)
        except ValueError as exc:
            raise SystemExit("invalid callback bridge timeout") from exc
        if not 1.0 <= timeout_value <= 60.0:
            raise SystemExit("invalid callback bridge timeout")

print(json.dumps({"enabled": enabled == "1"}, sort_keys=True))
PY
)" || {
  echo "QiWe image callback bridge production observation env is invalid" >&2
  exit 1
}

if [[ "$ENV_FACTS" == '{"enabled": true}' ]]; then
  OBSERVED_STATE="enabled"
else
  OBSERVED_STATE="disabled"
fi

if [[ "$EXPECTED_STATE" == "auto" ]]; then
  EXPECTED_STATE="$OBSERVED_STATE"
fi
if [[ "$EXPECTED_STATE" != "enabled" && "$EXPECTED_STATE" != "disabled" ]]; then
  echo "QiWe image callback bridge expected state must be disabled, enabled, or auto" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" != "$OBSERVED_STATE" ]]; then
  echo "QiWe image callback bridge observed state does not match expected state" >&2
  exit 1
fi

echo "qiwe_image_callback_bridge_production_observation_state=${OBSERVED_STATE}"
echo "qiwe_image_callback_bridge_production_release_sha=${RELEASE_SHA}"
echo "QiWe image callback bridge production observation passed"
