#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK:-}" != "approved-production-qiwe-image-callback-bridge-rollback" ]]; then
  echo "QiWe image callback bridge production rollback requires explicit owner approval" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/qiwe-image-callback-bridge-production-observation-smoke.sh"
RUNUSER_BIN="/usr/sbin/runuser"
HERMES_SYSTEMD_USER="ubuntu"
HERMES_SERVICE="hermes-gateway-erhua.service"

if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "QiWe image callback bridge production rollback requires the release-local observation script" >&2
  exit 1
fi
if [[ ! -x "$RUNUSER_BIN" ]]; then
  echo "runuser is required for QiWe image callback bridge production rollback" >&2
  exit 1
fi

run_observation() {
  local expected_state="$1"
  env -i \
    PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1 \
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE="$expected_state" \
    "$OBSERVATION_SCRIPT" >/dev/null
}

restart_erhua() {
  env -i \
    PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    HOME="/home/${HERMES_SYSTEMD_USER}" \
    USER="$HERMES_SYSTEMD_USER" \
    LOGNAME="$HERMES_SYSTEMD_USER" \
    "$RUNUSER_BIN" -l "$HERMES_SYSTEMD_USER" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user restart ${HERMES_SERVICE}"
  env -i \
    PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    HOME="/home/${HERMES_SYSTEMD_USER}" \
    USER="$HERMES_SYSTEMD_USER" \
    LOGNAME="$HERMES_SYSTEMD_USER" \
    "$RUNUSER_BIN" -l "$HERMES_SYSTEMD_USER" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user is-active --quiet ${HERMES_SERVICE}"
}

run_observation disabled
restart_erhua
run_observation disabled

echo "QiWe image callback bridge production rolled back"
