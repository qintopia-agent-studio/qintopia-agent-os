#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ROLLBACK:-}" != "approved-production-xiaoman-feishu-internal-group-rollback" ]]; then
  echo "Xiaoman Feishu internal-group production rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
RUNUSER_BIN="/usr/sbin/runuser"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-feishu-internal-group-production-observation-smoke.sh"
HERMES_SYSTEMD_USER="ubuntu"
HERMES_SERVICE="hermes-gateway-xiaoman.service"
DIRECT_PREFLIGHT_SERVICE="qintopia-agentos-xiaoman-feishu-poster-preflight.service"
INTAKE_SERVICE="qintopia-agentos-operations-intake.service"
CALLBACK_SERVICE="qintopia-agentos-xiaoman-poster-review-callback.service"
GROUP_DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"

if [[ ! -x "$SYSTEMCTL" || ! -x "$RUNUSER_BIN" || ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "Xiaoman Feishu internal-group production rollback prerequisites are missing" >&2
  exit 1
fi

run_observation() {
  local delivery_state="$1"
  env -i \
    PATH="$PATH" \
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE=1 \
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE=disabled \
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_DELIVERY_EXPECTED_STATE="$delivery_state" \
    "$OBSERVATION_SCRIPT" >/dev/null
}

restart_xiaoman() {
  env -i \
    PATH="$PATH" \
    HOME="/home/${HERMES_SYSTEMD_USER}" \
    USER="$HERMES_SYSTEMD_USER" \
    LOGNAME="$HERMES_SYSTEMD_USER" \
    "$RUNUSER_BIN" -l "$HERMES_SYSTEMD_USER" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user restart ${HERMES_SERVICE}"
  env -i \
    PATH="$PATH" \
    HOME="/home/${HERMES_SYSTEMD_USER}" \
    USER="$HERMES_SYSTEMD_USER" \
    LOGNAME="$HERMES_SYSTEMD_USER" \
    "$RUNUSER_BIN" -l "$HERMES_SYSTEMD_USER" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user is-active --quiet ${HERMES_SERVICE}"
}

"$SYSTEMCTL" disable --now "$GROUP_DELIVERY_TIMER"
run_observation stopped
"$SYSTEMCTL" start "$DIRECT_PREFLIGHT_SERVICE"
restart_xiaoman
"$SYSTEMCTL" restart "$INTAKE_SERVICE"
"$SYSTEMCTL" restart "$CALLBACK_SERVICE"
"$SYSTEMCTL" is-active --quiet "$INTAKE_SERVICE"
"$SYSTEMCTL" is-active --quiet "$CALLBACK_SERVICE"

if ! run_observation stopped; then
  "$SYSTEMCTL" disable --now "$GROUP_DELIVERY_TIMER" >/dev/null 2>&1 || true
  echo "Xiaoman Feishu internal-group rollback failed final observation; group delivery remains stopped" >&2
  exit 1
fi

echo "Xiaoman Feishu internal-group disabled; direct poster services remain active"
