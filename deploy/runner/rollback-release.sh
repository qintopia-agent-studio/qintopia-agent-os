#!/usr/bin/env bash
set -euo pipefail

release_root="/home/ubuntu/qintopia-agent-os-releases"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-root)
      release_root="${2:-}"
      shift 2
      ;;
    -h | --help)
      echo "Usage: deploy/runner/rollback-release.sh [--release-root <dir>]"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

previous_target="$(readlink -f "${release_root}/previous" 2>/dev/null || true)"
current_target="$(readlink -f "${release_root}/current" 2>/dev/null || true)"

if [[ -z "$previous_target" || ! -d "$previous_target" ]]; then
  echo "previous release target is missing" >&2
  exit 1
fi
if [[ -z "$current_target" || ! -d "$current_target" ]]; then
  echo "current release target is missing" >&2
  exit 1
fi

atomic_symlink() {
  python3 - "$release_root" "$1" "$2" <<'PY'
import os
import secrets
import sys

root, name, target = sys.argv[1:4]
temporary = os.path.join(root, f".{name}.{os.getpid()}.{secrets.token_hex(8)}")
try:
    os.symlink(target, temporary)
    os.replace(temporary, os.path.join(root, name))
    descriptor = os.open(root, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
finally:
    try:
        os.unlink(temporary)
    except FileNotFoundError:
        pass
PY
}

atomic_symlink rollback-from "$current_target"
atomic_symlink current "$previous_target"

if [[ "$(readlink -f "${release_root}/current" 2>/dev/null || true)" != "$previous_target" ]]; then
  echo "rollback current target verification failed" >&2
  exit 1
fi

echo "Rolled back current to ${previous_target}"
