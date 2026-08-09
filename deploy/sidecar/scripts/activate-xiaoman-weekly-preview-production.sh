#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_ACTIVATION:-}" != "approved-production-xiaoman-weekly-preview" ]]; then
  echo "xiaoman weekly preview production activation requires explicit owner approval" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-weekly-preview-production-observation-smoke.sh"
LEGACY_CRON_OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-legacy-cron-observation-smoke.sh"
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
if [[ ! -x "$OBSERVATION_SCRIPT" || ! -x "$LEGACY_CRON_OBSERVATION_SCRIPT" ]]; then
  echo "xiaoman weekly preview activation requires release-local observation scripts" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman weekly preview activation requires the persistent sidecar env file" >&2
  exit 1
fi
if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman weekly preview activation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman weekly preview activation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "xiaoman weekly preview activation requires ${key}=${expected}" >&2
    exit 1
  fi
}

cleanup_failed_activation() {
  "$SYSTEMCTL" disable --now "$TIMER_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" stop "$SERVICE_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" reset-failed "$SERVICE_NAME" >/dev/null 2>&1 || true
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

require_env_line "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED" "1"
require_env_line "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL" "approved-production-xiaoman-weekly-preview"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE" "1"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"

if ! env -i PATH="$PATH" QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 "$LEGACY_CRON_OBSERVATION_SCRIPT" >/dev/null; then
  cleanup_failed_activation
  exit 1
fi

if ! "$SYSTEMCTL" daemon-reload; then
  cleanup_failed_activation
  exit 1
fi
service_unit="$tmp_dir/service-unit.txt"
if ! "$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"; then
  cleanup_failed_activation
  exit 1
fi
if ! grep -F "ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=${EXPECTED_RELEASE_SHA} " "$service_unit" >/dev/null; then
  echo "xiaoman weekly preview activation requires service unit bound to ${EXPECTED_RELEASE_SHA}" >&2
  cleanup_failed_activation
  exit 1
fi
if ! "$SYSTEMCTL" enable "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
if ! "$SYSTEMCTL" restart "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
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
  QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OBSERVATION_ENABLE=1 \
  QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_EXPECTED_STATE=enabled \
  QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_RELEASE_SHA="$EXPECTED_RELEASE_SHA" \
  "$OBSERVATION_SCRIPT" >/dev/null; then
  cleanup_failed_activation
  exit 1
fi

echo "xiaoman weekly preview production timer enabled"
