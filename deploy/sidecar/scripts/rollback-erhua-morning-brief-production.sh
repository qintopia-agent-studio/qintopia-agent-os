#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_ROLLBACK:-}" != "approved-production-erhua-morning-brief-rollback" ]]; then
  echo "Erhua morning brief rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
PYTHON_BIN="/usr/bin/python3"
ENV_FILE="/etc/qintopia/message-sidecar.env"
SERVICE_NAME="qintopia-agentos-erhua-morning-brief.service"
TIMER_NAME="qintopia-agentos-erhua-morning-brief.timer"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/erhua-morning-brief-timer-observation-smoke.sh"

fail() {
  echo "Erhua morning brief rollback failed: $1" >&2
  exit 1
}

if [[ ! -x "$SYSTEMCTL" || ! -x "$PYTHON_BIN" ]]; then
  fail "fixed systemctl and python3 are required"
fi
if [[ ! -f "$ENV_FILE" ]]; then
  fail "persistent sidecar env file is required"
fi
if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  fail "release-local observation script is required"
fi

env_value() {
  local key="$1"
  "$PYTHON_BIN" - "$ENV_FILE" "$key" <<'PY'
import re
import sys

path, expected_key = sys.argv[1:3]
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")
seen = False

with open(path, encoding="utf-8") as fh:
    for raw in fh:
        line = raw.rstrip("\r\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        if not match:
            continue
        key, value = match.groups()
        if key != expected_key:
            continue
        if seen:
            raise SystemExit(1)
        seen = True
        value = value.strip()
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        if "$(" in value or "`" in value or any(ord(ch) < 32 or ord(ch) == 127 for ch in value):
            raise SystemExit(1)
        print(value)
if not seen:
    raise SystemExit(1)
PY
}

require_env_value() {
  local key="$1"
  local expected="$2"
  local value
  value="$(env_value "$key")" || fail "requires exactly one ${key}"
  if [[ "$value" != "$expected" ]]; then
    fail "${key} is not set to the reviewed rollback value"
  fi
}

require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED" "0"
require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL" "approved-production-erhua-morning-brief"

"$SYSTEMCTL" disable --now "$TIMER_NAME"
"$SYSTEMCTL" stop "$SERVICE_NAME" || true
"$SYSTEMCTL" reset-failed "$SERVICE_NAME" || true

env -i \
  PATH="$PATH" \
  QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1 \
  QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE=disabled \
  "$OBSERVATION_SCRIPT" >/dev/null

echo "Erhua morning brief timer rolled back"
