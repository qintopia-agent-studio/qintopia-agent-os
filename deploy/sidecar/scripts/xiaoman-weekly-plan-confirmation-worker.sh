#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED:-0}" != "1" ]]; then
  echo "xiaoman weekly plan confirmation skipped: persistent enablement is not 1" >&2
  exit 0
fi

if [[ -v QINTOPIA_RELEASE_DIR || -v QINTOPIA_XIAOMAN_WRAPPER_PATH || -v QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PYTHON || -v QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_OUTPUT_DIR ]]; then
  echo "xiaoman weekly plan confirmation refuses runtime path overrides" >&2
  exit 1
fi

RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
PYTHON_BIN="/usr/bin/python3"
WORKFLOW_PY="${RELEASE_DIR}/workflows/xiaoman-weekly-loop/weekly_loop.py"
WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation"

require_env() {
  local key="$1"
  if [[ -z "${!key:-}" ]]; then
    echo "xiaoman weekly plan confirmation requires ${key}" >&2
    exit 1
  fi
}

require_env "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_APPROVAL"
require_env "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_APPROVAL}" != "approved-production-xiaoman-weekly-plan-confirmation" ]]; then
  echo "xiaoman weekly plan confirmation requires the reviewed production approval value" >&2
  exit 1
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE}" != "1" ]]; then
  echo "xiaoman weekly plan confirmation requires Xiaoman activity wrappers to be enabled" >&2
  exit 1
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "xiaoman weekly plan confirmation requires an executable Python interpreter" >&2
  exit 1
fi
if [[ ! -f "$WORKFLOW_PY" ]]; then
  echo "xiaoman weekly plan confirmation workflow is missing from release/current" >&2
  exit 1
fi

mkdir -p "$WORK_DIR"
chmod 0700 "$WORK_DIR"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
report_json="${tmp_dir}/weekly-plan-confirmation.json"
review_message="${WORK_DIR}/latest-operator-review-message.txt"
summary_json="${WORK_DIR}/latest-summary.json"

args=("--mode" "weekly_plan_confirmation" "--json")
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_DATE:-}" ]]; then
  args+=("--date" "$QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_DATE")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_OPERATOR_NAME:-}" ]]; then
  args+=("--operator-name" "$QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_OPERATOR_NAME")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_AUDIENCE:-}" ]]; then
  args+=("--audience" "$QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_AUDIENCE")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_OWNER_NAME:-}" ]]; then
  args+=("--confirmation-owner-name" "$QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_OWNER_NAME")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PLAN_SHEET_LABEL:-}" ]]; then
  args+=("--plan-sheet-label" "$QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PLAN_SHEET_LABEL")
fi

PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" "$WORKFLOW_PY" "${args[@]}" >"$report_json"

"$PYTHON_BIN" - "$report_json" "$review_message" "$summary_json" <<'PY'
import json
import os
import sys
from pathlib import Path

report_path, message_path, summary_path = map(Path, sys.argv[1:4])
report = json.loads(report_path.read_text(encoding="utf-8"))

if report.get("success") is not True:
    raise SystemExit("xiaoman weekly plan confirmation worker received an unsuccessful report")
if report.get("mode") != "weekly_plan_confirmation":
    raise SystemExit("xiaoman weekly plan confirmation worker received the wrong mode")
if report.get("workflow_step") != "weekly_plan_confirmation":
    raise SystemExit("xiaoman weekly plan confirmation worker received the wrong workflow step")
if report.get("requires_human_confirmation") is not True:
    raise SystemExit("xiaoman weekly plan confirmation must keep human confirmation")
if report.get("external_send_executed") is not False:
    raise SystemExit("xiaoman weekly plan confirmation must not execute external send")
if report.get("safe_for_member_chat") is not False:
    raise SystemExit("xiaoman weekly plan confirmation must remain an operations-review draft")

message = str(report.get("operator_review_message") or "").strip()
if not message:
    raise SystemExit("xiaoman weekly plan confirmation did not return an operator review message")

message_path.write_text(message + "\n", encoding="utf-8")
os.chmod(message_path, 0o600)

summary = {
    "schema_version": 1,
    "worker": "xiaoman-weekly-plan-confirmation-worker",
    "mode": report.get("mode"),
    "workflow_step": report.get("workflow_step"),
    "date": report.get("date"),
    "record_source": report.get("record_source"),
    "mentions_count": len(report.get("mentions") or []),
    "requires_human_confirmation": report.get("requires_human_confirmation"),
    "external_send_executed": report.get("external_send_executed"),
    "safe_for_member_chat": report.get("safe_for_member_chat"),
    "operator_review_message_path": str(message_path),
}
summary_path.write_text(json.dumps(summary, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
os.chmod(summary_path, 0o600)
print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
PY
