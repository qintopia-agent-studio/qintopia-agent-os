#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "xiaoman daily case report auto-publish observation skipped: set QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE=1" >&2
  exit 0
fi

EXPECTED_STATE="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE:-enabled}"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.service"
TIMER_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer"

if [[ "$EXPECTED_STATE" != "enabled" && "$EXPECTED_STATE" != "disabled" ]]; then
  echo "xiaoman daily case report observation expected state must be enabled or disabled" >&2
  exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman daily case report observation requires the persistent sidecar env file" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman daily case report observation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report observation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "xiaoman daily case report observation requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_present_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report observation requires exactly one ${key}" >&2
    exit 1
  fi
}

env_value() {
  local key="$1"
  grep -E "^${key}=" "$ENV_FILE" | cut -d= -f2-
}

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "1"
  require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL" "approved-production-xiaoman-daily-case-report-auto-publish"
  require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE" "1"
  require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
  require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID"
  require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND"
  case "$(env_value "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND")" in
    feishu-base)
      ;;
    https-public)
      require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_UPLOAD_ENDPOINT"
      require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL"
      require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_ALLOWED_HOSTS"
      ;;
    *)
      echo "xiaoman daily case report observation requires a reviewed storage backend" >&2
      exit 1
      ;;
  esac
else
  require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "0"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

service_unit="$tmp_dir/service-unit.txt"
"$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"
grep -F "xiaoman-daily-case-report-auto-publish-worker.sh" "$service_unit" >/dev/null
grep -F "QINTOPIA_DEPLOYED_COMMIT_SHA=" "$service_unit" >/dev/null
grep -F "EnvironmentFile=${ENV_FILE}" "$service_unit" >/dev/null

timer_unit="$tmp_dir/timer-unit.txt"
"$SYSTEMCTL" cat "$TIMER_NAME" >"$timer_unit"
grep -F "OnCalendar=*-*-* 08:00:00" "$timer_unit" >/dev/null
grep -F "Unit=${SERVICE_NAME}" "$timer_unit" >/dev/null

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  "$SYSTEMCTL" is-enabled --quiet "$TIMER_NAME"
  "$SYSTEMCTL" is-active --quiet "$TIMER_NAME"
  next_elapse="$("$SYSTEMCTL" show --property=NextElapseUSecRealtime --value "$TIMER_NAME")"
  if [[ -z "$next_elapse" || "$next_elapse" == "n/a" ]]; then
    echo "xiaoman daily case report timer has no future realtime trigger" >&2
    exit 1
  fi
else
  if "$SYSTEMCTL" is-enabled --quiet "$TIMER_NAME"; then
    echo "xiaoman daily case report timer remains enabled" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-active --quiet "$TIMER_NAME"; then
    echo "xiaoman daily case report timer remains active" >&2
    exit 1
  fi
fi

echo "xiaoman daily case report auto-publish observation passed"
