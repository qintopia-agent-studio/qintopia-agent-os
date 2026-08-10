#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_ACTIVATION:-}" != "approved-production-erhua-morning-brief" ]]; then
  echo "Erhua morning brief production activation requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
PYTHON_BIN="/usr/bin/python3"
ENV_FILE="/etc/qintopia/message-sidecar.env"
RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
HERMES_PYTHON="/home/ubuntu/.hermes/hermes-agent/venv/bin/python"
HERMES_VENV="/home/ubuntu/.hermes/hermes-agent/venv"
SERVICE_NAME="qintopia-agentos-erhua-morning-brief.service"
TIMER_NAME="qintopia-agentos-erhua-morning-brief.timer"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/erhua-morning-brief-timer-observation-smoke.sh"
ERHUA_CRON_OBSERVATION_SCRIPT="${SCRIPT_DIR}/erhua-legacy-cron-observation-smoke.sh"
XIAOMAN_CRON_OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-legacy-cron-observation-smoke.sh"

fail() {
  echo "Erhua morning brief activation failed: $1" >&2
  exit 1
}

if [[ ! -x "$SYSTEMCTL" || ! -x "$PYTHON_BIN" ]]; then
  fail "fixed systemctl and python3 are required"
fi
if [[ ! -f "$ENV_FILE" ]]; then
  fail "persistent sidecar env file is required"
fi
for script in "$OBSERVATION_SCRIPT" "$ERHUA_CRON_OBSERVATION_SCRIPT" "$XIAOMAN_CRON_OBSERVATION_SCRIPT"; do
  if [[ ! -x "$script" ]]; then
    fail "release-local observation script is missing"
  fi
done

release_dir="$("$PYTHON_BIN" - "$RELEASE_CURRENT_DIR" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
if not path.exists() and not path.is_symlink():
    raise SystemExit(1)
resolved = path.resolve()
if not resolved.is_dir():
    raise SystemExit(1)
print(resolved)
PY
)" || fail "release/current is not a valid release directory"

python_validator="${release_dir}/runtime/hermes/validate_hermes_python.py"
if [[ ! -f "$python_validator" ]]; then
  fail "Hermes Python validator is missing from release/current"
fi
PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" "$python_validator" \
  --python "$HERMES_PYTHON" \
  --venv-dir "$HERMES_VENV" \
  --release-dir "$release_dir" >/dev/null

require_present_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^(export[[:space:]]+)?${key}[[:space:]]*=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    fail "requires exactly one ${key}"
  fi
}

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
    fail "${key} is not set to the reviewed production value"
  fi
}

require_env_present() {
  local key="$1"
  local value
  value="$(env_value "$key")" || fail "requires exactly one ${key}"
  if [[ -z "$value" ]]; then
    fail "${key} is required"
  fi
}

require_env_sha256() {
  local key="$1"
  local value
  value="$(env_value "$key")" || fail "requires exactly one ${key}"
  if [[ ! "$value" =~ ^[0-9a-f]{64}$ ]]; then
    fail "${key} must be a canonical SHA-256"
  fi
}

require_auto_publish_boundary() {
  require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED" "1"
  require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL" "approved-production-erhua-morning-brief-auto-publish"
  require_env_value "QINTOPIA_QIWE_TEXT_SEND_ENABLED" "1"
  require_env_value "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-text-send"
  require_env_sha256 "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256"
  for key in \
    QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID \
    QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID \
    QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID \
    QIWE_API_URL \
    QIWE_TOKEN \
    QIWE_GUID \
    QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS \
    QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS; do
    require_env_present "$key"
  done
  "$PYTHON_BIN" - "$ENV_FILE" <<'PY'
from urllib.parse import urlparse
import re
import sys

path = sys.argv[1]
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")
values = {}
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
        value = value.strip()
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        values.setdefault(key, value)

target = values["QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID"]
allowed_groups = {
    item.strip()
    for item in values["QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS"].split(",")
    if item.strip()
}
if target not in allowed_groups:
    raise SystemExit("Erhua morning brief target group id is not allowlisted")

url = urlparse(values["QIWE_API_URL"])
allowed_hosts = {
    item.strip().lower()
    for item in values["QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS"].split(",")
    if item.strip()
}
if (
    url.scheme != "https"
    or url.hostname is None
    or url.username
    or url.password
    or url.path != "/qiwe/api/qw/doApi"
    or url.query
    or url.fragment
    or url.hostname.lower() not in allowed_hosts
):
    raise SystemExit("QiWe API URL is outside the reviewed host/path allowlist")
PY
}

cleanup_failed_activation() {
  "$SYSTEMCTL" disable --now "$TIMER_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" stop "$SERVICE_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" reset-failed "$SERVICE_NAME" >/dev/null 2>&1 || true
}

require_present_env_line "QINTOPIA_SIDECAR_DATABASE_URL"
require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED" "1"
require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL" "approved-production-erhua-morning-brief"
require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE" "1"
require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"
require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"
require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"
require_auto_publish_boundary

env -i PATH="$PATH" QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE=1 "$ERHUA_CRON_OBSERVATION_SCRIPT" >/dev/null
env -i PATH="$PATH" QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 "$XIAOMAN_CRON_OBSERVATION_SCRIPT" >/dev/null

"$SYSTEMCTL" daemon-reload
"$SYSTEMCTL" enable "$TIMER_NAME"
"$SYSTEMCTL" restart "$TIMER_NAME"
if ! "$SYSTEMCTL" is-enabled --quiet "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
if ! "$SYSTEMCTL" is-active --quiet "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
timer_next="$("$SYSTEMCTL" show --property=NextElapseUSecRealtime --value "$TIMER_NAME")"
if [[ -z "$timer_next" || "$timer_next" == "0" || "$timer_next" == "infinity" ]]; then
  cleanup_failed_activation
  fail "timer does not have a future realtime trigger"
fi

if ! env -i \
  PATH="$PATH" \
  QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1 \
  QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE=enabled \
  "$OBSERVATION_SCRIPT" >/dev/null; then
  cleanup_failed_activation
  exit 1
fi

echo "Erhua morning brief production timer enabled"
