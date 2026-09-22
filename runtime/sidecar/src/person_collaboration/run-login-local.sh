#!/usr/bin/env bash
# Password workbench on a dedicated synthetic database; never fall back to production.
set -euo pipefail
sidecar_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != "--init-fixture" ) ]]; then
  echo 'Usage: bash run-login-local.sh [--init-fixture]' >&2
  exit 2
fi
export QINTOPIA_COLLABORATION_LOCAL_ENABLE=1
export QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL=postgres://postgres@127.0.0.1:55448/qintopia_test
export QINTOPIA_COLLABORATION_LOCAL_TENANT=synthetic-collaboration-login-20260918
unset QINTOPIA_COLLABORATION_LOCAL_OPERATOR_LINK
exec "$sidecar_dir/target/debug/qintopia-message-sidecar" run-collaboration-local --port 18876 "$@"
