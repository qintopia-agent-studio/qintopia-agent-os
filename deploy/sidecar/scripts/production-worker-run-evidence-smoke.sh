#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE:-}" != "1" ]]; then
  echo "production worker-run evidence skipped: set QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE=1" >&2
  exit 0
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"

target="${1:-}"
evidence_key=""
task_name=""
log_path=""
summary_path=""
expected_worker=""

case "$target" in
  erhua-morning-brief-worker-run)
    evidence_key="erhua_morning_brief"
    task_name="erhua-morning-brief"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/erhua-morning-brief/hermes-cron.log"
    ;;
  xiaoman-daily-case-report-worker-run)
    evidence_key="xiaoman_daily_case_report"
    task_name="xiaoman-daily-case-report"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-daily-case-report/hermes-cron.log"
    ;;
  xiaoman-weekly-recruitment-worker-run)
    evidence_key="xiaoman_weekly_recruitment"
    task_name="xiaoman-weekly-recruitment"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/hermes-cron.log"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/latest-summary.json"
    expected_worker="xiaoman-weekly-recruitment-worker"
    ;;
  xiaoman-weekly-plan-confirmation-worker-run)
    evidence_key="xiaoman_weekly_plan_confirmation"
    task_name="xiaoman-weekly-plan-confirmation"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/hermes-cron.log"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/latest-summary.json"
    expected_worker="xiaoman-weekly-plan-confirmation-worker"
    ;;
  xiaoman-weekly-preview-worker-run)
    evidence_key="xiaoman_weekly_preview"
    task_name="xiaoman-weekly-preview"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview/hermes-cron.log"
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

if [[ ! -f "$log_path" ]]; then
  echo "${evidence_key}_worker_run_result=not_started"
  exit 0
fi

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  fail_evidence "python_unavailable"
fi

run_epoch=""
set +e
run_epoch="$("$PYTHON_BIN" - "$log_path" "$task_name" <<'PY'
import datetime as dt
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
task = sys.argv[2]
pattern = re.compile(
    r"^(?P<ts>[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z) "
    + re.escape(task)
    + r" run=(?P<status>ok|failed)(?: exit=[0-9]+)?$"
)

latest = None
try:
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for raw_line in handle:
            match = pattern.fullmatch(raw_line.strip())
            if match:
                latest = match
except OSError:
    raise SystemExit(3)

if latest is None:
    raise SystemExit(2)
if latest.group("status") != "ok":
    raise SystemExit(1)

try:
    timestamp = dt.datetime.strptime(
        latest.group("ts"), "%Y-%m-%dT%H:%M:%SZ"
    ).replace(tzinfo=dt.timezone.utc)
except ValueError:
    raise SystemExit(1)
print(int(timestamp.timestamp()))
PY
)"; parse_status=$?
set -e
case "$parse_status" in
  0)
    ;;
  2)
    echo "${evidence_key}_worker_run_result=not_started"
    exit 0
    ;;
  *)
    fail_evidence "worker_failed"
    ;;
esac

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
