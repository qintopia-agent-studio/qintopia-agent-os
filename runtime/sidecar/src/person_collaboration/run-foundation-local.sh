#!/usr/bin/env bash
# Rebuild the exact current source before serving the isolated first-batch demo.
set -euo pipefail
sidecar_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != "--init-fixture" ) ]]; then
  echo 'Usage: run-foundation-local.sh [--init-fixture]' >&2
  exit 2
fi
: "${QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL:?Set an explicit isolated loopback qintopia_test database}"
: "${QINTOPIA_COLLABORATION_LOCAL_TENANT:?Set a unique synthetic-collaboration- tenant}"
: "${QINTOPIA_FOUNDATION_LOCAL_PORT:?Set a free loopback port}"
: "${QINTOPIA_WELCOME_RENDER_PYTHON:?Set a Python interpreter with Pillow}"
if [[ "${1:-}" == --init-fixture ]]; then
  : "${QINTOPIA_FOUNDATION_FIXTURE_PASSWORD:?Set a synthetic account password of at least 12 characters}"
fi
export QINTOPIA_COLLABORATION_LOCAL_ENABLE=1
export QINTOPIA_FOUNDATION_LOCAL_ENABLE=1
unset QINTOPIA_COLLABORATION_LOCAL_OPERATOR_LINK
cargo build --locked --manifest-path "$sidecar_dir/Cargo.toml"
python3 - "$QINTOPIA_FOUNDATION_LOCAL_PORT" <<'PY'
import socket, sys
port = int(sys.argv[1])
if not 1024 <= port <= 65535:
    raise SystemExit('Choose a non-privileged port.')
with socket.socket() as probe:
    probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    probe.bind(('127.0.0.1', port))
    probe.listen(1)
PY
python3 - "$sidecar_dir" <<'PY'
import hashlib, pathlib, subprocess, sys
root = pathlib.Path(sys.argv[1]).parents[1]
paths = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', 'runtime/sidecar', 'runtime/postgres/migrations', 'registry', 'workflows/resident-welcome', 'skills/person-foundation', 'skills/qintopia-tools/variants/erhua', 'agents/anan', 'agents/huabaosi', 'agents/erhua'], cwd=root).split(b'\0')
h = hashlib.sha256()
for name in sorted(set(paths)):
    if name:
        p = root / name.decode()
        if p.is_file(): h.update(name + b'\0' + p.read_bytes())
binary = pathlib.Path(sys.argv[1]) / 'target/debug/qintopia-message-sidecar'
print('Foundation source SHA256:', h.hexdigest())
print('Foundation binary SHA256:', hashlib.sha256(binary.read_bytes()).hexdigest())
PY
exec "$sidecar_dir/target/debug/qintopia-message-sidecar" run-collaboration-local --port "$QINTOPIA_FOUNDATION_LOCAL_PORT" "$@"
