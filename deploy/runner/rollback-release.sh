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

ln -sfn "$current_target" "${release_root}/rollback-from"
ln -sfn "$previous_target" "${release_root}/current"

echo "Rolled back current to ${previous_target}"
