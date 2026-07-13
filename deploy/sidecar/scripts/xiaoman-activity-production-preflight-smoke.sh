#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE:-}" != "1" ]]; then
  echo "xiaoman activity production preflight skipped: set QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1 to run read-only production checks" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

if [[ -z "${QINTOPIA_SIDECAR_BIN:-}" && -x "${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar" ]]; then
  export QINTOPIA_SIDECAR_BIN="${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar"
fi

run_step() {
  local label="$1"
  shift
  echo "[xiaoman-preflight] ${label}" >&2
  "$@"
}

run_step \
  "activity signal timer observation" \
  env QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE=1 \
  "${SCRIPT_DIR}/xiaoman-activity-signal-timer-observation-smoke.sh"

run_step \
  "activity promotion starter timer observation" \
  env QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE=1 \
  "${SCRIPT_DIR}/xiaoman-activity-promotion-starter-timer-observation-smoke.sh"

run_step \
  "operations evidence/visual timer observation" \
  env QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1 \
  "${SCRIPT_DIR}/operations-downstream-timers-observation-smoke.sh"

run_step \
  "Xiaoman downstream evidence/visual preview" \
  env QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1 \
  "${SCRIPT_DIR}/xiaoman-activity-downstream-observation-smoke.sh"

run_step \
  "activity image-generation starter timer observation" \
  env QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE=1 \
  "${SCRIPT_DIR}/xiaoman-activity-image-generation-starter-observation-smoke.sh"

run_step \
  "activity send request starter timer observation" \
  env QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1 \
  "${SCRIPT_DIR}/xiaoman-activity-send-request-starter-observation-smoke.sh"

run_step \
  "operations group send-ready timer observation" \
  env QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE=1 \
  "${SCRIPT_DIR}/operations-group-send-ready-timer-observation-smoke.sh"

echo "xiaoman activity production preflight passed"
