#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-xiaoman-daily-case-report-auto-publish-backfill"

fail() {
  local reason="$1"
  echo "qintopia_runtime_one_shot_safe_failure=xiaoman daily case report backfill: ${reason}" >&2
  echo "xiaoman daily case report backfill failed: ${reason}" >&2
  exit 1
}

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_BACKFILL:-}" != "$APPROVAL" ]]; then
  fail "approval missing"
fi

EXPECTED_RELEASE_SHA="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_RELEASE_SHA:-}"
BACKFILL_DATE="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd -P)"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
WORKER="${RELEASE_DIR}/deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh"

if [[ ! "$EXPECTED_RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  fail "release sha invalid"
fi
if [[ ! "$BACKFILL_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  fail "backfill date invalid"
fi
if [[ "${RELEASE_DIR##*/}" != "$EXPECTED_RELEASE_SHA" ]]; then
  fail "release sha drift"
fi
if [[ ! -f "$ENV_FILE" ]]; then
  fail "persistent env missing"
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  fail "python unavailable"
fi
if [[ ! -x "$WORKER" ]]; then
  fail "worker missing"
fi

"$PYTHON_BIN" - "$BACKFILL_DATE" <<'PY' || fail "backfill date rejected"
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
    fail "env ${key} count invalid"
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    fail "env ${key} value invalid"
  fi
}

require_env_line_any() {
  local key="$1"
  local pattern="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    fail "env ${key} count invalid"
  fi
  if ! grep -Eq "^${key}=(${pattern})$" "$ENV_FILE"; then
    fail "env ${key} value invalid"
  fi
}

require_present_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    fail "env ${key} count invalid"
  fi
}

require_env_line_any "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "0|1"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL" "approved-production-xiaoman-daily-case-report-auto-publish"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE" "1"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a
export PATH="/usr/bin:/bin:/usr/sbin:/sbin"
export QINTOPIA_DEPLOYED_COMMIT_SHA="$EXPECTED_RELEASE_SHA"
export QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA="$EXPECTED_RELEASE_SHA"
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE="$BACKFILL_DATE"
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL="$APPROVAL"

if ! "$WORKER" >"${tmp_dir}/worker-output.txt" 2>&1; then
  fail "worker failed"
fi

echo "xiaoman daily case report backfill completed for ${BACKFILL_DATE}"
