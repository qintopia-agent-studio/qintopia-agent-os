#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-xiaoman-daily-case-report-auto-publish-backfill"

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_BACKFILL:-}" != "$APPROVAL" ]]; then
  echo "xiaoman daily case report backfill requires explicit owner approval" >&2
  exit 1
fi

EXPECTED_RELEASE_SHA="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_RELEASE_SHA:-}"
BACKFILL_DATE="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd -P)"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.service"

if [[ ! "$EXPECTED_RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_RELEASE_SHA must be a 40-character lowercase hex SHA" >&2
  exit 1
fi
if [[ ! "$BACKFILL_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE must be YYYY-MM-DD" >&2
  exit 1
fi
if [[ "${RELEASE_DIR##*/}" != "$EXPECTED_RELEASE_SHA" ]]; then
  echo "xiaoman daily case report backfill must run from the reviewed release/current SHA" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman daily case report backfill requires the persistent sidecar env file" >&2
  exit 1
fi
if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman daily case report backfill" >&2
  exit 1
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "python3 is required for xiaoman daily case report backfill" >&2
  exit 1
fi

"$PYTHON_BIN" - "$BACKFILL_DATE" <<'PY'
from __future__ import annotations

import sys
from datetime import datetime
from zoneinfo import ZoneInfo

try:
    requested = datetime.strptime(sys.argv[1], "%Y-%m-%d").date()
except ValueError as exc:
    raise SystemExit("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE is not a real calendar date") from exc

today = datetime.now(ZoneInfo("Asia/Shanghai")).date()
if requested > today:
    raise SystemExit("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE must not be in the future")
PY

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report backfill requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "xiaoman daily case report backfill requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_present_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report backfill requires exactly one ${key}" >&2
    exit 1
  fi
}

require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "1"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL" "approved-production-xiaoman-daily-case-report-auto-publish"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE" "1"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID"

tmp_dir="$(mktemp -d)"
cleanup() {
  "$SYSTEMCTL" unset-environment \
    QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE \
    QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL >/dev/null 2>&1 || true
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

service_unit="$tmp_dir/service-unit.txt"
"$SYSTEMCTL" daemon-reload
"$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"
grep -F "ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=${EXPECTED_RELEASE_SHA} " "$service_unit" >/dev/null
grep -F "xiaoman-daily-case-report-auto-publish-worker.sh" "$service_unit" >/dev/null
grep -F "EnvironmentFile=${ENV_FILE}" "$service_unit" >/dev/null

"$SYSTEMCTL" set-environment \
  "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE=${BACKFILL_DATE}" \
  "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL=${APPROVAL}"
"$SYSTEMCTL" start "$SERVICE_NAME"

echo "xiaoman daily case report backfill started for ${BACKFILL_DATE}"
