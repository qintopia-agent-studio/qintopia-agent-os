#!/usr/bin/env bash
set -euo pipefail

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEM_PYTHON="/usr/bin/python3"
HERMES_VENV="/home/ubuntu/.hermes/hermes-agent/venv"
DEFAULT_HERMES_PYTHON="/home/ubuntu/.hermes/hermes-agent/venv/bin/python"
PYTHON_BIN="${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PYTHON:-$DEFAULT_HERMES_PYTHON}"
WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview"

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED:-}" != "1" ]]; then
  echo "xiaoman_weekly_preview_worker_status=disabled"
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
WORKFLOW_PY="${RELEASE_DIR}/workflows/xiaoman-weekly-preview/weekly_preview.py"
PYTHON_VALIDATOR="${RELEASE_DIR}/runtime/hermes/validate_hermes_python.py"

fail() {
  echo "xiaoman weekly preview worker failed: $1" >&2
  exit 1
}

required_env() {
  local key="$1"
  local value="${!key:-}"
  if [[ -z "$value" ]]; then
    fail "missing ${key}"
  fi
}

for key in \
  QINTOPIA_DEPLOYED_COMMIT_SHA \
  QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE \
  QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE; do
  required_env "$key"
done

if [[ "$(basename "$RELEASE_DIR")" != "$QINTOPIA_DEPLOYED_COMMIT_SHA" ]]; then
  fail "release directory does not match deployed commit SHA"
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE}" != "1" ]]; then
  fail "Xiaoman activity wrappers are not enabled"
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE}" != "1" ]]; then
  fail "Xiaoman activity read-through is not enabled"
fi
if [[ ! -f "$WORKFLOW_PY" ]]; then
  fail "weekly preview workflow is missing from release/current"
fi
if [[ ! -f "$PYTHON_VALIDATOR" ]]; then
  fail "Hermes Python validator is missing from release/current"
fi
if [[ ! -x "$SYSTEM_PYTHON" ]]; then
  fail "system Python validator runner is missing"
fi

PYTHONDONTWRITEBYTECODE=1 "$SYSTEM_PYTHON" "$PYTHON_VALIDATOR" \
  --python "$PYTHON_BIN" \
  --venv-dir "$HERMES_VENV" \
  --release-dir "$RELEASE_DIR" >/dev/null

umask 077
mkdir -p "$WORK_DIR"
chmod 0700 "$WORK_DIR"
tmp_dir="$(mktemp -d "${WORK_DIR}/run.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

report_json="${tmp_dir}/weekly-preview.json"
operator_name="${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OPERATOR_NAME:-刘珊}"
audience="${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_AUDIENCE:-居民群}"
args=(--operator-name "$operator_name" --audience "$audience" --json)
if [[ -n "${QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_DATE:-}" ]]; then
  args=(--date "$QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_DATE" "${args[@]}")
fi

PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" "$WORKFLOW_PY" "${args[@]}" >"$report_json"

PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" - "$report_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)

success = report.get("success") is True
print(json.dumps({
    "success": success,
    "worker": "xiaoman-weekly-preview-worker",
    "workflow": "xiaoman-weekly-preview",
    "workflow_step": report.get("workflow_step"),
    "publishable_count": report.get("publishable_count"),
    "skipped_count": report.get("skipped_count"),
    "missing_followup_count": len(report.get("missing_followups") or []),
    "requires_human_confirmation": report.get("requires_human_confirmation"),
    "external_send_executed": report.get("external_send_executed"),
    "database_writes": False,
    "send_request_created": False,
    "safe_for_chat": False,
}, ensure_ascii=False, indent=2))

if not success:
    raise SystemExit(1)
PY
