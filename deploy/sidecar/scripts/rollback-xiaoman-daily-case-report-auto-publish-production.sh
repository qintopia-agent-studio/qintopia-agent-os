#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ROLLBACK:-}" != "approved-production-xiaoman-daily-case-report-auto-publish-rollback" ]]; then
  echo "xiaoman daily case report auto-publish rollback requires explicit owner approval" >&2
  exit 1
fi

ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.service"
TIMER_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman daily case report rollback requires the persistent sidecar env file" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman daily case report rollback" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$TIMER_NAME"
"$SYSTEMCTL" stop "$SERVICE_NAME" || true
"$SYSTEMCTL" reset-failed "$SERVICE_NAME" || true

count="$(grep -Ec "^QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=" "$ENV_FILE" || true)"
if [[ "$count" != "1" ]]; then
  echo "xiaoman daily case report rollback requires exactly one persistent enablement flag" >&2
  exit 1
fi
if ! grep -Fxq "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0" "$ENV_FILE"; then
  echo "xiaoman daily case report rollback requires QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0" >&2
  exit 1
fi

echo "xiaoman daily case report auto-publish timer rolled back"
