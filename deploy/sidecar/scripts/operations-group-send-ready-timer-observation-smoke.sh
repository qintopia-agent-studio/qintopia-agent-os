#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "operations group send-ready timer observation skipped: set QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
SERVICE_NAME="${QINTOPIA_OPERATIONS_GROUP_SEND_READY_SERVICE_NAME:-qintopia-agentos-operations-group-send-ready.service}"
TIMER_NAME="${QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_NAME:-qintopia-agentos-operations-group-send-ready.timer}"
EXPECTED_INTERVAL="${QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_INTERVAL_EXPECTED:-1min}"
JOURNAL_LINES="${QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_JOURNAL_LINES:-80}"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
JOURNALCTL="${JOURNALCTL:-journalctl}"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for operations group send-ready timer observation" >&2
  exit 1
fi

if ! command -v "$JOURNALCTL" >/dev/null 2>&1; then
  echo "journalctl is required for operations group send-ready timer observation" >&2
  exit 1
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
    "--use-feishu-base"
    "send_executed=true"
    "message_id"
    "raw_chat"
    "base_token"
  )

  local value_name
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN \
    QINTOPIA_DAILY_DIGEST_FEISHU_BASE_TOKEN \
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

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

source_env_if_present

timer_status="$tmp_dir/timer-status.txt"
"$SYSTEMCTL" is-active "$TIMER_NAME" >"$timer_status"
grep -Fx active "$timer_status" >/dev/null

timer_enabled="$tmp_dir/timer-enabled.txt"
"$SYSTEMCTL" is-enabled "$TIMER_NAME" >"$timer_enabled"
grep -E '^(enabled|enabled-runtime|static)$' "$timer_enabled" >/dev/null

service_unit="$tmp_dir/service-unit.txt"
"$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"
grep -E "ExecStart=.*run-group-message-send-worker --once --apply$" "$service_unit" >/dev/null
assert_no_sensitive_output "service unit" "$service_unit"

timer_unit="$tmp_dir/timer-unit.txt"
"$SYSTEMCTL" cat "$TIMER_NAME" >"$timer_unit"
grep -F "OnBootSec=4min" "$timer_unit" >/dev/null
grep -F "OnUnitActiveSec=${EXPECTED_INTERVAL}" "$timer_unit" >/dev/null
grep -F "Unit=${SERVICE_NAME}" "$timer_unit" >/dev/null
assert_no_sensitive_output "timer unit" "$timer_unit"

timer_list="$tmp_dir/list-timers.txt"
"$SYSTEMCTL" list-timers "$TIMER_NAME" --no-pager >"$timer_list"
grep -F "$TIMER_NAME" "$timer_list" >/dev/null
assert_no_sensitive_output "timer list" "$timer_list"

journal="$tmp_dir/journal.txt"
"$JOURNALCTL" -u "$SERVICE_NAME" -n "$JOURNAL_LINES" --no-pager >"$journal" || true
assert_no_sensitive_output "service journal" "$journal"

echo "operations group send-ready timer observation passed"
