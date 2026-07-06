#!/usr/bin/env bash
set -euo pipefail

restart_targets=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --restart-targets)
      restart_targets="${2:-}"
      shift 2
      ;;
    -h | --help)
      echo "Usage: deploy/runner/smoke-release.sh --restart-targets <comma-separated-targets>"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

IFS=',' read -r -a targets <<<"$restart_targets"
hermes_systemd_user="${QINTOPIA_HERMES_SYSTEMD_USER:-ubuntu}"

system_services=(
  qintopia-message-sidecar.service
  qintopia-message-embedding-worker.service
  qintopia-message-identity-worker.service
  qintopia-agentos-raw-archive-worker.service
  qintopia-agentos-event-signal-worker.service
  qintopia-agentos-graph-projection-worker.service
  qintopia-agentos-member-profile-worker.service
  qintopia-agentos-daily-digest-worker.service
  qintopia-agentos-daily-digest-publisher.service
)

restart_system_services() {
  systemctl daemon-reload
  for service in "${system_services[@]}"; do
    systemctl restart "$service"
    systemctl is-active --quiet "$service"
  done
}

restart_hermes_service() {
  local service="$1"
  runuser -l "$hermes_systemd_user" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user restart ${service}"
  runuser -l "$hermes_systemd_user" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user is-active --quiet ${service}"
}

for target in "${targets[@]}"; do
  case "$target" in
    qintopia-system-services)
      restart_system_services
      ;;
    hermes-erhua)
      restart_hermes_service hermes-gateway-erhua.service
      ;;
    hermes-wenyuange)
      restart_hermes_service hermes-gateway-wenyuange.service
      ;;
    hermes-xiaoman)
      restart_hermes_service hermes-gateway-xiaoman.service
      ;;
    hermes-silaoshi)
      restart_hermes_service hermes-gateway-silaoshi.service
      ;;
    hermes-huabaosi)
      restart_hermes_service hermes-gateway-huabaosi.service
      ;;
    hermes-guanerye)
      restart_hermes_service hermes-gateway-guanerye.service
      ;;
    "")
      ;;
    *)
      echo "unsupported restart target: ${target}" >&2
      exit 2
      ;;
  esac
done

echo "Smoke checks passed for restart targets: ${restart_targets}"
