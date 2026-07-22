#!/usr/bin/env bash
set -euo pipefail
PATH="/usr/bin:/bin:/usr/sbin:/sbin"

if [[ "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Huabaosi image generation production observation skipped: set QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
ENV_FILE="/etc/qintopia/message-sidecar.env"
RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
PROVIDER_SERVICE_NAME="qintopia-agentos-huabaosi-image-generation-worker.service"
PROVIDER_TIMER_NAME="qintopia-agentos-huabaosi-image-generation-worker.timer"
PROVIDER_PREFLIGHT_NAME="qintopia-agentos-huabaosi-image-generation-preflight.service"
SYSTEMCTL="/usr/bin/systemctl"
EXPECTED_STATE="${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_EXPECTED_STATE:-auto}"

cd "$MONOREPO_ROOT"

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  SIDECAR_BIN="$QINTOPIA_SIDECAR_BIN"
else
  SIDECAR_BIN="${RELEASE_CURRENT_DIR}/sidecar/qintopia-message-sidecar"
fi

if ! python3 - "$SIDECAR_BIN" "$RELEASE_CURRENT_DIR" <<'PY'
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
PY
then
  echo "Huabaosi image generation production observation requires the immutable release/current sidecar binary with approved features" >&2
  exit 1
fi

CHILD_ENV=(
  "PATH=/usr/bin:/bin"
  "HOME=/nonexistent"
  "PYTHONDONTWRITEBYTECODE=1"
)
SENSITIVE_VALUES=()
GENERATION_ENABLED="0"

add_child_env() {
  CHILD_ENV+=("$1=$2")
}

add_sensitive_value() {
  if [[ -n "$1" ]]; then
    SENSITIVE_VALUES+=("$1")
  fi
}

load_observation_env() {
  if [[ ! -f "$ENV_FILE" ]]; then
    return 0
  fi

  local parsed_env="${tmp_dir}/huabaosi-image-observation-env.nul"
  local key
  local value
  if ! python3 - "$ENV_FILE" >"$parsed_env" <<'PY'
import re
import sys

path = sys.argv[1]
allowed = {
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
    "QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    "QINTOPIA_HUABAOSI_IMAGE_MODEL",
    "QINTOPIA_HUABAOSI_IMAGE_PROVIDER",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
    "QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN",
    "QIWE_GUID",
    "QIWE_TOKEN",
}
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")
seen = set()

with open(path, encoding="utf-8") as fh:
    for lineno, raw in enumerate(fh, 1):
        line = raw.rstrip("\r\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        if not match:
            raise SystemExit(f"invalid Huabaosi image observation env line {lineno}")
        key, value = match.groups()
        if key not in allowed:
            continue
        if key in seen:
            raise SystemExit(f"duplicate Huabaosi image observation env key {key}")
        seen.add(key)
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        if key == "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED" and value not in {"0", "1"}:
            raise SystemExit(f"invalid Huabaosi image observation env value for {key}")
        sys.stdout.buffer.write(key.encode())
        sys.stdout.buffer.write(b"\0")
        sys.stdout.buffer.write(value.encode())
        sys.stdout.buffer.write(b"\0")
PY
  then
    echo "Huabaosi image generation production observation env is invalid" >&2
    exit 1
  fi

  while IFS= read -r -d '' key && IFS= read -r -d '' value; do
    add_child_env "$key" "$value"
    case "$key" in
      QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED)
        GENERATION_ENABLED="$value"
        ;;
      QINTOPIA_SIDECAR_DATABASE_URL|QINTOPIA_HUABAOSI_IMAGE_API_KEY|QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL|QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT|QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL|QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS|QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN|QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS|QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID|QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS|QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH|QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN|QIWE_TOKEN|QIWE_GUID)
        add_sensitive_value "$value"
        ;;
    esac
  done <"$parsed_env"

}

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "QINTOPIA_SIDECAR_DATABASE_URL=postgres://"
    "provider_endpoint"
    "media_upload_endpoint"
    "artifact_uri"
    "--use-feishu-base"
    "send_executed=true"
    "message_id"
    "raw_chat"
    "base_token"
  )

  forbidden+=("${SENSITIVE_VALUES[@]}")

  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

assert_generation_state() {
  local generation_flag="$GENERATION_ENABLED"
  generation_flag="${generation_flag//[[:space:]]/}"
  if [[ "$EXPECTED_STATE" == "auto" ]]; then
    if [[ "$generation_flag" == "1" ]]; then
      EXPECTED_STATE="enabled"
    else
      EXPECTED_STATE="disabled"
    fi
  fi
  if [[ "$EXPECTED_STATE" != "disabled" && "$EXPECTED_STATE" != "enabled" ]]; then
    echo "Huabaosi production expected state must be disabled, enabled, or auto" >&2
    exit 1
  fi
  if [[ "$EXPECTED_STATE" == "enabled" && "$generation_flag" != "1" ]]; then
    echo "Huabaosi image generation enablement does not match expected state" >&2
    exit 1
  fi
  if [[ "$EXPECTED_STATE" == "disabled" && "$generation_flag" == "1" ]]; then
    echo "Huabaosi image generation disablement does not match expected state" >&2
    exit 1
  fi
}

assert_provider_state() {
  if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
    echo "systemctl is required for Huabaosi image generation production observation" >&2
    exit 1
  fi

  if [[ "$EXPECTED_STATE" == "enabled" ]]; then
    local unit
    for unit in "$PROVIDER_PREFLIGHT_NAME" "$PROVIDER_SERVICE_NAME" "$PROVIDER_TIMER_NAME"; do
      if ! "$SYSTEMCTL" cat "$unit" >/dev/null 2>&1; then
        echo "Huabaosi production unit is missing" >&2
        exit 1
      fi
    done
    if ! "$SYSTEMCTL" is-active --quiet "$PROVIDER_TIMER_NAME" >/dev/null 2>&1; then
      echo "Huabaosi provider timer must be active" >&2
      exit 1
    fi
    if ! "$SYSTEMCTL" is-enabled --quiet "$PROVIDER_TIMER_NAME" >/dev/null 2>&1; then
      echo "Huabaosi provider timer must be enabled" >&2
      exit 1
    fi
  else
    if "$SYSTEMCTL" is-active --quiet "$PROVIDER_TIMER_NAME" >/dev/null 2>&1; then
      echo "Huabaosi provider timer must not be active" >&2
      exit 1
    fi
    if "$SYSTEMCTL" is-enabled --quiet "$PROVIDER_TIMER_NAME" >/dev/null 2>&1; then
      echo "Huabaosi provider timer must not be enabled" >&2
      exit 1
    fi
  fi
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

load_observation_env
assert_generation_state
assert_provider_state

preflight="$tmp_dir/preflight.json"
preflight_stderr="$tmp_dir/preflight.stderr"
set +e
env -i "${CHILD_ENV[@]}" "$SIDECAR_BIN" huabaosi-image-generation-preflight >"$preflight" 2>"$preflight_stderr"
preflight_status=$?
set -e
python3 - "$preflight" "$preflight_status" "$EXPECTED_STATE" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)
status = int(sys.argv[2])
expected_state = sys.argv[3]

assert payload["worker"] == "huabaosi-image-generation-worker"
assert payload["generation_enabled"] is (expected_state == "enabled")
assert payload["safe_for_chat"] is False
assert isinstance(payload["config_valid"], bool)
assert isinstance(payload["media_allowed_host_count"], int)
missing = payload["missing_configuration"]
allowed_missing = {
    "QINTOPIA_HUABAOSI_IMAGE_PROVIDER",
    "QINTOPIA_HUABAOSI_IMAGE_MODEL",
    "QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
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
}
assert isinstance(missing, list)
assert len(missing) == len(set(missing))
assert set(missing).issubset(allowed_missing)
if expected_state == "enabled":
    assert payload["adapter_compiled"] is True
    assert payload.get("adapter_mode") == "production"
    assert payload["config_valid"] is True
    assert payload["success"] is True
    assert payload["action_status"] == "adapter_config_ready"
    assert missing == []
    assert status == 0
elif payload["config_valid"] and not payload["adapter_compiled"]:
    assert payload["success"] is True
    assert payload["action_status"] == "adapter_config_ready"
    assert status == 0
elif payload["config_valid"]:
    assert payload.get("adapter_mode") == "production"
    assert payload["success"] is False
    assert payload["action_status"] == "live_adapter_compiled_requires_owner_review"
    assert status != 0
else:
    assert payload["success"] is False
    assert payload["action_status"] == "adapter_not_configured"
    assert status != 0
PY
assert_no_sensitive_output "image adapter preflight" "$preflight"
assert_no_sensitive_output "image adapter preflight stderr" "$preflight_stderr"

worker_preview="$tmp_dir/worker-preview.json"
worker_stderr="$tmp_dir/worker-preview.stderr"
set +e
env -i "${CHILD_ENV[@]}" "$SIDECAR_BIN" run-huabaosi-image-generation-worker --once --dry-run >"$worker_preview" 2>"$worker_stderr"
worker_status=$?
set -e
assert_no_sensitive_output "image worker dry-run" "$worker_preview"
assert_no_sensitive_output "image worker dry-run stderr" "$worker_stderr"
if [[ "$worker_status" != "0" ]]; then
  echo "Huabaosi image worker dry-run failed" >&2
  exit 1
fi
python3 - "$worker_preview" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "huabaosi-image-generation-worker"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["fixture_mode"] is False
assert payload["safe_for_chat"] is False
assert payload["action_status"] in {
    "no_claimable_image_request",
    "image_generation_preview",
}
assert payload["artifact_ids"] == []
preview = payload["artifact_preview"]
if preview is not None:
    assert preview["artifact_type"] == "generated_image"
    assert preview["review_status"] == "pending"
    assert preview["mime_type"] == "image/jpeg"
    assert preview["byte_size"] == 0
PY

echo "Huabaosi image generation production observation passed"
