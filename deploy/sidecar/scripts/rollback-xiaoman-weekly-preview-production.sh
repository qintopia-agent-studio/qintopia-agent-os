#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_ROLLBACK:-}" != "approved-production-xiaoman-weekly-preview-rollback" ]]; then
  echo "xiaoman weekly preview rollback requires explicit owner approval" >&2
  exit 1
fi

ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SERVICE_NAME="qintopia-agentos-xiaoman-weekly-preview.service"
TIMER_NAME="qintopia-agentos-xiaoman-weekly-preview.timer"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman weekly preview rollback requires the persistent sidecar env file" >&2
  exit 1
fi
if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman weekly preview rollback" >&2
  exit 1
fi

count="$(grep -Ec "^QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=" "$ENV_FILE" || true)"
if [[ "$count" != "1" ]]; then
  echo "xiaoman weekly preview rollback requires exactly one persistent enablement flag" >&2
  exit 1
fi
if ! grep -Fxq "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0" "$ENV_FILE"; then
  echo "xiaoman weekly preview rollback requires QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$TIMER_NAME"
"$SYSTEMCTL" stop "$SERVICE_NAME" || true
"$SYSTEMCTL" reset-failed "$SERVICE_NAME" || true

echo "xiaoman weekly preview timer rolled back"
