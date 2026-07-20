#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: install-release-systemd-units.sh --release-root <dir> --release-sha <sha>

Renders the reviewed release-local systemd units, installs the fixed unit allowlist,
and enables only internal AgentOS worker timers.
USAGE
}

release_root=""
release_sha=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-root)
      release_root="${2:-}"
      shift 2
      ;;
    --release-sha)
      release_sha="${2:-}"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$release_root" || -z "$release_sha" ]]; then
  usage >&2
  exit 2
fi

release_dir="$(python3 - "${release_root}/${release_sha}" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
if path.exists() or path.is_symlink():
    print(path.resolve())
PY
)"
if [[ -z "$release_dir" ]]; then
  echo "release directory is missing: ${release_root}/${release_sha}" >&2
  exit 1
fi

current_target="$(python3 - "${release_root}/current" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
if path.exists() or path.is_symlink():
    print(path.resolve())
PY
)"
if [[ "$current_target" != "$release_dir" ]]; then
  echo "release is not the current target: ${release_dir}" >&2
  exit 1
fi

render_script="${release_dir}/deploy/sidecar/scripts/render-systemd-units.sh"
if [[ ! -x "$render_script" ]]; then
  echo "release systemd renderer is missing or not executable: ${render_script}" >&2
  exit 1
fi

systemctl_bin="${SYSTEMCTL:-systemctl}"
unit_dir="${QINTOPIA_SYSTEMD_UNIT_DIR:-/etc/systemd/system}"
render_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$render_dir"
}
trap cleanup EXIT

"$render_script" \
  --target-sha "$release_sha" \
  --artifact-dir "${release_dir}/sidecar" \
  --monorepo-dir "$release_dir" \
  --migrations-dir "${release_dir}/runtime/postgres/migrations" \
  --output-dir "$render_dir"

unit_files=(
  qintopia-message-sidecar.service
  qintopia-message-embedding-worker.service
  qintopia-message-identity-worker.service
  qintopia-agentos-member-profile-worker.service
  qintopia-agentos-graph-projection-worker.service
  qintopia-agentos-event-signal-worker.service
  qintopia-agentos-daily-digest-worker.service
  qintopia-agentos-daily-digest-publisher.service
  qintopia-agentos-raw-archive-worker.service
  qintopia-agentos-operations-workflow-sync.service
  qintopia-agentos-operations-workflow-sync.timer
  qintopia-agentos-operations-evidence-worker.service
  qintopia-agentos-operations-evidence-worker.timer
  qintopia-agentos-operations-visual-worker.service
  qintopia-agentos-operations-visual-worker.timer
  qintopia-agentos-operations-workbench-event.service
  qintopia-agentos-operations-workbench-event.timer
  qintopia-agentos-operations-group-send-ready.service
  qintopia-agentos-operations-group-send-ready.timer
  qintopia-agentos-xiaoman-activity-signal-worker.service
  qintopia-agentos-xiaoman-activity-signal-worker.timer
  qintopia-agentos-xiaoman-activity-promotion-starter-worker.service
  qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer
  qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service
  qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer
  qintopia-agentos-huabaosi-image-generation-preflight.service
  qintopia-agentos-huabaosi-image-generation-worker.service
  qintopia-agentos-huabaosi-image-generation-worker.timer
  qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service
  qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service
  qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer
  qintopia-agentos-qiwe-image-send-preflight.service
  qintopia-agentos-qiwe-image-send-worker.service
  qintopia-agentos-qiwe-image-send-worker.timer
  qintopia-agentos-xiaoman-activity-send-request-starter-worker.service
  qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer
)

mkdir -p "$unit_dir"
for unit_file in "${unit_files[@]}"; do
  source_path="${render_dir}/${unit_file}"
  if [[ ! -f "$source_path" ]]; then
    echo "rendered unit is missing: ${unit_file}" >&2
    exit 1
  fi
  install -m 0644 "$source_path" "${unit_dir}/${unit_file}"
done

"$systemctl_bin" daemon-reload

internal_timers=(
  qintopia-agentos-operations-workflow-sync.timer
  qintopia-agentos-operations-evidence-worker.timer
  qintopia-agentos-operations-visual-worker.timer
  qintopia-agentos-operations-workbench-event.timer
  qintopia-agentos-operations-group-send-ready.timer
  qintopia-agentos-xiaoman-activity-signal-worker.timer
  qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer
  qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer
  qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer
)

for timer in "${internal_timers[@]}"; do
  "$systemctl_bin" enable --now "$timer"
  "$systemctl_bin" is-active --quiet "$timer"
done

echo "Installed release systemd units for ${release_sha}"
