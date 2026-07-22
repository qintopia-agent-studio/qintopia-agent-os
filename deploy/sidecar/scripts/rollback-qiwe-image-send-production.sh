#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK:-}" != "approved-production-qiwe-image-send-rollback" ]]; then
  echo "QiWe image-send production rollback requires explicit owner approval" >&2
  exit 1
fi

ENV_FILE="/etc/qintopia/message-sidecar.env"
SYSTEMCTL="systemctl"
WORKER_SERVICE="qintopia-agentos-qiwe-image-send-worker.service"
WORKER_TIMER="qintopia-agentos-qiwe-image-send-worker.timer"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "QiWe image-send production rollback requires the persistent sidecar env file" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for QiWe image-send production rollback" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$WORKER_TIMER"
"$SYSTEMCTL" stop "$WORKER_SERVICE" || true
"$SYSTEMCTL" reset-failed "$WORKER_SERVICE" || true

count="$(grep -Ec "^QINTOPIA_QIWE_IMAGE_SEND_ENABLED=" "$ENV_FILE" || true)"
if [[ "$count" != "1" ]]; then
  echo "QiWe image-send production rollback requires exactly one persistent enablement flag" >&2
  exit 1
fi
if ! grep -Fxq "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0" "$ENV_FILE"; then
  echo "QiWe image-send production rollback requires QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0" >&2
  exit 1
fi

echo "QiWe image-send production timer rolled back"
