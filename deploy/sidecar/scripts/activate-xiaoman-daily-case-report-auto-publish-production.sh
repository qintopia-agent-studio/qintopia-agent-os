#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ACTIVATION:-}" != "approved-production-xiaoman-daily-case-report-auto-publish" ]]; then
  echo "xiaoman daily case report auto-publish production activation requires explicit owner approval" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.service"
TIMER_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer"

if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "xiaoman daily case report activation requires the release-local observation script" >&2
  exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman daily case report activation requires the persistent sidecar env file" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman daily case report activation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report activation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "xiaoman daily case report activation requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_present_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report activation requires exactly one ${key}" >&2
    exit 1
  fi
}

cleanup_failed_activation() {
  "$SYSTEMCTL" disable --now "$TIMER_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" stop "$SERVICE_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" reset-failed "$SERVICE_NAME" >/dev/null 2>&1 || true
}

require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "1"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL" "approved-production-xiaoman-daily-case-report-auto-publish"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE" "1"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_UPLOAD_ENDPOINT"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_ALLOWED_HOSTS"

"$SYSTEMCTL" enable "$TIMER_NAME"
"$SYSTEMCTL" restart "$TIMER_NAME"
if ! "$SYSTEMCTL" is-enabled --quiet "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
if ! "$SYSTEMCTL" is-active --quiet "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
if ! env -i \
  PATH="$PATH" \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE=1 \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE=enabled \
  "$OBSERVATION_SCRIPT" >/dev/null; then
  cleanup_failed_activation
  exit 1
fi

echo "xiaoman daily case report auto-publish production timer enabled"
