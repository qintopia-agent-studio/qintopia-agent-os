#!/usr/bin/env bash
set -euo pipefail

TASK_NAME="xiaoman-weekly-plan-confirmation"
WORKER="/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh"
ENV_FILE="/etc/qintopia/message-sidecar.env"
STATE_DIR="/home/ubuntu/.local/state/qintopia-agentos/${TASK_NAME}"
LOG_FILE="${STATE_DIR}/hermes-cron.log"

umask 077
mkdir -p "$STATE_DIR"

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

if output="$("$WORKER" 2>&1)"; then
  printf '%s\n' "$output" >>"$LOG_FILE"
else
  rc=$?
  printf '%s\n' "$output" >>"$LOG_FILE"
  echo "${TASK_NAME} worker failed (exit=${rc}); evidence in server-local log"
  exit "$rc"
fi
