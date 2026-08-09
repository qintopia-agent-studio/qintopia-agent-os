#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED:-0}" != "1" ]]; then
  echo "xiaoman weekly recruitment skipped: persistent enablement is not 1" >&2
  exit 0
fi

if [[ -v QINTOPIA_RELEASE_DIR || -v QINTOPIA_XIAOMAN_WRAPPER_PATH || -v QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PYTHON || -v QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OUTPUT_DIR ]]; then
  echo "xiaoman weekly recruitment refuses runtime path overrides" >&2
  exit 1
fi

RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
PYTHON_BIN="/usr/bin/python3"
WORKFLOW_PY="${RELEASE_DIR}/workflows/xiaoman-weekly-loop/weekly_loop.py"
WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment"

require_env() {
  local key="$1"
  if [[ -z "${!key:-}" ]]; then
    echo "xiaoman weekly recruitment requires ${key}" >&2
    exit 1
  fi
}

require_env "QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_APPROVAL"
require_env "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"
require_env "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE"
require_env "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_APPROVAL}" != "approved-production-xiaoman-weekly-recruitment" ]]; then
  echo "xiaoman weekly recruitment requires the reviewed production approval value" >&2
  exit 1
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE}" != "1" ]]; then
  echo "xiaoman weekly recruitment requires Xiaoman activity wrappers to be enabled" >&2
  exit 1
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE}" != "1" ]]; then
  echo "xiaoman weekly recruitment requires Xiaoman activity Feishu Base mode to be enabled" >&2
  exit 1
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE}" != "1" ]]; then
  echo "xiaoman weekly recruitment requires Xiaoman activity read-through to be enabled" >&2
  exit 1
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "xiaoman weekly recruitment requires an executable Python interpreter" >&2
  exit 1
fi
if [[ ! -f "$WORKFLOW_PY" ]]; then
  echo "xiaoman weekly recruitment workflow is missing from release/current" >&2
  exit 1
fi

mkdir -p "$WORK_DIR"
chmod 0700 "$WORK_DIR"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
report_json="${tmp_dir}/weekly-recruitment.json"
review_message="${WORK_DIR}/latest-operator-review-message.txt"
summary_json="${WORK_DIR}/latest-summary.json"

args=("--mode" "weekly_recruitment_form" "--json")
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_DATE:-}" ]]; then
  args+=("--date" "$QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_DATE")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OPERATOR_NAME:-}" ]]; then
  args+=("--operator-name" "$QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OPERATOR_NAME")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_AUDIENCE:-}" ]]; then
  args+=("--audience" "$QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_AUDIENCE")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_FORM_LABEL:-}" ]]; then
  args+=("--form-label" "$QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_FORM_LABEL")
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
    raise SystemExit("xiaoman weekly recruitment worker received an unsuccessful report")
if report.get("mode") != "weekly_recruitment_form":
    raise SystemExit("xiaoman weekly recruitment worker received the wrong mode")
if report.get("workflow_step") != "weekly_recruitment_form":
    raise SystemExit("xiaoman weekly recruitment worker received the wrong workflow step")
if report.get("requires_human_confirmation") is not True:
    raise SystemExit("xiaoman weekly recruitment must keep human confirmation")
if report.get("external_send_executed") is not False:
    raise SystemExit("xiaoman weekly recruitment must not execute external send")
if report.get("safe_for_member_chat") is not False:
    raise SystemExit("xiaoman weekly recruitment must remain an operations-review draft")

message = str(report.get("operator_review_message") or "").strip()
if not message:
    raise SystemExit("xiaoman weekly recruitment did not return an operator review message")

message_path.write_text(message + "\n", encoding="utf-8")
os.chmod(message_path, 0o600)

summary = {
    "schema_version": 1,
    "worker": "xiaoman-weekly-recruitment-worker",
    "mode": report.get("mode"),
    "workflow_step": report.get("workflow_step"),
    "date": report.get("date"),
    "record_source": report.get("record_source"),
    "requires_human_confirmation": report.get("requires_human_confirmation"),
    "external_send_executed": report.get("external_send_executed"),
    "safe_for_member_chat": report.get("safe_for_member_chat"),
    "operator_review_message_path": str(message_path),
}
summary_path.write_text(json.dumps(summary, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
os.chmod(summary_path, 0o600)
print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
PY
