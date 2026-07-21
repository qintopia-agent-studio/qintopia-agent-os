#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION:-}" != "approved-production-image-generation" ]]; then
  echo "Huabaosi production activation requires explicit owner approval" >&2
  exit 1
fi

SYSTEMCTL="${SYSTEMCTL:-systemctl}"
PREFLIGHT_SERVICE="qintopia-agentos-huabaosi-image-generation-preflight.service"
WORKER_TIMER="qintopia-agentos-huabaosi-image-generation-worker.timer"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi production activation" >&2
  exit 1
fi

"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
"$SYSTEMCTL" enable --now "$WORKER_TIMER"
"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"
"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"

echo "Huabaosi image generation production timer activated"
