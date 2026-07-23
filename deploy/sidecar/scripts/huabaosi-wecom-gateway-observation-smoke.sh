#!/usr/bin/env bash
set -euo pipefail
PATH="/usr/bin:/bin"
export PATH

if [[ "${QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Huabaosi WeCom gateway observation skipped: set QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE=1 to inspect read-only runtime state" >&2
  exit 0
fi

SERVICE_NAME="hermes-gateway-huabaosi.service"
PROFILE_DIR="/home/ubuntu/.hermes/profiles/huabaosi"
PROFILE_CONFIG="${PROFILE_DIR}/config.yaml"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
EXPECTED_DROP_IN_PATH="/home/ubuntu/.config/systemd/user/hermes-gateway-huabaosi.service.d/env.conf"
EXPECTED_ENVIRONMENT_FILE="/home/ubuntu/.hermes/profiles/huabaosi/.env (ignore_errors=no)"
JOURNAL_LINES="160"
JOURNAL_SINCE="30 minutes ago"
SYSTEMCTL="/usr/bin/systemctl"
JOURNALCTL="/usr/bin/journalctl"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi WeCom gateway observation" >&2
  exit 1
fi

if ! command -v "$JOURNALCTL" >/dev/null 2>&1; then
  echo "journalctl is required for Huabaosi WeCom gateway observation" >&2
  exit 1
fi

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "corpsecret"
    "encoding_aes_key"
    "private chat"
    "raw_chat"
    "message_id"
    "media_url"
    "file_url"
    "download_url"
    "prompt:"
    "Traceback (most recent call last)"
    "QIWE_TOKEN"
    "QIWE_GUID"
    "WECOM_TOKEN"
    "WECOM_SECRET"
  )

  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

count_matches() {
  local pattern="$1"
  local file="$2"
  local count
  count="$(grep -E "$pattern" "$file" | wc -l | tr -d '[:space:]' || true)"
  if [[ -z "$count" ]]; then
    count="0"
  fi
  printf '%s\n' "$count"
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

service_status="$tmp_dir/service-status.txt"
"$SYSTEMCTL" --user is-active "$SERVICE_NAME" >"$service_status"
grep -Fx active "$service_status" >/dev/null
assert_no_sensitive_output "service status" "$service_status"

service_properties="$tmp_dir/service-properties.txt"
"$SYSTEMCTL" --user show "$SERVICE_NAME" --property=WorkingDirectory --property=ExecStart --property=DropInPaths --property=EnvironmentFiles >"$service_properties"
grep -Fx "WorkingDirectory=${PROFILE_DIR}" "$service_properties" >/dev/null
grep -E "^ExecStart=.*/home/ubuntu/\\.hermes/hermes-agent/.+ -m hermes_cli\\.main --profile huabaosi gateway run --replace.*$" "$service_properties" >/dev/null
if ! grep -Fx "DropInPaths=${EXPECTED_DROP_IN_PATH}" "$service_properties" >/dev/null; then
  echo "Huabaosi WeCom gateway service does not use the single reviewed environment drop-in" >&2
  exit 1
fi
if ! grep -Fx "EnvironmentFiles=${EXPECTED_ENVIRONMENT_FILE}" "$service_properties" >/dev/null; then
  echo "Huabaosi WeCom gateway service does not require the fixed profile environment file" >&2
  exit 1
fi
assert_no_sensitive_output "service properties" "$service_properties"

if [[ ! -r "$PROFILE_CONFIG" ]]; then
  echo "Huabaosi WeCom profile config is not readable" >&2
  exit 1
fi

busy_mode="$(
  (grep -E '^[[:space:]]*busy_input_mode:[[:space:]]*"?[A-Za-z_-]+"?[[:space:]]*$' "$PROFILE_CONFIG" || true) |
    tail -n 1 |
    sed -E 's/^[[:space:]]*busy_input_mode:[[:space:]]*"?([A-Za-z_-]+)"?[[:space:]]*$/\1/'
)"
if [[ -z "$busy_mode" ]]; then
  echo "Huabaosi WeCom profile config is missing busy_input_mode" >&2
  exit 1
fi
case "$busy_mode" in
  interrupt | queue | ignore | reject) ;;
  *)
    echo "Huabaosi WeCom profile config has an unexpected busy_input_mode" >&2
    exit 1
    ;;
esac

if [[ ! -e "$RELEASE_CURRENT" ]]; then
  echo "release/current is missing for Huabaosi WeCom observation" >&2
  exit 1
fi

journal="$tmp_dir/journal.txt"
"$JOURNALCTL" --user -u "$SERVICE_NAME" --since "$JOURNAL_SINCE" -n "$JOURNAL_LINES" --no-pager -o cat >"$journal" || true
assert_no_sensitive_output "service journal" "$journal"

internal_filter_count="$(count_matches 'internal[- ]process|process filter|filtered internal|skip(ped|ping) internal' "$journal")"
send_fallback_count="$(count_matches 'Send failed: .*plain-text fallback|Fallback send also failed|Response formatting failed' "$journal")"
api_timeout_count="$(count_matches 'API call failed.*Request timed out|Request timed out|request timed out|Timeout sending message to WeCom' "$journal")"

printf 'Huabaosi WeCom gateway observation passed: service=%s busy_input_mode=%s release_current_present=true journal_window=30m internal_filter_count=%s send_fallback_count=%s api_timeout_count=%s\n' \
  "$SERVICE_NAME" \
  "$busy_mode" \
  "$internal_filter_count" \
  "$send_fallback_count" \
  "$api_timeout_count"
