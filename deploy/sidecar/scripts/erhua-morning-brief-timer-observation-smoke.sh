#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "erhua morning brief timer observation skipped: set QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

ENV_FILE="/etc/qintopia/message-sidecar.env"
SERVICE_NAME="qintopia-agentos-erhua-morning-brief.service"
TIMER_NAME="qintopia-agentos-erhua-morning-brief.timer"
EXPECTED_STATE="${QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_EXPECTED_STATE:-disabled}"
EXPECTED_CALENDAR="*-*-* 08:10:00"
JOURNAL_LINES="80"
JOURNAL_DISABLED_SINCE="30 minutes ago"
SYSTEMCTL="/usr/bin/systemctl"
JOURNALCTL="/usr/bin/journalctl"
PYTHON_BIN="/usr/bin/python3"
FIXED_HERMES_PYTHON="/home/ubuntu/.hermes/hermes-agent/venv/bin/python"

if [[ "$EXPECTED_STATE" != "enabled" && "$EXPECTED_STATE" != "disabled" ]]; then
  echo "erhua morning brief timer observation expected state must be enabled or disabled" >&2
  exit 1
fi
if [[ ! -x "$SYSTEMCTL" || ! -x "$JOURNALCTL" || ! -x "$PYTHON_BIN" ]]; then
  echo "erhua morning brief timer observation requires fixed systemctl, journalctl, and python3" >&2
  exit 1
fi

env_value() {
  local key="$1"
  if [[ ! -f "$ENV_FILE" ]]; then
    return 0
  fi
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
PY
}

require_observed_env_value() {
  local key="$1"
  local expected="$2"
  local value
  value="$(env_value "$key")" || {
    echo "erhua morning brief timer observation requires exactly one ${key}" >&2
    exit 1
  }
  if [[ "$value" != "$expected" ]]; then
    echo "erhua morning brief timer observation found unexpected ${key}" >&2
    exit 1
  fi
}

require_observed_env_present() {
  local key="$1"
  local value
  value="$(env_value "$key")" || {
    echo "erhua morning brief timer observation requires exactly one ${key}" >&2
    exit 1
  }
  if [[ -z "$value" ]]; then
    echo "erhua morning brief timer observation requires ${key}" >&2
    exit 1
  fi
}

require_observed_env_sha256() {
  local key="$1"
  local value
  value="$(env_value "$key")" || {
    echo "erhua morning brief timer observation requires exactly one ${key}" >&2
    exit 1
  }
  if [[ ! "$value" =~ ^[0-9a-f]{64}$ ]]; then
    echo "erhua morning brief timer observation requires canonical ${key}" >&2
    exit 1
  fi
}

require_observed_auto_publish_boundary() {
  require_observed_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED" "1"
  require_observed_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL" "approved-production-erhua-morning-brief-auto-publish"
  require_observed_env_value "QINTOPIA_QIWE_TEXT_SEND_ENABLED" "1"
  require_observed_env_value "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-text-send"
  require_observed_env_sha256 "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256"
  for key in \
    QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID \
    QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID \
    QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID \
    QIWE_API_URL \
    QIWE_TOKEN \
    QIWE_GUID \
    QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS \
    QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS; do
    require_observed_env_present "$key"
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

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  require_observed_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED" "1"
  require_observed_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL" "approved-production-erhua-morning-brief"
  require_observed_env_value "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE" "1"
  require_observed_env_value "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"
  require_observed_env_value "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"
  require_observed_env_value "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"
  require_observed_auto_publish_boundary
else
  erhua_enabled="$(env_value "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED")" || {
    echo "erhua morning brief timer observation requires at most one QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED" >&2
    exit 1
  }
  if [[ -n "$erhua_enabled" && "$erhua_enabled" != "0" ]]; then
    echo "erhua morning brief timer observation expected disabled Erhua config" >&2
    exit 1
  fi
fi

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "QINTOPIA_SIDECAR_DATABASE_URL=postgres://"
    "postgres://"
    "postgresql://"
    "QIWE_TOKEN="
    "QIWE_GUID="
    "base_token"
    "client_secret"
    "access_token"
    "send_executed=true"
    "operations-group-message-confirm"
    "run-group-message-send-worker"
    "morning_brief_text"
    "operator_review_message"
  )

  local value_name
  local value
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN \
    QINTOPIA_DAILY_DIGEST_FEISHU_BASE_TOKEN \
    QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_CONFIG \
    QIWE_TOKEN \
    QIWE_GUID; do
    value="$(env_value "$value_name" || true)"
    if [[ -n "$value" ]]; then
      forbidden+=("$value")
    fi
  done

  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

service_unit="$tmp_dir/service-unit.txt"
"$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"
grep -F "ExecStart=/usr/bin/env" "$service_unit" >/dev/null
grep -F "QINTOPIA_DEPLOYED_COMMIT_SHA=" "$service_unit" >/dev/null
grep -F "QINTOPIA_ERHUA_MORNING_BRIEF_PYTHON=${FIXED_HERMES_PYTHON}" "$service_unit" >/dev/null
grep -F "deploy/sidecar/scripts/erhua-morning-brief-worker.sh" "$service_unit" >/dev/null
grep -F "EnvironmentFile=/etc/qintopia/message-sidecar.env" "$service_unit" >/dev/null
grep -F "WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/" "$service_unit" >/dev/null
if grep -E "/usr/bin/env python3|(^|[[:space:]])python3([[:space:]]|$)" "$service_unit" >/dev/null; then
  echo "erhua morning brief service must not use a drifting python interpreter" >&2
  exit 1
fi
assert_no_sensitive_output "service unit" "$service_unit"

timer_unit="$tmp_dir/timer-unit.txt"
"$SYSTEMCTL" cat "$TIMER_NAME" >"$timer_unit"
grep -F "OnCalendar=${EXPECTED_CALENDAR}" "$timer_unit" >/dev/null
grep -F "Persistent=true" "$timer_unit" >/dev/null
grep -F "Unit=${SERVICE_NAME}" "$timer_unit" >/dev/null
assert_no_sensitive_output "timer unit" "$timer_unit"

timer_enabled="$tmp_dir/timer-enabled.txt"
"$SYSTEMCTL" is-enabled "$TIMER_NAME" >"$timer_enabled" 2>/dev/null || true
if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  grep -E '^(enabled|enabled-runtime|static)$' "$timer_enabled" >/dev/null
else
  grep -E '^(disabled|indirect|generated|transient|masked|linked|linked-runtime)$' "$timer_enabled" >/dev/null
fi

timer_status="$tmp_dir/timer-status.txt"
"$SYSTEMCTL" is-active "$TIMER_NAME" >"$timer_status" 2>/dev/null || true
timer_active_since=""
if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  grep -Fx active "$timer_status" >/dev/null
  timer_next="$("$SYSTEMCTL" show --property=NextElapseUSecRealtime --value "$TIMER_NAME")"
  if [[ -z "$timer_next" || "$timer_next" == "0" || "$timer_next" == "infinity" ]]; then
    echo "erhua morning brief timer must have a future realtime trigger" >&2
    exit 1
  fi
  timer_active_since="$("$SYSTEMCTL" show --property=ActiveEnterTimestamp --value "$TIMER_NAME")"
  if [[ -z "$timer_active_since" || "$timer_active_since" == "n/a" ]]; then
    echo "erhua morning brief timer must have an active-enter timestamp" >&2
    exit 1
  fi
else
  if grep -Fx active "$timer_status" >/dev/null; then
    echo "erhua morning brief timer must not be active in disabled observation" >&2
    exit 1
  fi
fi

timer_list="$tmp_dir/list-timers.txt"
"$SYSTEMCTL" list-timers "$TIMER_NAME" --no-pager >"$timer_list" 2>/dev/null || true
assert_no_sensitive_output "timer list" "$timer_list"

journal="$tmp_dir/journal.txt"
if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  "$JOURNALCTL" -u "$SERVICE_NAME" --since "$timer_active_since" -n "$JOURNAL_LINES" --no-pager >"$journal" 2>/dev/null || true
else
  "$JOURNALCTL" -u "$SERVICE_NAME" --since "$JOURNAL_DISABLED_SINCE" -n "$JOURNAL_LINES" --no-pager >"$journal" 2>/dev/null || true
fi
assert_no_sensitive_output "service journal" "$journal"

echo "erhua morning brief timer observation passed"
