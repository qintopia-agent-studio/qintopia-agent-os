#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HERMES_CRON_SNAPSHOT_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Hermes cron snapshot observation skipped: set QINTOPIA_HERMES_CRON_SNAPSHOT_OBSERVATION_ENABLE=1" >&2
  exit 0
fi

PATH="/usr/bin:/bin"
export PATH

GIT_BIN="/usr/bin/git"
SNAPSHOT_ROOT="/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot"
UNIT_DIR="/home/ubuntu/.config/systemd/user"
SERVICE_UNIT="${UNIT_DIR}/hermes-cron-snapshot.service"
TIMER_UNIT="${UNIT_DIR}/hermes-cron-snapshot.timer"
SYNC_SCRIPT="/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"

fail() {
  echo "hermes_cron_snapshot_observation_error=$1"
  exit 1
}

[[ -x "$GIT_BIN" ]] || fail "git_unavailable"

for required_file in "$SERVICE_UNIT" "$TIMER_UNIT"; do
  [[ -f "$required_file" && ! -L "$required_file" ]] || fail "unit_missing"
  if [[ "$(wc -c <"$required_file")" -gt 4096 ]]; then
    fail "unit_invalid"
  fi
done

grep -Fx "ExecStart=${SYNC_SCRIPT}" "$SERVICE_UNIT" >/dev/null || fail "service_exec_drift"
grep -Fx "OnUnitActiveSec=5min" "$TIMER_UNIT" >/dev/null || fail "timer_interval_drift"

[[ -d "$SNAPSHOT_ROOT/.git" && ! -L "$SNAPSHOT_ROOT" ]] || fail "repo_missing"

root_mode="$(stat -c '%a' "$SNAPSHOT_ROOT")"
case "$root_mode" in
  700 | 0700) ;;
  *) fail "repo_mode_drift" ;;
esac

if [[ -n "$("$GIT_BIN" -C "$SNAPSHOT_ROOT" remote)" ]]; then
  fail "repo_remote_present"
fi

latest_commit_epoch="$("$GIT_BIN" -C "$SNAPSHOT_ROOT" log -1 --format=%ct 2>/dev/null || true)"
if [[ ! "$latest_commit_epoch" =~ ^[0-9]{9,12}$ ]]; then
  fail "repo_commit_missing"
fi

echo "hermes_cron_snapshot_observation_result=success"
echo "hermes_cron_snapshot_timer_unit_present=true"
echo "hermes_cron_snapshot_service_unit_present=true"
echo "hermes_cron_snapshot_repo_present=true"
echo "hermes_cron_snapshot_remote_absent=true"
echo "hermes_cron_snapshot_latest_commit_epoch=${latest_commit_epoch}"
