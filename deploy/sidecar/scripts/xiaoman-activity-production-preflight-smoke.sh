#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE:-}" != "1" ]]; then
  echo "xiaoman activity production preflight skipped: set QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1 to run read-only production checks" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
CHILD_PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PRIMARY_SIDECAR_BIN=""
QIWE_SIDECAR_BIN=""

if [[ -x "${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar" ]]; then
  PRIMARY_SIDECAR_BIN="${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar"
fi

if [[ -x "${MONOREPO_ROOT}/sidecar-profiles/qiwe-production/qintopia-message-sidecar" ]]; then
  QIWE_SIDECAR_BIN="${MONOREPO_ROOT}/sidecar-profiles/qiwe-production/qintopia-message-sidecar"
fi

run_step() {
  local label="$1"
  local enable_key="$2"
  local script_path="$3"
  local sidecar_bin="$4"
  local child_env=(
    "PATH=${CHILD_PATH}"
    "${enable_key}=1"
  )

  if [[ -n "$sidecar_bin" ]]; then
    child_env+=("QINTOPIA_SIDECAR_BIN=${sidecar_bin}")
  fi

  echo "[xiaoman-preflight] ${label}" >&2
  env -i "${child_env[@]}" "$script_path"
}

run_step \
  "activity signal timer observation" \
  QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/xiaoman-activity-signal-timer-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "Xiaoman legacy Hermes cron observation" \
  QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/xiaoman-legacy-cron-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "activity promotion starter timer observation" \
  QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/xiaoman-activity-promotion-starter-timer-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "operations evidence/visual timer observation" \
  QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/operations-downstream-timers-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "Xiaoman downstream evidence/visual preview" \
  QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/xiaoman-activity-downstream-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "activity image-generation starter timer observation" \
  QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/xiaoman-activity-image-generation-starter-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "Huabaosi image provider disabled-state observation" \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/huabaosi-image-generation-production-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "activity send request starter timer observation" \
  QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/xiaoman-activity-send-request-starter-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "operations group send-ready timer observation" \
  QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/operations-group-send-ready-timer-observation-smoke.sh" \
  "$PRIMARY_SIDECAR_BIN"

run_step \
  "QiWe image-send production observation" \
  QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/qiwe-image-send-production-observation-smoke.sh" \
  "$QIWE_SIDECAR_BIN"

run_step \
  "QiWe image callback bridge production observation" \
  QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE \
  "${SCRIPT_DIR}/qiwe-image-callback-bridge-production-observation-smoke.sh" \
  "$QIWE_SIDECAR_BIN"

echo "xiaoman activity production preflight passed"
