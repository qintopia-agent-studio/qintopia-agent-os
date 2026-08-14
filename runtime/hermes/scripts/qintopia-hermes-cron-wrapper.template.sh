#!/usr/bin/env bash
# Template for Hermes no_agent cron wrappers. Copy to
# runtime/hermes/scripts/qintopia_<task>.sh, replace __TASK_NAME__ and
# __WORKER_SCRIPT__, and deploy the copy to the profile-local
# /home/ubuntu/.hermes/profiles/<profile>/scripts/ directory through the task's reviewed
# apply script. Hermes delivers any stdout to the job's origin chat, so success paths
# must stay silent; only the failure line below may print.
set -euo pipefail

TASK_NAME="__TASK_NAME__"
WORKER="/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/__WORKER_SCRIPT__"
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
