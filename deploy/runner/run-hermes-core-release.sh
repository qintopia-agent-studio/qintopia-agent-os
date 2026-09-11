#!/usr/bin/env bash
set -euo pipefail

readonly CORE_ROOT=/var/lib/qintopia-hermes-core
readonly LOCK_FILE="${CORE_ROOT}/state/manager.lock"
readonly FLOCK_BIN=/usr/bin/flock
readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd -P)"
readonly CONTROLLER="${REPO_ROOT}/tools/deploy/run-hermes-core-release-production.mjs"

if [[ "${EUID}" -ne 0 ]]; then
  printf 'hermes_core_production=blocked\n' >&2
  printf 'hermes_core_production_error=runner_must_be_root\n' >&2
  exit 1
fi
if [[ ! -x "$FLOCK_BIN" || ! -f "$LOCK_FILE" || -L "$LOCK_FILE" || ! -f "$CONTROLLER" ]]; then
  printf 'hermes_core_production=blocked\n' >&2
  printf 'hermes_core_production_error=runner_dependency_missing\n' >&2
  exit 1
fi
node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node; do
  if [[ -x "$candidate" ]]; then
    node_bin="$candidate"
    break
  fi
done
[[ -n "$node_bin" ]] || {
  printf 'hermes_core_production=blocked\n' >&2
  printf 'hermes_core_production_error=runner_dependency_missing\n' >&2
  exit 1
}

exec 9<>"$LOCK_FILE"
if ! "$FLOCK_BIN" -n 9; then
  printf 'hermes_core_production=blocked\n' >&2
  printf 'hermes_core_production_error=manager_lock_busy\n' >&2
  exit 75
fi

exec /usr/bin/env -i \
  PATH=/usr/local/bin:/usr/bin:/bin \
  QINTOPIA_HERMES_CORE_LOCK_HELD=1 \
  "$node_bin" "$CONTROLLER" "$@"
