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

restart_system=false
for target in "${targets[@]}"; do
  if [[ "$target" == "qintopia-system-services" ]]; then
    restart_system=true
  fi
done

if [[ "$restart_system" == "true" ]]; then
  systemctl daemon-reload
  for service in "${system_services[@]}"; do
    systemctl restart "$service"
    systemctl is-active --quiet "$service"
  done
fi

echo "Smoke checks passed for restart targets: ${restart_targets}"
