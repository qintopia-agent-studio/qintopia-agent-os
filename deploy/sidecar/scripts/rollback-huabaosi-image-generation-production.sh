#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK:-}" != "approved-production-image-generation-rollback" ]]; then
  echo "Huabaosi production rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
WORKER_SERVICE="qintopia-agentos-huabaosi-image-generation-worker.service"
WORKER_TIMER="qintopia-agentos-huabaosi-image-generation-worker.timer"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi production rollback" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$WORKER_TIMER"
"$SYSTEMCTL" stop "$WORKER_SERVICE" >/dev/null 2>&1 || true
"$SYSTEMCTL" reset-failed "$WORKER_SERVICE" >/dev/null 2>&1 || true

echo "Huabaosi image generation production timer disabled"
