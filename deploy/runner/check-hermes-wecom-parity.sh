#!/usr/bin/env bash
set -euo pipefail

PATH="/usr/local/bin:/usr/bin:/bin"
export PATH

readonly BASELINE_HOME="/home/ubuntu/.hermes"
readonly CANDIDATE_ROOT="/home/ubuntu/.local/state/qintopia-agentos/hermes-core-staging"
readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)"
readonly CHECKER="${REPO_ROOT}/tools/deploy/check-hermes-wecom-parity.mjs"

if [[ "$#" -ne 2 || "$1" != "--candidate-home" || "$2" != "${CANDIDATE_ROOT}/"* ]]; then
  printf '%s\n' \
    "hermes_wecom_parity=blocked" \
    "hermes_wecom_parity_error=invalid_invocation" >&2
  exit 2
fi
candidate_home="$2"

node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node /opt/homebrew/bin/node; do
  if [[ -x "$candidate" ]]; then
    node_bin="$candidate"
    break
  fi
done
if [[ -z "$node_bin" || ! -f "$CHECKER" ]]; then
  printf '%s\n' \
    "hermes_wecom_parity=blocked" \
    "hermes_wecom_parity_error=checker_unavailable" >&2
  exit 1
fi

exec /usr/bin/env -i PATH="$PATH" "$node_bin" "$CHECKER" \
  --baseline-home "$BASELINE_HOME" \
  --candidate-home "$candidate_home" \
  --mode production
