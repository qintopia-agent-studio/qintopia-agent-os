#!/usr/bin/env bash
set -euo pipefail

readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd -P)"
readonly BUILDER="${REPO_ROOT}/tools/deploy/build-hermes-core-artifact.mjs"

if [[ ! -f "$BUILDER" ]]; then
  printf 'hermes_core_artifact_build=blocked\n' >&2
  printf 'hermes_core_artifact_error=builder_dependency_missing\n' >&2
  exit 1
fi

node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node; do
  if [[ -x "$candidate" ]]; then
    node_bin="$candidate"
    break
  fi
done
if [[ -z "$node_bin" ]]; then
  printf 'hermes_core_artifact_build=blocked\n' >&2
  printf 'hermes_core_artifact_error=builder_dependency_missing\n' >&2
  exit 1
fi

exec "$node_bin" "$BUILDER" "$@"
