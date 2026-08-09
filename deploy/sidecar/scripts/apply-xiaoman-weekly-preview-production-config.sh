#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_CONFIG:-}" != "approved-production-xiaoman-weekly-preview-config" ]]; then
  echo "xiaoman weekly preview production config requires explicit owner approval" >&2
  exit 1
fi

PYTHON_BIN="/usr/bin/python3"
ENV_FILE="/etc/qintopia/message-sidecar.env"

if [[ "${1:-}" != "--enable" && "${1:-}" != "--disable" ]]; then
  echo "usage: apply-xiaoman-weekly-preview-production-config.sh --enable|--disable" >&2
  exit 2
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "xiaoman weekly preview config requires /usr/bin/python3" >&2
  exit 1
fi
if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman weekly preview config requires the persistent sidecar env file" >&2
  exit 1
fi

"$PYTHON_BIN" - "$ENV_FILE" "$1" <<'PY'
from pathlib import Path
import os
import re
import sys
import tempfile

env_path = Path(sys.argv[1])
mode = sys.argv[2]
enabled = "1" if mode == "--enable" else "0"

required_single = ["QINTOPIA_SIDECAR_DATABASE_URL"]
managed = {
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED": enabled,
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL": "approved-production-xiaoman-weekly-preview",
    "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE": "1",
    "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE": "1",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE": "1",
}

assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=")
lines = env_path.read_text(encoding="utf-8").splitlines()

counts = {key: 0 for key in required_single}
for line in lines:
    match = assignment.match(line.strip())
    if match and match.group(1) in counts:
        counts[match.group(1)] += 1
if counts["QINTOPIA_SIDECAR_DATABASE_URL"] != 1:
    raise SystemExit("requires exactly one QINTOPIA_SIDECAR_DATABASE_URL")

filtered = []
for line in lines:
    match = assignment.match(line.strip())
    if match and match.group(1) in managed:
        continue
    filtered.append(line)

if filtered and filtered[-1].strip():
    filtered.append("")
filtered.append("# Managed by apply-xiaoman-weekly-preview-production-config.sh")
for key, value in managed.items():
    filtered.append(f"{key}={value}")

payload = "\n".join(filtered).rstrip() + "\n"
stat = env_path.stat()
fd, tmp_name = tempfile.mkstemp(prefix=f".{env_path.name}.", dir=str(env_path.parent), text=True)
try:
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    os.chown(tmp_name, stat.st_uid, stat.st_gid)
    os.chmod(tmp_name, stat.st_mode & 0o777)
    os.replace(tmp_name, env_path)
except Exception:
    try:
        os.unlink(tmp_name)
    except FileNotFoundError:
        pass
    raise

print(f"xiaoman weekly preview production config applied: enabled={enabled}")
PY
