#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG:-}" != "approved-production-erhua-morning-brief-config" ]]; then
  echo "Erhua morning brief production config requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
ENV_FILE="/etc/qintopia/message-sidecar.env"

fail() {
  echo "Erhua morning brief production config failed: $1" >&2
  exit 1
}

if [[ "$#" != "1" ]]; then
  fail "mode must be explicitly set to --enable or --disable"
fi

MODE="$1"
if [[ "$MODE" != "--enable" && "$MODE" != "--disable" ]]; then
  fail "mode must be --enable or --disable"
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  fail "fixed python3 is required"
fi
if [[ ! -f "$ENV_FILE" ]]; then
  fail "persistent sidecar env file is required"
fi

"$PYTHON_BIN" - "$ENV_FILE" "$MODE" <<'PY'
from __future__ import annotations

import os
import re
import stat
import sys
import tempfile
from pathlib import Path


path = Path(sys.argv[1])
mode = sys.argv[2]
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=")
target_values = {
    "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED": "1" if mode == "--enable" else "0",
    "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL": (
        "approved-production-erhua-morning-brief"
    ),
}
if mode == "--enable":
    target_values.update(
        {
            "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE": "1",
            "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE": "1",
            "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE": "1",
        }
    )

try:
    st = path.stat()
except FileNotFoundError:
    raise SystemExit("persistent sidecar env file is required")

original = path.read_text(encoding="utf-8").splitlines()
database_url_count = 0
kept: list[str] = []

for line in original:
    match = assignment.match(line)
    if not match:
        kept.append(line)
        continue
    key = match.group(1)
    if key == "QINTOPIA_SIDECAR_DATABASE_URL":
        database_url_count += 1
    if key in target_values:
        continue
    kept.append(line)

if database_url_count != 1:
    raise SystemExit("requires exactly one QINTOPIA_SIDECAR_DATABASE_URL")

while kept and not kept[-1].strip():
    kept.pop()
if kept:
    kept.append("")
for key, value in target_values.items():
    kept.append(f"{key}={value}")

env_dir = str(path.parent)
fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=env_dir, text=True)
try:
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write("\n".join(kept))
        fh.write("\n")
    os.chown(temp_name, st.st_uid, st.st_gid)
    os.chmod(temp_name, stat.S_IMODE(st.st_mode))
    os.replace(temp_name, path)
except Exception:
    try:
        os.unlink(temp_name)
    except FileNotFoundError:
        pass
    raise
PY

if [[ "$MODE" == "--enable" ]]; then
  echo "Erhua morning brief production config applied: enabled=1"
else
  echo "Erhua morning brief production config applied: enabled=0"
fi
