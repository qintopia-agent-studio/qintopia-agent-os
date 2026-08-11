#!/usr/bin/env bash
# Hermes no_agent cron wrapper for the Xiaoman Sunday weekly plan confirmation.
# Hermes delivers any stdout to the job origin chat, so the success path stays silent
# and every worker line goes to the server-local log instead.
set -euo pipefail

TASK_NAME="xiaoman-weekly-plan-confirmation"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
ENV_FILE="/etc/qintopia/message-sidecar.env"
STATE_DIR="/home/ubuntu/.local/state/qintopia-agentos/${TASK_NAME}"
LOG_FILE="${STATE_DIR}/hermes-cron.log"

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
umask 077
mkdir -p "$STATE_DIR"

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a
export PATH="/usr/bin:/bin:/usr/sbin:/sbin"

unset QINTOPIA_RELEASE_DIR
unset QINTOPIA_XIAOMAN_WRAPPER_PATH
unset QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PYTHON
unset QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_OUTPUT_DIR

export QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=1

release_dir="$(cd "$RELEASE_CURRENT" && pwd -P)"
release_sha="${release_dir##*/}"
if [[ ! "$release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "${TASK_NAME} wrapper could not resolve the release SHA from release/current"
  exit 1
fi
export QINTOPIA_DEPLOYED_COMMIT_SHA="$release_sha"
WORKER="${release_dir}/deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh"

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
if output="$("$WORKER" 2>&1)"; then
  printf '%s %s run=ok\n' "$started_at" "$TASK_NAME" >>"$LOG_FILE"
  printf '%s\n' "$output" >>"$LOG_FILE"
else
  rc=$?
  printf '%s %s run=failed exit=%s\n' "$started_at" "$TASK_NAME" "$rc" >>"$LOG_FILE"
  printf '%s\n' "$output" >>"$LOG_FILE"
  echo "${TASK_NAME} worker failed (exit=${rc}); evidence in server-local log"
  exit "$rc"
fi
