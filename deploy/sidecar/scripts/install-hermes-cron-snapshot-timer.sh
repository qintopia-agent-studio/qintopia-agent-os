#!/usr/bin/env bash
# Install the hermes-cron-snapshot systemd user timer (infra, not an Agent task) and
# run one immediate snapshot so the baseline history exists. Owner approval is
# required; the script writes only the two unit files under the fixed user unit dir.
set -euo pipefail

if [[ "${QINTOPIA_HERMES_CRON_SNAPSHOT:-}" != "approved-production-hermes-cron-snapshot" ]]; then
  echo "Hermes cron snapshot timer install requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
RUNUSER="/usr/sbin/runuser"
STAT="/usr/bin/stat"
RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
SYNC_SCRIPT="${RELEASE_DIR}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"
UNIT_DIR="/home/ubuntu/.config/systemd/user"
SERVICE_UNIT="${UNIT_DIR}/hermes-cron-snapshot.service"
TIMER_UNIT="${UNIT_DIR}/hermes-cron-snapshot.timer"
HOME_DIR="/home/ubuntu"

fail() {
  printf 'qintopia_runtime_one_shot_safe_failure=hermes cron snapshot install: %s\n' "$1" >&2
  echo "Hermes cron snapshot timer install failed: $1" >&2
  exit 1
}

[[ -x "$SYSTEMCTL" ]] || fail "fixed systemctl is required"
[[ -x "$RUNUSER" ]] || fail "fixed runuser is required"
[[ -x "$STAT" ]] || fail "fixed stat is required"
[[ -x "$SYNC_SCRIPT" ]] || fail "sync script is missing from release/current"
[[ "$(readlink -f "$RELEASE_DIR")" != "$RELEASE_DIR" ]] || fail "release/current must be a symlink"
[[ -d "$HOME_DIR" ]] || fail "ubuntu home directory is missing"

UBUNTU_UID="$("$STAT" -c "%u" "$HOME_DIR")" || fail "ubuntu uid lookup failed"
UBUNTU_GID="$("$STAT" -c "%g" "$HOME_DIR")" || fail "ubuntu gid lookup failed"

systemctl_user() {
  "$RUNUSER" -u ubuntu -- /usr/bin/env -i \
    HOME="$HOME_DIR" \
    XDG_RUNTIME_DIR="/run/user/${UBUNTU_UID}" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${UBUNTU_UID}/bus" \
    PATH="/usr/bin:/bin" \
    "$SYSTEMCTL" --user "$@"
}

run_baseline_sync() {
  QINTOPIA_HERMES_CRON_SNAPSHOT="approved-production-hermes-cron-snapshot" \
    "$SYNC_SCRIPT" >/dev/null 2>&1
}

umask 077
mkdir -p "$UNIT_DIR" || fail "user unit directory create failed"
chown "$UBUNTU_UID:$UBUNTU_GID" "$UNIT_DIR" || fail "user unit directory ownership failed"

cat >"$SERVICE_UNIT" <<EOF || fail "service unit write failed"
[Unit]
Description=Hermes cron state snapshot (server-local git history)

[Service]
Type=oneshot
ExecStart=${SYNC_SCRIPT}
EOF

cat >"$TIMER_UNIT" <<EOF || fail "timer unit write failed"
[Unit]
Description=Hermes cron state snapshot timer

[Timer]
OnBootSec=2min
OnUnitActiveSec=5min
Persistent=false

[Install]
WantedBy=timers.target
EOF

chmod 0600 "$SERVICE_UNIT" "$TIMER_UNIT" || fail "unit mode update failed"
chown "$UBUNTU_UID:$UBUNTU_GID" "$SERVICE_UNIT" "$TIMER_UNIT" || fail "unit ownership update failed"

systemctl_user daemon-reload || fail "user systemd daemon-reload failed"
systemctl_user enable --now hermes-cron-snapshot.timer >/dev/null ||
  fail "user timer enable failed"

run_baseline_sync || fail "baseline snapshot sync failed"

echo "hermes-cron-snapshot timer installed and baseline snapshot created"
