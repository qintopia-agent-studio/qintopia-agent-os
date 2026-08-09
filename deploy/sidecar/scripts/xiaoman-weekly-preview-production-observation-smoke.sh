#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "xiaoman weekly preview observation skipped: set QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OBSERVATION_ENABLE=1" >&2
  exit 0
fi

EXPECTED_STATE="${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_EXPECTED_STATE:-enabled}"
EXPECTED_RELEASE_SHA="${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_RELEASE_SHA:-}"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-xiaoman-weekly-preview.service"
TIMER_NAME="qintopia-agentos-xiaoman-weekly-preview.timer"

if [[ ! "$EXPECTED_RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_RELEASE_SHA must be a 40-character lowercase hex SHA" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" != "enabled" && "$EXPECTED_STATE" != "disabled" ]]; then
  echo "xiaoman weekly preview expected state must be enabled or disabled" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman weekly preview observation requires the persistent sidecar env file" >&2
  exit 1
fi
if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman weekly preview observation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman weekly preview observation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "xiaoman weekly preview observation requires ${key}=${expected}" >&2
    exit 1
  fi
}

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  require_env_line "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED" "1"
else
  require_env_line "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED" "0"
fi
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE" "1"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

service_unit="$tmp_dir/service-unit.txt"
"$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"
grep -F "xiaoman-weekly-preview-worker.sh" "$service_unit" >/dev/null
grep -F "ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=${EXPECTED_RELEASE_SHA} " "$service_unit" >/dev/null
grep -F "EnvironmentFile=${ENV_FILE}" "$service_unit" >/dev/null

timer_unit="$tmp_dir/timer-unit.txt"
"$SYSTEMCTL" cat "$TIMER_NAME" >"$timer_unit"
grep -F "OnCalendar=Mon *-*-* 09:30:00" "$timer_unit" >/dev/null
grep -F "Unit=${SERVICE_NAME}" "$timer_unit" >/dev/null

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  "$SYSTEMCTL" is-enabled --quiet "$TIMER_NAME"
  "$SYSTEMCTL" is-active --quiet "$TIMER_NAME"
  next_elapse="$("$SYSTEMCTL" show --property=NextElapseUSecRealtime --value "$TIMER_NAME")"
  if [[ -z "$next_elapse" || "$next_elapse" == "n/a" || "$next_elapse" == "0" || "$next_elapse" == "infinity" ]]; then
    echo "xiaoman weekly preview timer has no future realtime trigger" >&2
    exit 1
  fi
else
  if "$SYSTEMCTL" is-enabled --quiet "$TIMER_NAME"; then
    echo "xiaoman weekly preview timer remains enabled" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-active --quiet "$TIMER_NAME"; then
    echo "xiaoman weekly preview timer remains active" >&2
    exit 1
  fi
fi

echo "xiaoman weekly preview observation passed"
