#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ROLLBACK:-}" != "approved-production-xiaoman-weekly-preview-rollback" ]]; then
  echo "xiaoman weekly preview rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
SERVICE_NAME="qintopia-agentos-xiaoman-weekly-preview.service"
TIMER_NAME="qintopia-agentos-xiaoman-weekly-preview.timer"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-weekly-preview-timer-observation-smoke.sh"

if [[ ! -x "$SYSTEMCTL" ]]; then
  echo "systemctl is required for xiaoman weekly preview rollback" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman weekly preview rollback requires the persistent sidecar env file" >&2
  exit 1
fi
if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "xiaoman weekly preview rollback requires the release-local observation script" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$TIMER_NAME"
"$SYSTEMCTL" stop "$SERVICE_NAME" || true
"$SYSTEMCTL" reset-failed "$SERVICE_NAME" || true

count="$(grep -Ec "^(export[[:space:]]+)?QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED[[:space:]]*=" "$ENV_FILE" || true)"
if [[ "$count" != "1" ]]; then
  echo "xiaoman weekly preview rollback requires exactly one persistent enablement flag" >&2
  exit 1
fi
if ! grep -Eq "^(export[[:space:]]+)?QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED[[:space:]]*=[[:space:]]*['\"]?0['\"]?[[:space:]]*$" "$ENV_FILE"; then
  echo "xiaoman weekly preview rollback requires QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0" >&2
  exit 1
fi

env -i \
  PATH="$PATH" \
  QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_TIMER_OBSERVATION_ENABLE=1 \
  QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_TIMER_EXPECTED_STATE=disabled \
  "$OBSERVATION_SCRIPT" >/dev/null

echo "xiaoman weekly preview production timer rolled back"
