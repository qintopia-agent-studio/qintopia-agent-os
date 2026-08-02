#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ACTIVATION:-}" != "approved-production-xiaoman-feishu-internal-group" ]]; then
  echo "Xiaoman Feishu internal-group production activation requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
RUNUSER_BIN="/usr/sbin/runuser"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-feishu-internal-group-production-observation-smoke.sh"
HERMES_SYSTEMD_USER="ubuntu"
HERMES_SERVICE="hermes-gateway-xiaoman.service"
GROUP_PREFLIGHT_SERVICE="qintopia-agentos-xiaoman-feishu-internal-group-poster-preflight.service"
INTAKE_SERVICE="qintopia-agentos-operations-intake.service"
CALLBACK_SERVICE="qintopia-agentos-xiaoman-poster-review-callback.service"
GROUP_DELIVERY_SERVICE="qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.service"
GROUP_DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"

if [[ ! -x "$SYSTEMCTL" || ! -x "$RUNUSER_BIN" || ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "Xiaoman Feishu internal-group production activation prerequisites are missing" >&2
  exit 1
fi

run_observation() {
  local group_delivery_state="$1"
  env -i \
    PATH="$PATH" \
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE=1 \
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE=enabled \
    QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_DELIVERY_EXPECTED_STATE="$group_delivery_state" \
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

cleanup_failed_activation() {
  "$SYSTEMCTL" disable --now "$GROUP_DELIVERY_TIMER" >/dev/null 2>&1 || true
  "$SYSTEMCTL" stop "$GROUP_DELIVERY_SERVICE" >/dev/null 2>&1 || true
  "$SYSTEMCTL" reset-failed "$GROUP_DELIVERY_SERVICE" >/dev/null 2>&1 || true
}

run_observation stopped
"$SYSTEMCTL" start "$GROUP_PREFLIGHT_SERVICE"
restart_xiaoman
"$SYSTEMCTL" restart "$INTAKE_SERVICE"
"$SYSTEMCTL" restart "$CALLBACK_SERVICE"
"$SYSTEMCTL" is-active --quiet "$INTAKE_SERVICE"
"$SYSTEMCTL" is-active --quiet "$CALLBACK_SERVICE"
if ! "$SYSTEMCTL" enable "$GROUP_DELIVERY_TIMER"; then
  cleanup_failed_activation
  exit 1
fi
if ! "$SYSTEMCTL" restart "$GROUP_DELIVERY_TIMER"; then
  cleanup_failed_activation
  exit 1
fi

if ! run_observation active; then
  cleanup_failed_activation
  echo "Xiaoman Feishu internal-group activation failed final observation; group delivery stopped" >&2
  exit 1
fi

echo "Xiaoman Feishu internal-group intake, thread delivery, and review activated"
