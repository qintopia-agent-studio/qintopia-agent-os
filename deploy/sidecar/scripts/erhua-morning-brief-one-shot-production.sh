#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-erhua-morning-brief-one-shot"

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_ONE_SHOT:-}" != "$APPROVAL" ]]; then
  echo "erhua morning brief one-shot requires explicit owner approval" >&2
  exit 1
fi

EXPECTED_RELEASE_SHA="${QINTOPIA_ERHUA_MORNING_BRIEF_ONE_SHOT_RELEASE_SHA:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd -P)"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-erhua-morning-brief.service"
TIMER_NAME="qintopia-agentos-erhua-morning-brief.timer"

if [[ ! "$EXPECTED_RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "QINTOPIA_ERHUA_MORNING_BRIEF_ONE_SHOT_RELEASE_SHA must be a 40-character lowercase hex SHA" >&2
  exit 1
fi
if [[ "${RELEASE_DIR##*/}" != "$EXPECTED_RELEASE_SHA" ]]; then
  echo "erhua morning brief one-shot must run from the reviewed release/current SHA" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "erhua morning brief one-shot requires the persistent sidecar env file" >&2
  exit 1
fi
if [[ ! -x "$SYSTEMCTL" ]]; then
  echo "systemctl is required for erhua morning brief one-shot" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "erhua morning brief one-shot requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "erhua morning brief one-shot requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_present_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "erhua morning brief one-shot requires exactly one ${key}" >&2
    exit 1
  fi
}

require_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED" "1"
require_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL" "approved-production-erhua-morning-brief"
require_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED" "1"
require_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL" "approved-production-erhua-morning-brief-auto-publish"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE" "1"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"
require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"
require_env_line "QINTOPIA_QIWE_TEXT_SEND_ENABLED" "1"
require_env_line "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-text-send"
require_present_env_line "QINTOPIA_SIDECAR_DATABASE_URL"
require_present_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID"
require_present_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID"
require_present_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID"
require_present_env_line "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS"
require_present_env_line "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256"
require_present_env_line "QIWE_API_URL"
require_present_env_line "QIWE_TOKEN"
require_present_env_line "QIWE_GUID"
require_present_env_line "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

service_unit="$tmp_dir/service-unit.txt"
"$SYSTEMCTL" daemon-reload
"$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"
grep -F "ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=${EXPECTED_RELEASE_SHA} " "$service_unit" >/dev/null
grep -F "erhua-morning-brief-worker.sh" "$service_unit" >/dev/null
grep -F "EnvironmentFile=${ENV_FILE}" "$service_unit" >/dev/null

if [[ "$("$SYSTEMCTL" is-enabled "$TIMER_NAME")" != "enabled" ]]; then
  echo "erhua morning brief one-shot requires the production timer to be enabled" >&2
  exit 1
fi
if [[ "$("$SYSTEMCTL" is-active "$TIMER_NAME")" != "active" ]]; then
  echo "erhua morning brief one-shot requires the production timer to be active" >&2
  exit 1
fi

"$SYSTEMCTL" start "$SERVICE_NAME"

echo "erhua morning brief one-shot completed"
