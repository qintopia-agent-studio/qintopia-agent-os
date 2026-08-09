#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_ROLLBACK:-}" != "approved-production-erhua-morning-brief-rollback" ]]; then
  echo "Erhua morning brief rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
SERVICE_NAME="qintopia-agentos-erhua-morning-brief.service"
TIMER_NAME="qintopia-agentos-erhua-morning-brief.timer"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/erhua-morning-brief-timer-observation-smoke.sh"

if [[ ! -x "$SYSTEMCTL" ]]; then
  echo "Erhua morning brief rollback requires fixed systemctl" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "Erhua morning brief rollback requires the persistent sidecar env file" >&2
  exit 1
fi
if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "Erhua morning brief rollback requires the release-local observation script" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$TIMER_NAME"
"$SYSTEMCTL" stop "$SERVICE_NAME" || true
"$SYSTEMCTL" reset-failed "$SERVICE_NAME" || true

count="$(grep -Ec "^QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED=" "$ENV_FILE" || true)"
if [[ "$count" != "1" ]]; then
  echo "Erhua morning brief rollback requires exactly one persistent enablement flag" >&2
  exit 1
fi
if ! grep -Fxq "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED=0" "$ENV_FILE"; then
  echo "Erhua morning brief rollback requires QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED=0" >&2
  exit 1
fi

env -i \
  PATH="$PATH" \
  QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1 \
  QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE=disabled \
  "$OBSERVATION_SCRIPT" >/dev/null

echo "Erhua morning brief timer rolled back"
