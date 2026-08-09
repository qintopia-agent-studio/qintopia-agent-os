#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ACTIVATION:-}" != "approved-production-xiaoman-weekly-preview" ]]; then
  echo "xiaoman weekly preview activation requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
SERVICE_NAME="qintopia-agentos-xiaoman-weekly-preview.service"
TIMER_NAME="qintopia-agentos-xiaoman-weekly-preview.timer"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-weekly-preview-timer-observation-smoke.sh"
XIAOMAN_CRON_OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-legacy-cron-observation-smoke.sh"

fail() {
  echo "xiaoman weekly preview activation failed: $1" >&2
  exit 1
}

if [[ ! -x "$SYSTEMCTL" ]]; then
  fail "fixed systemctl is required"
fi
if [[ ! -f "$ENV_FILE" ]]; then
  fail "persistent sidecar env file is required"
fi
for script in "$OBSERVATION_SCRIPT" "$XIAOMAN_CRON_OBSERVATION_SCRIPT"; do
  if [[ ! -x "$script" ]]; then
    fail "release-local observation script is missing"
  fi
done

require_env_value() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^(export[[:space:]]+)?${key}[[:space:]]*=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    fail "requires exactly one ${key}"
  fi
  if ! grep -Eq "^(export[[:space:]]+)?${key}[[:space:]]*=[[:space:]]*['\"]?${expected}['\"]?[[:space:]]*$" "$ENV_FILE"; then
    fail "${key} is not set to the reviewed production value"
  fi
}

cleanup_failed_activation() {
  "$SYSTEMCTL" disable --now "$TIMER_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" stop "$SERVICE_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" reset-failed "$SERVICE_NAME" >/dev/null 2>&1 || true
}

require_env_value "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED" "1"
require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE" "1"
require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"

env -i PATH="$PATH" QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 "$XIAOMAN_CRON_OBSERVATION_SCRIPT" >/dev/null

"$SYSTEMCTL" daemon-reload
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
  QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_TIMER_OBSERVATION_ENABLE=1 \
  QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_TIMER_EXPECTED_STATE=enabled \
  "$OBSERVATION_SCRIPT" >/dev/null; then
  cleanup_failed_activation
  exit 1
fi

echo "xiaoman weekly preview production timer enabled"
