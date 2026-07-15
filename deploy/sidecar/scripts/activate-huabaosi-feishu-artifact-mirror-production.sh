#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION:-}" != "approved-production-huabaosi-feishu-artifact-mirror" ]]; then
  echo "Huabaosi Feishu mirror production activation requires explicit owner approval" >&2
  exit 1
fi

SYSTEMCTL="${SYSTEMCTL:-systemctl}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
PREFLIGHT_SERVICE="qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service"
WORKER_TIMER="qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi Feishu mirror production activation" >&2
  exit 1
fi

if ! python3 - "$ENV_FILE" <<'PY'
import re
import sys

path = sys.argv[1]
key_name = "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED"
values = []
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")

try:
    fh = open(path, encoding="utf-8")
except OSError:
    raise SystemExit(1)

with fh:
    for raw in fh:
        line = raw.rstrip("\r\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        if not match:
            continue
        key, value = match.groups()
        if key != key_name:
            continue
        if any(token in value for token in ("$(", "`", "\\", ";", "|", "&", "<", ">", "(", ")")):
            raise SystemExit(1)
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        values.append(value)

if values != ["1"]:
    raise SystemExit(1)
PY
then
  echo "Huabaosi Feishu mirror production activation requires one persistent enable flag set to 1" >&2
  exit 1
fi

"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
"$SYSTEMCTL" enable --now "$WORKER_TIMER"
"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"
"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"

echo "Huabaosi Feishu artifact mirror production timer activated"
