#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION:-}" != "approved-production-image-generation" ]]; then
  echo "Huabaosi production activation requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
PREFLIGHT_SERVICE="qintopia-agentos-huabaosi-image-generation-preflight.service"
WORKER_TIMER="qintopia-agentos-huabaosi-image-generation-worker.timer"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi production activation" >&2
  exit 1
fi

"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
"$SYSTEMCTL" enable "$WORKER_TIMER"
"$SYSTEMCTL" restart "$WORKER_TIMER"
"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"
"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"
timer_next_elapse="$("$SYSTEMCTL" show --property=NextElapseUSecMonotonic --value "$WORKER_TIMER")"
if [[ -z "$timer_next_elapse" || "$timer_next_elapse" == "0" || "$timer_next_elapse" == "infinity" ]]; then
  "$SYSTEMCTL" disable --now "$WORKER_TIMER" >/dev/null 2>&1 || true
  echo "Huabaosi image generation production timer has no future trigger" >&2
  exit 1
fi

echo "Huabaosi image generation production timer activated"
