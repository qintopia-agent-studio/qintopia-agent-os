#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Huabaosi WeCom canary observation skipped: set QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE=1 to inspect disabled canary state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
CANARY_SERVICE_NAME="qintopia-agentos-huabaosi-wecom-canary-gateway.service"
CANARY_TIMER_NAME="qintopia-agentos-huabaosi-wecom-canary-gateway.timer"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"

cd "$MONOREPO_ROOT"

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "corpsecret"
    "encoding_aes_key"
    "raw_chat"
    "message_text"
    "message_id"
    "chat_id"
    "user_id"
    "bot_id"
    "media_url"
    "file_url"
    "download_url"
    "Traceback (most recent call last)"
    "QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN="
    "WECOM_TOKEN"
    "WECOM_SECRET"
  )

  local value_name
  for value_name in \
    QINTOPIA_HUABAOSI_WECOM_CANARY_ENDPOINT \
    QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN \
    QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_BOT_IDS \
    QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_CHAT_IDS \
    QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_USER_IDS \
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

assert_canary_unscheduled() {
  if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
    echo "systemctl is required for Huabaosi WeCom canary observation" >&2
    exit 1
  fi

  local unit
  for unit in "$CANARY_SERVICE_NAME" "$CANARY_TIMER_NAME"; do
    if "$SYSTEMCTL" cat "$unit" >/dev/null 2>&1; then
      echo "Huabaosi WeCom canary unit must not be installed" >&2
      exit 1
    fi
    if "$SYSTEMCTL" is-active --quiet "$unit" >/dev/null 2>&1; then
      echo "Huabaosi WeCom canary unit must not be active" >&2
      exit 1
    fi
    if "$SYSTEMCTL" is-enabled --quiet "$unit" >/dev/null 2>&1; then
      echo "Huabaosi WeCom canary unit must not be enabled" >&2
      exit 1
    fi
  done
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

assert_canary_unscheduled

preflight="$tmp_dir/preflight.json"
preflight_stderr="$tmp_dir/preflight.stderr"
set +e
"${BIN_CMD[@]}" huabaosi-wecom-canary-preflight >"$preflight" 2>"$preflight_stderr"
preflight_status=$?
set -e

assert_no_sensitive_output "Huabaosi WeCom canary preflight" "$preflight"
assert_no_sensitive_output "Huabaosi WeCom canary preflight stderr" "$preflight_stderr"

python3 - "$preflight" "$preflight_status" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)
status = int(sys.argv[2])

assert payload["worker"] == "huabaosi-wecom-canary-gateway"
assert payload["safe_for_chat"] is False
assert payload["protocol"] == "huabaosi_wecom_canary_https_json_v1"
assert payload["canary_enabled"] is False
assert payload["success"] is False
assert status == 0
assert payload["action_status"] in {
    "staging_adapter_not_compiled",
    "canary_configuration_not_approved",
}
assert payload["rollback_command"].startswith(
    "unset QINTOPIA_HUABAOSI_WECOM_CANARY_ENABLED"
)
missing = payload["missing_configuration"]
allowed_missing = {
    "QINTOPIA_HUABAOSI_WECOM_CANARY_ENDPOINT",
    "QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN",
    "QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_BOT_IDS",
    "QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_CHAT_IDS",
    "QINTOPIA_HUABAOSI_WECOM_CANARY_ALLOWED_USER_IDS",
}
assert isinstance(missing, list)
assert len(missing) == len(set(missing))
assert set(missing).issubset(allowed_missing)
for key in ["allowed_bot_count", "allowed_chat_count", "allowed_user_count"]:
    assert isinstance(payload[key], int)
PY

echo "Huabaosi WeCom canary observation passed"
