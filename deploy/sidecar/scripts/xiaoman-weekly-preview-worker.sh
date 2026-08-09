#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED:-0}" != "1" ]]; then
  echo "xiaoman weekly preview skipped: persistent enablement is not 1" >&2
  exit 0
fi

if [[ -v QINTOPIA_RELEASE_DIR || -v QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PYTHON || -v QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OUTPUT_DIR ]]; then
  echo "xiaoman weekly preview refuses runtime path overrides" >&2
  exit 1
fi

RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
PYTHON_BIN="/usr/bin/python3"
WORKFLOW_PY="${RELEASE_DIR}/workflows/xiaoman-weekly-preview/weekly_preview.py"
WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview"

require_env() {
  local key="$1"
  if [[ -z "${!key:-}" ]]; then
    echo "xiaoman weekly preview requires ${key}" >&2
    exit 1
  fi
}

require_env "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL"
require_env "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"
require_env "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL}" != "approved-production-xiaoman-weekly-preview" ]]; then
  echo "xiaoman weekly preview requires the reviewed production approval value" >&2
  exit 1
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE}" != "1" ]]; then
  echo "xiaoman weekly preview requires Xiaoman activity wrappers to be enabled" >&2
  exit 1
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE}" != "1" ]]; then
  echo "xiaoman weekly preview requires Xiaoman activity read-through to be enabled" >&2
  exit 1
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "xiaoman weekly preview requires an executable Python interpreter" >&2
  exit 1
fi
if [[ ! -f "$WORKFLOW_PY" ]]; then
  echo "xiaoman weekly preview workflow is missing from release/current" >&2
  exit 1
fi

mkdir -p "$WORK_DIR"
chmod 0700 "$WORK_DIR"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
report_json="${tmp_dir}/weekly-preview.json"
review_message="${WORK_DIR}/latest-operator-review-message.txt"
summary_json="${WORK_DIR}/latest-summary.json"

args=("--json")
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_DATE:-}" ]]; then
  args+=("--date" "$QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_DATE")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OPERATOR_NAME:-}" ]]; then
  args+=("--operator-name" "$QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OPERATOR_NAME")
fi
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_AUDIENCE:-}" ]]; then
  args+=("--audience" "$QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_AUDIENCE")
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
    raise SystemExit("xiaoman weekly preview worker received an unsuccessful report")
if report.get("mode") != "weekly_preview":
    raise SystemExit("xiaoman weekly preview worker received the wrong mode")
if report.get("requires_human_confirmation") is not True:
    raise SystemExit("xiaoman weekly preview must keep human confirmation")
if report.get("external_send_executed") is not False:
    raise SystemExit("xiaoman weekly preview must not execute external send")
if report.get("safe_for_member_chat") is not False:
    raise SystemExit("xiaoman weekly preview must remain an operations-review draft")

message = str(report.get("operator_review_message") or "").strip()
if not message:
    raise SystemExit("xiaoman weekly preview did not return an operator review message")

message_path.write_text(message + "\n", encoding="utf-8")
os.chmod(message_path, 0o600)

summary = {
    "schema_version": 1,
    "worker": "xiaoman-weekly-preview-worker",
    "mode": report.get("mode"),
    "workflow_step": report.get("workflow_step"),
    "week_start": report.get("week_start") or report.get("date"),
    "publishable_count": report.get("publishable_count", 0),
    "skipped_count": report.get("skipped_count", 0),
    "missing_followups_count": len(report.get("missing_followups") or []),
    "requires_human_confirmation": report.get("requires_human_confirmation"),
    "external_send_executed": report.get("external_send_executed"),
    "safe_for_member_chat": report.get("safe_for_member_chat"),
    "operator_review_message_path": str(message_path),
}
summary_path.write_text(json.dumps(summary, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
os.chmod(summary_path, 0o600)
print(json.dumps(summary, ensure_ascii=False, separators=(",", ":")))
PY
