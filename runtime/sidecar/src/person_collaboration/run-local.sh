#!/usr/bin/env bash
# Local synthetic workbench only. Never select production configuration implicitly.
set -euo pipefail
sidecar_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != "--init-fixture" ) ]]; then
  echo 'Usage: bash run-local.sh [--init-fixture]' >&2
  exit 2
fi
binary="$sidecar_dir/target/debug/qintopia-message-sidecar"
if [[ ! -x "$binary" ]]; then
  echo 'Build the sidecar first: cargo build --manifest-path runtime/sidecar/Cargo.toml' >&2
  exit 1
fi
export QINTOPIA_COLLABORATION_LOCAL_ENABLE=1
export QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL=postgres://postgres@127.0.0.1:55439/qintopia_test
export QINTOPIA_COLLABORATION_LOCAL_TENANT=synthetic-collaboration-confirmed-20260911
unset QINTOPIA_COLLABORATION_LOCAL_OPERATOR_LINK
exec "$binary" run-collaboration-local --port 18875 "$@"
