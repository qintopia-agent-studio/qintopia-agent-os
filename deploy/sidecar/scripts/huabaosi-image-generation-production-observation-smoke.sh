#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Huabaosi image generation production observation skipped: set QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1 to inspect disabled runtime state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
PROVIDER_SERVICE_NAME="qintopia-agentos-huabaosi-image-generation-worker.service"
PROVIDER_TIMER_NAME="qintopia-agentos-huabaosi-image-generation-worker.timer"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"

cd "$MONOREPO_ROOT"

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi

source_env_if_present() {
  if [[ -f "$ENV_FILE" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$ENV_FILE"
    set +a
  fi
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

  local value_name
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_HUABAOSI_IMAGE_API_KEY \
    QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL \
    QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT \
    QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL \
    QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS \
    QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN \
    QIWE_TOKEN \
    QIWE_GUID; do
    if [[ -n "${!value_name:-}" ]]; then
      forbidden+=("${!value_name}")
    fi
  done

  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

assert_generation_disabled() {
  local generation_flag="${QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED:-0}"
  generation_flag="${generation_flag//[[:space:]]/}"
  if [[ "$generation_flag" == "1" ]]; then
    echo "Huabaosi image generation must remain disabled during production observation" >&2
    exit 1
  fi
}

assert_provider_unscheduled() {
  if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
    echo "systemctl is required for Huabaosi image generation production observation" >&2
    exit 1
  fi

  local unit
  for unit in "$PROVIDER_SERVICE_NAME" "$PROVIDER_TIMER_NAME"; do
    if "$SYSTEMCTL" cat "$unit" >/dev/null 2>&1; then
      echo "Huabaosi provider worker unit must not be installed" >&2
      exit 1
    fi
    if "$SYSTEMCTL" is-active --quiet "$unit" >/dev/null 2>&1; then
      echo "Huabaosi provider worker unit must not be active" >&2
      exit 1
    fi
    if "$SYSTEMCTL" is-enabled --quiet "$unit" >/dev/null 2>&1; then
      echo "Huabaosi provider worker unit must not be enabled" >&2
      exit 1
    fi
  done
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

source_env_if_present
assert_generation_disabled
assert_provider_unscheduled

preflight="$tmp_dir/preflight.json"
preflight_stderr="$tmp_dir/preflight.stderr"
set +e
"${BIN_CMD[@]}" huabaosi-image-generation-preflight >"$preflight" 2>"$preflight_stderr"
preflight_status=$?
set -e
python3 - "$preflight" "$preflight_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)
status = int(sys.argv[2])

assert payload["worker"] == "huabaosi-image-generation-worker"
assert payload["generation_enabled"] is False
assert payload["adapter_compiled"] is False
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
}
assert isinstance(missing, list)
assert len(missing) == len(set(missing))
assert set(missing).issubset(allowed_missing)
if payload["config_valid"]:
    assert payload["success"] is True
    assert payload["action_status"] == "adapter_config_ready"
    assert missing == []
    assert status == 0
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
"${BIN_CMD[@]}" run-huabaosi-image-generation-worker --once --dry-run >"$worker_preview" 2>"$worker_stderr"
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
