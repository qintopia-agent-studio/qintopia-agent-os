#!/usr/bin/env bash
# Hermes no_agent cron wrapper for the Xiaoman daily case report auto-publish.
# Instantiated from runtime/hermes/scripts/qintopia-hermes-cron-wrapper.template.sh with
# TASK_NAME=xiaoman-daily-case-report and
# WORKER_SCRIPT=xiaoman-daily-case-report-auto-publish-worker.sh.
# Hermes delivers any stdout to the job origin chat, so the success path stays silent
# and every worker line goes to the server-local log instead.
set -euo pipefail

TASK_NAME="xiaoman-daily-case-report"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
WORKER="${RELEASE_CURRENT}/deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh"
ENV_FILE="/etc/qintopia/message-sidecar.env"
STATE_DIR="/home/ubuntu/.local/state/qintopia-agentos/${TASK_NAME}"
LOG_FILE="${STATE_DIR}/hermes-cron.log"

# The worker renders through system Pillow (/usr/bin/python3) and falls back to the
# fixed /usr/bin/psql, so the PATH stays pinned to the system directories only.
export PATH="/usr/bin:/bin"
umask 077
mkdir -p "$STATE_DIR"

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

# The Hermes runner passes the gateway process env through, while the retired systemd
# unit started from a clean environment. Clear the runtime path override group so
# every Hermes run is identical to the release-managed timer run.
unset QINTOPIA_RELEASE_DIR
unset QINTOPIA_XIAOMAN_WRAPPER_PATH
unset QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_DIR

# Enablement moved to the Hermes job's enabled field, so the persistent sidecar flag
# stays 0 for the retired systemd path and the wrapper supplies the worker flag itself.
# The remaining worker gates (production approval and the Xiaoman activity switches)
# deliberately keep coming from the reviewed sidecar env file.
export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1

# The retired systemd unit bound both release SHAs at the exec boundary; derive the
# same value from release/current so the worker's Feishu upload boundary and the
# release-identity checks behave identically. Export after sourcing the persistent
# env so a stale value there cannot override the release binding.
release_sha="$(cd "$RELEASE_CURRENT" && pwd -P)"
release_sha="${release_sha##*/}"
if [[ ! "$release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "${TASK_NAME} wrapper could not resolve the release SHA from release/current"
  exit 1
fi
export QINTOPIA_DEPLOYED_COMMIT_SHA="$release_sha"
export QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA="$release_sha"

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
