#!/usr/bin/env bash
set -euo pipefail

readonly STATE_ROOT=/var/lib/qintopia-agent-os-deploy
readonly LOCK_FILE="${STATE_ROOT}/hermes-core-bootstrap.lock"
readonly FLOCK_BIN=/usr/bin/flock
readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd -P)"
readonly BOOTSTRAPPER="${REPO_ROOT}/tools/deploy/bootstrap-hermes-core-root.mjs"

blocked() {
  printf 'hermes_core_bootstrap=blocked\n' >&2
  printf 'hermes_core_bootstrap_error=%s\n' "$1" >&2
  exit "${2:-1}"
}

if [[ "${EUID}" -ne 0 ]]; then
  blocked runner_must_be_root
fi
if [[ ! -x "${FLOCK_BIN}" || ! -f "${BOOTSTRAPPER}" ]]; then
  blocked runner_dependency_missing
fi
if [[ -L "${STATE_ROOT}" || ! -d "${STATE_ROOT}" \
  || "$(stat -c '%u:%g:%a' "${STATE_ROOT}" 2>/dev/null || true)" != "0:0:700" ]]; then
  blocked bootstrap_state_root_invalid
fi

node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node; do
  if [[ -x "${candidate}" ]]; then
    node_bin="${candidate}"
    break
  fi
done
if [[ -z "${node_bin}" ]]; then
  blocked runner_dependency_missing
fi
python_bin=""
for candidate in /usr/bin/python3 /usr/local/bin/python3; do
  if [[ -x "${candidate}" ]]; then
    python_bin="${candidate}"
    break
  fi
done
if [[ -z "${python_bin}" ]]; then
  blocked runner_dependency_missing
fi

if ! "${python_bin}" - "${LOCK_FILE}" "${STATE_ROOT}" <<'PY'
import os
import stat
import sys

lock_path, parent = sys.argv[1:3]
flags = os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)
try:
    descriptor = os.open(lock_path, flags | os.O_CREAT | os.O_EXCL, 0o600)
    created = True
except FileExistsError:
    descriptor = os.open(lock_path, flags)
    created = False
try:
    if created:
        os.fchown(descriptor, 0, 0)
        os.fchmod(descriptor, 0o600)
    path_metadata = os.lstat(lock_path)
    descriptor_metadata = os.fstat(descriptor)
    if (
        stat.S_ISLNK(path_metadata.st_mode)
        or not stat.S_ISREG(path_metadata.st_mode)
        or path_metadata.st_nlink != 1
        or path_metadata.st_uid != 0
        or path_metadata.st_gid != 0
        or stat.S_IMODE(path_metadata.st_mode) != 0o600
        or path_metadata.st_dev != descriptor_metadata.st_dev
        or path_metadata.st_ino != descriptor_metadata.st_ino
    ):
        raise RuntimeError("invalid lock")
    os.fsync(descriptor)
finally:
    os.close(descriptor)
parent_descriptor = os.open(
    parent,
    os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0),
)
try:
    os.fsync(parent_descriptor)
finally:
    os.close(parent_descriptor)
PY
then
  blocked bootstrap_lock_invalid
fi

exec 9<>"${LOCK_FILE}"
if ! "${FLOCK_BIN}" -n 9; then
  blocked bootstrap_lock_busy 75
fi
lock_path_identity="$(stat -c '%d:%i:%u:%g:%a:%h' "${LOCK_FILE}" 2>/dev/null || true)"
lock_fd_identity="$(stat -Lc '%d:%i:%u:%g:%a:%h' /proc/self/fd/9 2>/dev/null || true)"
if [[ -z "${lock_path_identity}" || "${lock_path_identity}" != "${lock_fd_identity}" \
  || "${lock_path_identity}" != *:0:0:600:1 ]]; then
  blocked bootstrap_lock_invalid
fi

exec /usr/bin/env -i \
  PATH=/usr/local/bin:/usr/bin:/bin \
  QINTOPIA_HERMES_CORE_BOOTSTRAP_LOCK_HELD=1 \
  "${node_bin}" "${BOOTSTRAPPER}" "$@"
