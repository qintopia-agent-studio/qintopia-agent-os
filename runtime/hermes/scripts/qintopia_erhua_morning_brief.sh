#!/usr/bin/env bash
set -euo pipefail

TASK_NAME="erhua-morning-brief"
WORKER="/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/erhua-morning-brief-worker.sh"
ENV_FILE="/etc/qintopia/message-sidecar.env"
STATE_DIR="/home/ubuntu/.local/state/qintopia-agentos/${TASK_NAME}"
LOG_FILE="${STATE_DIR}/hermes-cron.log"
RELEASE_LINK="/home/ubuntu/qintopia-agent-os-releases/current"

umask 077
mkdir -p "$STATE_DIR"

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

# The release binding must win over any stale persistent env value.
export QINTOPIA_DEPLOYED_COMMIT_SHA="$(basename "$(readlink -f "$RELEASE_LINK")")"

if output="$("$WORKER" 2>&1)"; then
  printf '%s\n' "$output" >>"$LOG_FILE"
else
  rc=$?
  printf '%s\n' "$output" >>"$LOG_FILE"
  echo "${TASK_NAME} worker failed (exit=${rc}); evidence in server-local log"
  exit "$rc"
fi
