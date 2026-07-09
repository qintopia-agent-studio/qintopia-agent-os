#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "operations downstream timers observation skipped: set QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1 to inspect evidence/visual timer state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
EVIDENCE_SERVICE_NAME="${QINTOPIA_OPERATIONS_EVIDENCE_WORKER_SERVICE_NAME:-qintopia-agentos-operations-evidence-worker.service}"
EVIDENCE_TIMER_NAME="${QINTOPIA_OPERATIONS_EVIDENCE_WORKER_TIMER_NAME:-qintopia-agentos-operations-evidence-worker.timer}"
EVIDENCE_EXPECTED_INTERVAL="${QINTOPIA_OPERATIONS_EVIDENCE_WORKER_TIMER_INTERVAL_EXPECTED:-2min}"
VISUAL_SERVICE_NAME="${QINTOPIA_OPERATIONS_VISUAL_WORKER_SERVICE_NAME:-qintopia-agentos-operations-visual-worker.service}"
VISUAL_TIMER_NAME="${QINTOPIA_OPERATIONS_VISUAL_WORKER_TIMER_NAME:-qintopia-agentos-operations-visual-worker.timer}"
VISUAL_EXPECTED_INTERVAL="${QINTOPIA_OPERATIONS_VISUAL_WORKER_TIMER_INTERVAL_EXPECTED:-2min}"
JOURNAL_LINES="${QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_JOURNAL_LINES:-80}"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
JOURNALCTL="${JOURNALCTL:-journalctl}"

cd "$MONOREPO_ROOT"

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for operations downstream timer observation" >&2
  exit 1
fi

if ! command -v "$JOURNALCTL" >/dev/null 2>&1; then
  echo "journalctl is required for operations downstream timer observation" >&2
  exit 1
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
    "run-group-message-send-worker"
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
  local label="$1"
  local service_name="$2"
  local timer_name="$3"
  local expected_boot_sec="$4"
  local expected_interval="$5"
  local expected_exec="$6"
  local tmp_dir="$7"

  local timer_status="${tmp_dir}/${label}-timer-status.txt"
  "$SYSTEMCTL" is-active "$timer_name" >"$timer_status"
  grep -Fx active "$timer_status" >/dev/null

  local timer_enabled="${tmp_dir}/${label}-timer-enabled.txt"
  "$SYSTEMCTL" is-enabled "$timer_name" >"$timer_enabled"
  grep -E '^(enabled|enabled-runtime|static)$' "$timer_enabled" >/dev/null

  local service_unit="${tmp_dir}/${label}-service-unit.txt"
  "$SYSTEMCTL" cat "$service_name" >"$service_unit"
  grep -E "ExecStart=.*${expected_exec}$" "$service_unit" >/dev/null
  assert_no_sensitive_output "${label} service unit" "$service_unit"

  local timer_unit="${tmp_dir}/${label}-timer-unit.txt"
  "$SYSTEMCTL" cat "$timer_name" >"$timer_unit"
  grep -F "OnBootSec=${expected_boot_sec}" "$timer_unit" >/dev/null
  grep -F "OnUnitActiveSec=${expected_interval}" "$timer_unit" >/dev/null
  grep -F "Unit=${service_name}" "$timer_unit" >/dev/null
  assert_no_sensitive_output "${label} timer unit" "$timer_unit"

  local timer_list="${tmp_dir}/${label}-list-timers.txt"
  "$SYSTEMCTL" list-timers "$timer_name" --no-pager >"$timer_list"
  grep -F "$timer_name" "$timer_list" >/dev/null
  assert_no_sensitive_output "${label} timer list" "$timer_list"

  local journal="${tmp_dir}/${label}-journal.txt"
  "$JOURNALCTL" -u "$service_name" -n "$JOURNAL_LINES" --no-pager >"$journal" || true
  assert_no_sensitive_output "${label} service journal" "$journal"
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

source_env_if_present

assert_timer \
  "evidence" \
  "$EVIDENCE_SERVICE_NAME" \
  "$EVIDENCE_TIMER_NAME" \
  "7min" \
  "$EVIDENCE_EXPECTED_INTERVAL" \
  "run-evidence-worker --once --apply" \
  "$tmp_dir"

assert_timer \
  "visual" \
  "$VISUAL_SERVICE_NAME" \
  "$VISUAL_TIMER_NAME" \
  "8min" \
  "$VISUAL_EXPECTED_INTERVAL" \
  "run-collaboration-worker --work-item-type visual_asset_request --once --apply" \
  "$tmp_dir"

evidence_check="$tmp_dir/evidence-worker-check.json"
"${BIN_CMD[@]}" run-evidence-worker --once --dry-run >"$evidence_check"
python3 - "$evidence_check" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "evidence-worker"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["fixture_mode"] is False
assert payload["action_status"] in {
    "dry_run_ok",
    "no_claimable_evidence_request",
}
assert isinstance(payload["artifact_ids"], list)
assert isinstance(payload["artifact_previews"], list)
assert isinstance(payload["limitations"], list)
assert isinstance(payload["guardrails"], list)
PY
assert_no_sensitive_output "evidence worker check" "$evidence_check"

visual_check="$tmp_dir/visual-worker-check.json"
"${BIN_CMD[@]}" run-collaboration-worker --work-item-type visual_asset_request --once --dry-run >"$visual_check"
python3 - "$visual_check" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "collaboration-worker"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["fixture_mode"] is False
assert payload["action_status"] in {
    "dry_run_ok",
    "no_claimable_work_item",
}
assert isinstance(payload["artifact_ids"], list)
assert isinstance(payload["artifact_previews"], list)
assert isinstance(payload["limitations"], list)
assert isinstance(payload["guardrails"], list)
PY
assert_no_sensitive_output "visual worker check" "$visual_check"

echo "operations downstream timers observation passed"
