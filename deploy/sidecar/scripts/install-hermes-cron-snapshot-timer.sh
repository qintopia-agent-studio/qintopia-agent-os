#!/usr/bin/env bash
# Install the hermes-cron-snapshot systemd user timer (infra, not an Agent task) and
# run one immediate snapshot so the baseline history exists. Owner approval is
# required; the script writes only the two unit files under the fixed user unit dir.
set -euo pipefail

if [[ "${QINTOPIA_HERMES_CRON_SNAPSHOT:-}" != "approved-production-hermes-cron-snapshot" ]]; then
  echo "Hermes cron snapshot timer install requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin"
SYSTEMCTL="/usr/bin/systemctl"
RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
SYNC_SCRIPT="${RELEASE_DIR}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"
UNIT_DIR="/home/ubuntu/.config/systemd/user"
SERVICE_UNIT="${UNIT_DIR}/hermes-cron-snapshot.service"
TIMER_UNIT="${UNIT_DIR}/hermes-cron-snapshot.timer"

fail() {
  echo "Hermes cron snapshot timer install failed: $1" >&2
  exit 1
}

[[ -x "$SYSTEMCTL" ]] || fail "fixed systemctl is required"
[[ -x "$SYNC_SCRIPT" ]] || fail "sync script is missing from release/current"
[[ "$(readlink -f "$RELEASE_DIR")" != "$RELEASE_DIR" ]] || fail "release/current must be a symlink"

umask 077
mkdir -p "$UNIT_DIR"

cat >"$SERVICE_UNIT" <<EOF
[Unit]
Description=Hermes cron state snapshot (server-local git history)

[Service]
Type=oneshot
ExecStart=${SYNC_SCRIPT}
EOF

cat >"$TIMER_UNIT" <<EOF
[Unit]
Description=Hermes cron state snapshot timer

[Timer]
OnBootSec=2min
OnUnitActiveSec=5min
Persistent=false

[Install]
WantedBy=timers.target
EOF

chmod 0600 "$SERVICE_UNIT" "$TIMER_UNIT"

"$SYSTEMCTL" --user daemon-reload
"$SYSTEMCTL" --user enable --now hermes-cron-snapshot.timer >/dev/null

QINTOPIA_HERMES_CRON_SNAPSHOT="approved-production-hermes-cron-snapshot" "$SYNC_SCRIPT"

echo "hermes-cron-snapshot timer installed and baseline snapshot created"
