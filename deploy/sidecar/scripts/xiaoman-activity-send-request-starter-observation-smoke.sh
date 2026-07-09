#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "xiaoman activity send request starter observation skipped: set QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
SERVICE_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_NAME:-qintopia-agentos-xiaoman-activity-send-request-starter-worker.service}"
TIMER_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_NAME:-qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer}"
EXPECTED_INTERVAL="${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_INTERVAL_EXPECTED:-2min}"
JOURNAL_LINES="${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_JOURNAL_LINES:-80}"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
JOURNALCTL="${JOURNALCTL:-journalctl}"

cd "$MONOREPO_ROOT"

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi

source_env_if_present() {
  if [[ -f "$ENV_FILE" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$ENV_FILE"
    set +a
  fi
}

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "QINTOPIA_SIDECAR_DATABASE_URL=postgres://"
    "--use-feishu-base"
    "send_executed=true"
    "message_id"
    "raw_chat"
    "base_token"
  )

  local value_name
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN \
    QINTOPIA_DAILY_DIGEST_FEISHU_BASE_TOKEN \
    QIWE_TOKEN \
    QIWE_GUID; do
    if [[ -n "${!value_name:-}" ]]; then
      forbidden+=("${!value_name}")
    fi
  done

  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} leaked forbidden output: ${token}" >&2
      exit 1
    fi
  done
}

assert_timer() {
  if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
    echo "systemctl is required for Xiaoman activity send request starter timer observation" >&2
    exit 1
  fi

  if ! command -v "$JOURNALCTL" >/dev/null 2>&1; then
    echo "journalctl is required for Xiaoman activity send request starter timer observation" >&2
    exit 1
  fi

  timer_status="$tmp_dir/timer-status.txt"
  "$SYSTEMCTL" is-active "$TIMER_NAME" >"$timer_status"
  grep -Fx active "$timer_status" >/dev/null

  timer_enabled="$tmp_dir/timer-enabled.txt"
  "$SYSTEMCTL" is-enabled "$TIMER_NAME" >"$timer_enabled"
  grep -E '^(enabled|enabled-runtime|static)$' "$timer_enabled" >/dev/null

  service_unit="$tmp_dir/service-unit.txt"
  "$SYSTEMCTL" cat "$SERVICE_NAME" >"$service_unit"
  grep -E "ExecStart=.*run-xiaoman-activity-send-request-starter-worker --once --apply$" "$service_unit" >/dev/null
  assert_no_sensitive_output "service unit" "$service_unit"

  timer_unit="$tmp_dir/timer-unit.txt"
  "$SYSTEMCTL" cat "$TIMER_NAME" >"$timer_unit"
  grep -F "OnBootSec=9min" "$timer_unit" >/dev/null
  grep -F "OnUnitActiveSec=${EXPECTED_INTERVAL}" "$timer_unit" >/dev/null
  grep -F "Unit=${SERVICE_NAME}" "$timer_unit" >/dev/null
  assert_no_sensitive_output "timer unit" "$timer_unit"

  timer_list="$tmp_dir/list-timers.txt"
  "$SYSTEMCTL" list-timers "$TIMER_NAME" --no-pager >"$timer_list"
  grep -F "$TIMER_NAME" "$timer_list" >/dev/null
  assert_no_sensitive_output "timer list" "$timer_list"

  journal="$tmp_dir/journal.txt"
  "$JOURNALCTL" -u "$SERVICE_NAME" -n "$JOURNAL_LINES" --no-pager >"$journal" || true
  assert_no_sensitive_output "service journal" "$journal"
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

source_env_if_present
assert_timer

worker_check="$tmp_dir/worker-check.json"
"${BIN_CMD[@]}" run-xiaoman-activity-send-request-starter-worker --check-only >"$worker_check"
python3 - "$worker_check" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "xiaoman-activity-send-request-starter-worker"
assert payload["source"] == "agentos_work_items"
assert payload["dry_run"] is True
assert payload["check_only"] is True
assert payload["safe_for_chat"] is False
assert payload["action_status"] in {
    "no_eligible_approved_visual_artifacts",
    "group_message_requests_preview",
}
for field in (
    "scanned_count",
    "created_count",
    "existing_count",
    "missing_child_count",
):
    assert isinstance(payload[field], int)
assert isinstance(payload["work_items"], list)
PY
assert_no_sensitive_output "worker check" "$worker_check"

echo "xiaoman activity send request starter observation passed"
