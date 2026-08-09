#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_ROLLBACK:-}" != "approved-production-xiaoman-weekly-plan-confirmation-rollback" ]]; then
  echo "xiaoman weekly plan confirmation rollback requires explicit owner approval" >&2
  exit 1
fi

ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-xiaoman-weekly-plan-confirmation.service"
TIMER_NAME="qintopia-agentos-xiaoman-weekly-plan-confirmation.timer"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman weekly plan confirmation rollback requires the persistent sidecar env file" >&2
  exit 1
fi
if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman weekly plan confirmation rollback" >&2
  exit 1
fi

count="$(grep -Ec "^QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=" "$ENV_FILE" || true)"
if [[ "$count" != "1" ]]; then
  echo "xiaoman weekly plan confirmation rollback requires exactly one persistent enablement flag" >&2
  exit 1
fi
if ! grep -Fxq "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=0" "$ENV_FILE"; then
  echo "xiaoman weekly plan confirmation rollback requires QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=0" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$TIMER_NAME"
"$SYSTEMCTL" stop "$SERVICE_NAME" || true
"$SYSTEMCTL" reset-failed "$SERVICE_NAME" || true

echo "xiaoman weekly plan confirmation timer rolled back"
