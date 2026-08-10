#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE:-}" != "1" ]]; then
  echo "production worker-run evidence skipped: set QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE=1" >&2
  exit 0
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
PYTHON_BIN="/usr/bin/python3"

target="${1:-}"
evidence_key=""
service_name=""
timer_name=""
summary_path=""
expected_worker=""

case "$target" in
  erhua-morning-brief-worker-run)
    evidence_key="erhua_morning_brief"
    service_name="qintopia-agentos-erhua-morning-brief.service"
    timer_name="qintopia-agentos-erhua-morning-brief.timer"
    ;;
  xiaoman-daily-case-report-worker-run)
    evidence_key="xiaoman_daily_case_report"
    service_name="qintopia-agentos-xiaoman-daily-case-report-auto-publish.service"
    timer_name="qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer"
    ;;
  xiaoman-weekly-recruitment-worker-run)
    evidence_key="xiaoman_weekly_recruitment"
    service_name="qintopia-agentos-xiaoman-weekly-recruitment.service"
    timer_name="qintopia-agentos-xiaoman-weekly-recruitment.timer"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/latest-summary.json"
    expected_worker="xiaoman-weekly-recruitment-worker"
    ;;
  xiaoman-weekly-plan-confirmation-worker-run)
    evidence_key="xiaoman_weekly_plan_confirmation"
    service_name="qintopia-agentos-xiaoman-weekly-plan-confirmation.service"
    timer_name="qintopia-agentos-xiaoman-weekly-plan-confirmation.timer"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/latest-summary.json"
    expected_worker="xiaoman-weekly-plan-confirmation-worker"
    ;;
  xiaoman-weekly-preview-worker-run)
    evidence_key="xiaoman_weekly_preview"
    service_name="qintopia-agentos-xiaoman-weekly-preview.service"
    timer_name="qintopia-agentos-xiaoman-weekly-preview.timer"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview/latest-summary.json"
    expected_worker="xiaoman-weekly-preview-worker"
    ;;
  *)
    echo "unsupported production worker-run evidence target: ${target}" >&2
    exit 2
    ;;
esac

fail_evidence() {
  local reason="$1"
  echo "${evidence_key}_worker_run_error=${reason}"
  exit 1
}

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  fail_evidence "systemctl_unavailable"
fi
if ! "$SYSTEMCTL" is-enabled --quiet "$timer_name"; then
  fail_evidence "timer_not_enabled"
fi
if ! "$SYSTEMCTL" is-active --quiet "$timer_name"; then
  fail_evidence "timer_not_active"
fi

start_usec="$("$SYSTEMCTL" show --property=ExecMainStartTimestampUSec --value "$service_name")"
if [[ -z "$start_usec" || "$start_usec" == "0" || "$start_usec" == "n/a" ]]; then
  fail_evidence "service_never_started"
fi
if ! [[ "$start_usec" =~ ^[0-9]+$ ]]; then
  fail_evidence "worker_failed"
fi
exec_status="$("$SYSTEMCTL" show --property=ExecMainStatus --value "$service_name")"
result="$("$SYSTEMCTL" show --property=Result --value "$service_name")"
if [[ "$exec_status" != "0" || "$result" != "success" ]]; then
  fail_evidence "worker_failed"
fi
run_epoch="$((start_usec / 1000000))"

summary_date=""
if [[ -n "$summary_path" ]]; then
  if [[ ! -f "$summary_path" ]]; then
    fail_evidence "summary_missing"
  fi
  if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
    fail_evidence "python_unavailable"
  fi
  if [[ "$(wc -c <"$summary_path")" -gt 65536 ]]; then
    fail_evidence "summary_invalid"
  fi
  if ! summary_date="$("$PYTHON_BIN" - "$summary_path" "$expected_worker" <<'PY'
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
worker = sys.argv[2]
try:
    data = json.loads(path.read_text(encoding="utf-8"))
except Exception:
    raise SystemExit(1)
if not isinstance(data, dict):
    raise SystemExit(1)
if data.get("schema_version") != 1:
    raise SystemExit(1)
if data.get("worker") != worker:
    raise SystemExit(1)
if data.get("requires_human_confirmation") is not True:
    raise SystemExit(1)
if data.get("external_send_executed") is not False:
    raise SystemExit(1)
if data.get("safe_for_member_chat") is not False:
    raise SystemExit(1)
date_value = data.get("date") or data.get("week_start")
if not isinstance(date_value, str) or not re.fullmatch(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}", date_value
):
    raise SystemExit(1)
print(date_value)
PY
)"; then
    fail_evidence "summary_invalid"
  fi
fi

echo "${evidence_key}_worker_run_result=success"
echo "${evidence_key}_worker_run_epoch=${run_epoch}"
if [[ -n "$summary_path" ]]; then
  echo "${evidence_key}_worker_summary_present=true"
  echo "${evidence_key}_worker_summary_date=${summary_date}"
fi
