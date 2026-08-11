#!/usr/bin/env bash
# Hermes no_agent cron wrapper for the Xiaoman Saturday weekly recruitment.
# Instantiated from runtime/hermes/scripts/qintopia-hermes-cron-wrapper.template.sh with
# TASK_NAME=xiaoman-weekly-recruitment and WORKER_SCRIPT=xiaoman-weekly-recruitment-worker.sh.
# Hermes delivers any stdout to the job origin chat, so the success path stays silent
# and every worker line goes to the server-local log instead.
set -euo pipefail

TASK_NAME="xiaoman-weekly-recruitment"
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

# The Hermes runner passes the gateway process env through, while the retired systemd
# unit started from a clean environment. The worker refuses runtime path overrides on
# the mere definition and also honours optional request overrides, so clear both groups
# to keep every Hermes run identical to the release-managed timer run.
unset QINTOPIA_RELEASE_DIR
unset QINTOPIA_XIAOMAN_WRAPPER_PATH
unset QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PYTHON
unset QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OUTPUT_DIR
unset QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_DATE
unset QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OPERATOR_NAME
unset QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_AUDIENCE
unset QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_FORM_LABEL

# Enablement moved to the Hermes job's enabled field, so the persistent sidecar flag
# stays 0 for the retired systemd path and the wrapper supplies the worker flag itself.
# The remaining worker gates (production approval and the Xiaoman activity switches)
# deliberately keep coming from the reviewed sidecar env file.
export QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=1

# The retired systemd unit bound the release SHA at the exec boundary; resolve
# release/current once and use that immutable path for both identity and execution.
release_dir="$(cd "$RELEASE_CURRENT" && pwd -P)"
release_sha="${release_dir##*/}"
if [[ ! "$release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "${TASK_NAME} wrapper could not resolve the release SHA from release/current"
  exit 1
fi
export QINTOPIA_DEPLOYED_COMMIT_SHA="$release_sha"
WORKER="${release_dir}/deploy/sidecar/scripts/xiaoman-weekly-recruitment-worker.sh"

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
