#!/usr/bin/env bash
set -euo pipefail

# Fixed production input; staging is opt-in through an explicit argument.
PATH="/usr/local/bin:/usr/bin:/bin"
export PATH

readonly DEFAULT_HERMES_HOME="/home/ubuntu/.hermes"
readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)"
readonly CHECKER="${REPO_ROOT}/tools/deploy/check-hermes-wecom-readiness.mjs"

usage() {
  printf '%s\n' \
    "usage: check-hermes-wecom-readiness.sh" \
    "       check-hermes-wecom-readiness.sh --staging-home /absolute/path" >&2
}

hermes_home="$DEFAULT_HERMES_HOME"
mode="production"

if [[ "$#" -eq 0 ]]; then
  :
elif [[ "$#" -eq 2 && "$1" == "--staging-home" && "$2" == /* ]]; then
  hermes_home="$2"
  mode="staging"
  if [[ "$hermes_home" == "$DEFAULT_HERMES_HOME" ]]; then
    printf 'hermes_wecom_readiness=blocked\nhermes_wecom_error=staging_home_is_production_home\n' >&2
    exit 2
  fi
else
  usage
  printf 'hermes_wecom_readiness=blocked\nhermes_wecom_error=invalid_invocation\n' >&2
  exit 2
fi

if [[ ! -f "$CHECKER" ]]; then
  printf 'hermes_wecom_readiness=blocked\nhermes_wecom_error=checker_missing\n' >&2
  exit 1
fi

node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node /opt/homebrew/bin/node; do
  if [[ -x "$candidate" ]]; then
    node_bin="$candidate"
    break
  fi
done
if [[ -z "$node_bin" ]]; then
  printf 'hermes_wecom_readiness=blocked\nhermes_wecom_error=node_unavailable\n' >&2
  exit 1
fi

# Do not inherit operator environment, especially shell hooks or NODE_OPTIONS.
exec /usr/bin/env -i PATH="$PATH" "$node_bin" "$CHECKER" \
  --hermes-home "$hermes_home" \
  --mode "$mode"
