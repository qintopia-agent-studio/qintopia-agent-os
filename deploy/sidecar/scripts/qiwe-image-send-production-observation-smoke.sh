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
    "qiwe-production-adapter",
]:
    raise SystemExit(1)
if manifest.get("commit_sha") != release_sha:
    raise SystemExit(1)

print(release_sha)
PY
)"; then
  echo "QiWe image-send production observation requires the immutable release/current sidecar binary with approved production features" >&2
  exit 1
fi

parse_observation_env() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    return 0
  fi
  python3 - "$path" <<'PY'
import re
import sys

path = sys.argv[1]
allowed = {
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_SIDECAR_DATABASE_MAX_CONNECTIONS",
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
}
values = {}
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
        if key not in allowed:
            continue
        if key in values:
            raise SystemExit(f"duplicate QiWe observation env key {key}")
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        if "\x00" in value or "\n" in value:
            raise SystemExit(f"invalid QiWe observation env value for {key}")
        if key == "QINTOPIA_QIWE_IMAGE_SEND_ENABLED" and value not in {"0", "1"}:
            raise SystemExit(f"invalid QiWe observation env value for {key}")
        values[key] = value

for key, value in values.items():
    print(f"{key}={value}")
PY
}

CHILD_ENV=(
  "PATH=${PATH:-/usr/bin:/bin}"
  "QINTOPIA_DEPLOYED_COMMIT_SHA=$RELEASE_SHA"
)
SEND_ENABLED="0"
OBSERVATION_ENV_OUTPUT="$(parse_observation_env "$ENV_FILE")" || {
  echo "QiWe image-send production observation env is invalid" >&2
  exit 1
}
while IFS= read -r env_line; do
  [[ -n "$env_line" ]] || continue
  CHILD_ENV+=("$env_line")
  if [[ "$env_line" == QINTOPIA_QIWE_IMAGE_SEND_ENABLED=* ]]; then
    SEND_ENABLED="${env_line#QINTOPIA_QIWE_IMAGE_SEND_ENABLED=}"
  fi
done <<<"$OBSERVATION_ENV_OUTPUT"

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
  echo "QiWe image-send enablement does not match expected state" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" == "disabled" && "$SEND_ENABLED" == "1" ]]; then
  echo "QiWe image-send disablement does not match expected state" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for QiWe image-send production observation" >&2
  exit 1
fi

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  for unit in "$WORKER_PREFLIGHT_NAME" "$WORKER_SERVICE_NAME" "$WORKER_TIMER_NAME"; do
    if ! "$SYSTEMCTL" cat "$unit" >/dev/null 2>&1; then
      echo "QiWe image-send production unit is missing" >&2
      exit 1
    fi
  done
  if ! "$SYSTEMCTL" is-active --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer is not active" >&2
    exit 1
  fi
  if ! "$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer is not enabled" >&2
    exit 1
  fi
else
  if "$SYSTEMCTL" is-active --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer is active while send is disabled" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER_NAME" >/dev/null 2>&1; then
    echo "QiWe image-send production timer is enabled while send is disabled" >&2
    exit 1
  fi
fi

run_sidecar_with_observation_env() {
  env -i "${CHILD_ENV[@]}" "$SIDECAR_BIN" "$@"
}

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "file_token"
    "artifact_uri"
    "send_executed=true"
    "message_id"
    "raw_chat"
    "base_token"
    "callback-file-secret"
    "callback-aes-secret"
  )

  local assignment
  for assignment in "${CHILD_ENV[@]}"; do
    case "$assignment" in
      QINTOPIA_SIDECAR_DATABASE_URL=*|QIWE_TOKEN=*|QIWE_GUID=*|QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS=*|QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=*|QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS=*)
        forbidden+=("${assignment#*=}")
        ;;
    esac
  done

  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

preflight="$tmp_dir/preflight.json"
preflight_stderr="$tmp_dir/preflight.stderr"
set +e
run_sidecar_with_observation_env qiwe-image-send-preflight >"$preflight" 2>"$preflight_stderr"
preflight_status=$?
set -e
assert_no_sensitive_output "QiWe image-send preflight" "$preflight"
assert_no_sensitive_output "QiWe image-send preflight stderr" "$preflight_stderr"
python3 - "$preflight" "$preflight_status" "$EXPECTED_STATE" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)
status = int(sys.argv[2])
expected_state = sys.argv[3]

assert payload["worker"] == "qiwe-image-send-adapter"
assert payload["adapter_compiled"] is True
assert payload["production_adapter_compiled"] is True
assert payload["feishu_delivery_bridge_compiled"] is True
assert payload["send_enabled"] is (expected_state == "enabled")
assert payload["safe_for_chat"] is False
assert payload["protocol"] == "qiwe_async_url_upload_then_send_image"
assert isinstance(payload["missing_configuration"], list)
assert isinstance(payload["allowed_host_count"], int)
assert isinstance(payload["media_allowed_host_count"], int)
assert isinstance(payload["allowed_group_count"], int)
if expected_state == "enabled":
    assert payload["success"] is True
    assert payload["config_valid"] is True
    assert payload["webhook_ready"] is True
    assert payload["missing_configuration"] == []
    assert payload["action_status"] == "production_adapter_ready"
    assert status == 0
else:
    assert payload["success"] is True
    assert payload["config_valid"] is True
    assert payload["action_status"] == "production_adapter_disabled"
    assert status == 0
PY

preview="$tmp_dir/preview.json"
preview_stderr="$tmp_dir/preview.stderr"
set +e
run_sidecar_with_observation_env run-qiwe-image-send-worker --once --dry-run >"$preview" 2>"$preview_stderr"
preview_status=$?
set -e
assert_no_sensitive_output "QiWe image-send dry-run preview" "$preview"
assert_no_sensitive_output "QiWe image-send dry-run preview stderr" "$preview_stderr"
if [[ "$preview_status" != "0" ]]; then
  echo "QiWe image-send production dry-run preview failed" >&2
  exit 1
fi
python3 - "$preview" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "qiwe-image-send-adapter"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["phase"] == "upload"
assert payload["action_status"] in {
    "image_upload_preview",
    "no_claimable_send_request",
}
assert payload["external_upload_requested"] is False
assert payload["callback_received"] is False
assert payload["external_send_executed"] is False
assert payload["safe_for_chat"] is False
PY

echo "qiwe_image_send_production_observation_state=${EXPECTED_STATE}"
echo "QiWe image-send production observation passed"
