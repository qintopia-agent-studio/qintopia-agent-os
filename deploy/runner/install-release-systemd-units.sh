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
sidecar_env_file="/etc/qintopia/message-sidecar.env"
hermes_cron_snapshot_root="/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot"
if [[ -n "${QINTOPIA_RELEASE_SYSTEMD_INSTALL_TEST_ENV_FILE:-}" ]]; then
  case "$release_root" in
    /tmp/* | /private/tmp/* | /var/folders/* | /private/var/folders/*) ;;
    *)
      echo "test env file override is allowed only with a temporary release root" >&2
      exit 1
      ;;
  esac
  case "$QINTOPIA_RELEASE_SYSTEMD_INSTALL_TEST_ENV_FILE" in
    /tmp/* | /private/tmp/* | /var/folders/* | /private/var/folders/*)
      sidecar_env_file="$QINTOPIA_RELEASE_SYSTEMD_INSTALL_TEST_ENV_FILE"
      ;;
    *)
      echo "test env file override must stay under a temporary directory" >&2
      exit 1
      ;;
  esac
fi
render_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$render_dir"
}
trap cleanup EXIT

normalize_production_sidecar_env_metadata() {
  local env_file="$1"
  if [[ ! -e "$env_file" ]]; then
    echo "Production sidecar env file is absent; metadata normalization skipped"
    return
  fi
  python3 - "$env_file" <<'PY'
import os
import stat
import sys

path = sys.argv[1]
if not os.path.isabs(path):
    raise SystemExit("production sidecar env path must be absolute")
metadata = os.lstat(path)
if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
    raise SystemExit("production sidecar env must be a non-symlink regular file")
if metadata.st_nlink != 1:
    raise SystemExit("production sidecar env hard links are forbidden")
if metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
    raise SystemExit("production sidecar env must not be group/world writable")
if metadata.st_size > 1024 * 1024:
    raise SystemExit("production sidecar env is unexpectedly large")
PY
  chown root:ubuntu "$env_file"
  chmod 0640 "$env_file"
  echo "Normalized production sidecar env metadata"
}

normalize_production_sidecar_env_metadata "$sidecar_env_file"

prepare_hermes_cron_snapshot_root() {
  local snapshot_root="$1"
  case "$release_root" in
    /home/ubuntu/qintopia-agent-os-releases) ;;
    *)
      echo "Hermes cron snapshot root preparation skipped outside production release root"
      return
      ;;
  esac
  python3 - "$snapshot_root" <<'PY'
import errno
import os
import pwd
import grp
import stat
import sys
from pathlib import Path

snapshot_root = Path(sys.argv[1])
expected = Path("/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot")
if snapshot_root != expected:
    raise SystemExit("unexpected Hermes cron snapshot root")

flags = os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC
if hasattr(os, "O_NOFOLLOW"):
    flags |= os.O_NOFOLLOW

current_fd = os.open("/", flags)
snapshot_fd = None
try:
    for index, segment in enumerate(expected.parts[1:]):
        is_snapshot_root = index == len(expected.parts[1:]) - 1
        try:
            next_fd = os.open(segment, flags, dir_fd=current_fd)
        except FileNotFoundError:
            if segment in {"home", "ubuntu"}:
                raise SystemExit("Hermes cron snapshot base directory is missing")
            os.mkdir(segment, 0o700 if is_snapshot_root else 0o755, dir_fd=current_fd)
            next_fd = os.open(segment, flags, dir_fd=current_fd)
        except OSError as error:
            if error.errno in {errno.ELOOP, errno.ENOTDIR}:
                raise SystemExit(
                    "Hermes cron snapshot path components must be non-symlink directories"
                )
            raise

        metadata = os.fstat(next_fd)
        if not stat.S_ISDIR(metadata.st_mode):
            raise SystemExit(
                "Hermes cron snapshot path components must be non-symlink directories"
            )
        os.close(current_fd)
        current_fd = next_fd

    snapshot_fd = current_fd
    current_fd = None
    final_metadata = os.lstat(snapshot_root)
    snapshot_metadata = os.fstat(snapshot_fd)
    if stat.S_ISLNK(final_metadata.st_mode) or (
        final_metadata.st_dev,
        final_metadata.st_ino,
    ) != (snapshot_metadata.st_dev, snapshot_metadata.st_ino):
        raise SystemExit("Hermes cron snapshot root changed during preparation")
except BaseException:
    if current_fd is not None:
        os.close(current_fd)
    if snapshot_fd is not None:
        os.close(snapshot_fd)
    raise

ubuntu_uid = pwd.getpwnam("ubuntu").pw_uid
ubuntu_gid = grp.getgrnam("ubuntu").gr_gid
try:
    os.fchown(snapshot_fd, ubuntu_uid, ubuntu_gid)
    os.fchmod(snapshot_fd, 0o700)
    final_metadata = os.lstat(snapshot_root)
    snapshot_metadata = os.fstat(snapshot_fd)
    if stat.S_ISLNK(final_metadata.st_mode) or (
        final_metadata.st_dev,
        final_metadata.st_ino,
    ) != (snapshot_metadata.st_dev, snapshot_metadata.st_ino):
        raise SystemExit("Hermes cron snapshot root changed during preparation")
finally:
    os.close(snapshot_fd)
PY
  echo "Prepared Hermes cron snapshot root"
}

prepare_hermes_cron_snapshot_root "$hermes_cron_snapshot_root"

"$render_script" \
  --target-sha "$release_sha" \
  --artifact-dir "${release_dir}/sidecar" \
  --qiwe-artifact-dir "${release_dir}/sidecar-profiles/qiwe-production" \
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
  qintopia-agentos-operations-intake.service
  qintopia-agentos-xiaoman-poster-notification-starter.service
  qintopia-agentos-xiaoman-poster-notification-starter.timer
  qintopia-agentos-xiaoman-feishu-poster-preflight.service
  qintopia-agentos-xiaoman-feishu-poster-delivery.service
  qintopia-agentos-xiaoman-feishu-poster-delivery.timer
  qintopia-agentos-xiaoman-feishu-internal-group-poster-preflight.service
  qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.service
  qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer
  qintopia-agentos-xiaoman-poster-review-callback.service
  qintopia-agentos-huabaosi-image-generation-preflight.service
  qintopia-agentos-huabaosi-image-generation-worker.service
  qintopia-agentos-huabaosi-image-generation-worker.timer
  qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service
  qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service
  qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer
  qintopia-agentos-qiwe-image-send-preflight.service
  qintopia-agentos-qiwe-image-send-worker.service
  qintopia-agentos-qiwe-image-send-worker.timer
  qintopia-agentos-xiaoman-daily-case-report-auto-publish.service
  qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer
  qintopia-agentos-xiaoman-weekly-recruitment.service
  qintopia-agentos-xiaoman-weekly-recruitment.timer
  qintopia-agentos-xiaoman-weekly-plan-confirmation.service
  qintopia-agentos-xiaoman-weekly-plan-confirmation.timer
  qintopia-agentos-xiaoman-weekly-preview.service
  qintopia-agentos-xiaoman-weekly-preview.timer
  qintopia-agentos-xiaoman-activity-send-request-starter-worker.service
  qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer
  qintopia-agentos-erhua-morning-brief.service
  qintopia-agentos-erhua-morning-brief.timer
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

runner_unit_files=(
  qintopia-agent-os-deploy-runner.service
  qintopia-agent-os-deploy-runner.timer
)

for unit_file in "${runner_unit_files[@]}"; do
  source_path="${release_dir}/deploy/runner/${unit_file}"
  if [[ ! -f "$source_path" ]]; then
    echo "release deploy runner unit is missing: ${unit_file}" >&2
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
