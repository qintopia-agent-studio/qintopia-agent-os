#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_SPACE_AUTOMATION_RUNTIME_ROLLBACK:-}" != "approved-production-space-automation-runtime-rollback" ]]; then
  echo "Space automation runtime production rollback requires explicit owner approval" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/space-automation-runtime-production-observation-smoke.sh"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
DISPATCHER_TIMER="qintopia-agentos-automation-dispatcher.timer"
DISPATCHER_SERVICE="qintopia-agentos-automation-dispatcher.service"
EXECUTION_WORKER="qintopia-agentos-space-automation-execution-worker.service"

if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "Space automation runtime production rollback requires the release-local observation script" >&2
  exit 1
fi
if [[ ! -x "$SYSTEMCTL" ]]; then
  echo "systemctl is required for Space automation runtime production rollback" >&2
  exit 1
fi

shutdown_status=0
observed=""
if ! "$SYSTEMCTL" disable --now "$DISPATCHER_TIMER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" disable --now "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" stop "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" stop "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" reset-failed "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" reset-failed "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" reset-failed "$DISPATCHER_TIMER" >/dev/null 2>&1; then
  shutdown_status=1
fi

if "$SYSTEMCTL" is-enabled --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-enabled --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-active --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
  shutdown_status=1
fi
for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
  if ! observed="$("$SYSTEMCTL" show --property=LoadState --value "$unit" 2>/dev/null)" || [[ "$observed" != "loaded" ]]; then
    shutdown_status=1
  fi
done
for unit in "$DISPATCHER_TIMER" "$EXECUTION_WORKER"; do
  if ! observed="$("$SYSTEMCTL" show --property=UnitFileState --value "$unit" 2>/dev/null)" || [[ "$observed" != "disabled" ]]; then
    shutdown_status=1
  fi
done
for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
  if ! observed="$("$SYSTEMCTL" show --property=ActiveState --value "$unit" 2>/dev/null)" || [[ "$observed" != "inactive" ]]; then
    shutdown_status=1
  fi
done
if [[ "$shutdown_status" != "0" ]]; then
  echo "Space automation runtime production rollback could not prove all runtime units stopped" >&2
  exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
  echo "Space automation runtime production rollback requires the persistent sidecar env file" >&2
  exit 1
fi
count="$(grep -Ec '^QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED=' "$ENV_FILE" || true)"
if [[ "$count" != "1" ]]; then
  echo "Space automation runtime production rollback requires exactly one persistent execution flag" >&2
  exit 1
fi
if ! grep -Fxq "QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED=0" "$ENV_FILE"; then
  echo "Space automation runtime production rollback requires QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED=0" >&2
  exit 1
fi

env -i PATH="$PATH" \
  QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_ENABLE=1 \
  QINTOPIA_SPACE_AUTOMATION_RUNTIME_EXPECTED_STATE=disabled \
  "$OBSERVATION_SCRIPT" >/dev/null

echo "Space automation runtime production rollback passed"
