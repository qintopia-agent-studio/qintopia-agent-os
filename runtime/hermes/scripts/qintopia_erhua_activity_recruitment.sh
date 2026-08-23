#!/usr/bin/env bash
set -euo pipefail

TASK_NAME="erhua-activity-recruitment"
ENV_FILE="/etc/qintopia/message-sidecar.env"
STATE_DIR="/home/ubuntu/.local/state/qintopia-agentos/${TASK_NAME}"
LOG_FILE="${STATE_DIR}/hermes-cron.log"
RELEASE_LINK="/home/ubuntu/qintopia-agent-os-releases/current"

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
umask 077
mkdir -p "$STATE_DIR"

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a
export PATH="/usr/bin:/bin:/usr/sbin:/sbin"

# Recruitment reuses Erhua's home-group send identity (same resident group as the
# morning brief). Do NOT force the morning-brief / auto-publish enable switches
# here: the worker reads them from the persistent env file and fails closed when
# they are not "1", so an operator can still disable the broadcast as a rollback
# or stop-bleed measure by editing /etc/qintopia/message-sidecar.env.

# The release binding must win over any stale persistent env value; resolve
# release/current once and use that immutable path for both identity and execution.
release_dir="$(cd "$RELEASE_LINK" && pwd -P)"
release_sha="${release_dir##*/}"
if [[ ! "$release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "${TASK_NAME} wrapper could not resolve the release SHA from release/current"
  exit 1
fi
export QINTOPIA_DEPLOYED_COMMIT_SHA="$release_sha"
WORKER="${release_dir}/deploy/sidecar/scripts/erhua-activity-recruitment-worker.sh"

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
