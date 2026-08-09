#!/usr/bin/env bash
set -euo pipefail

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEM_PYTHON="/usr/bin/python3"
HERMES_VENV="/home/ubuntu/.hermes/hermes-agent/venv"
DEFAULT_HERMES_PYTHON="/home/ubuntu/.hermes/hermes-agent/venv/bin/python"
PYTHON_BIN="${QINTOPIA_ERHUA_MORNING_BRIEF_PYTHON:-$DEFAULT_HERMES_PYTHON}"
WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/erhua-morning-brief"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
WORKFLOW_PY="${RELEASE_DIR}/workflows/erhua-morning-brief/morning_brief.py"
SIDECAR_BIN="${RELEASE_DIR}/sidecar/qintopia-message-sidecar"
PYTHON_VALIDATOR="${RELEASE_DIR}/runtime/hermes/validate_hermes_python.py"

fail() {
  echo "erhua morning brief worker failed: $1" >&2
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
  QINTOPIA_SIDECAR_DATABASE_URL \
  QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED \
  QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL \
  QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE \
  QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE; do
  required_env "$key"
done

export QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE
export QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE
export QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN="$SIDECAR_BIN"

if [[ "$(basename "$RELEASE_DIR")" != "$QINTOPIA_DEPLOYED_COMMIT_SHA" ]]; then
  fail "release directory does not match deployed commit SHA"
fi
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED}" != "1" ]]; then
  fail "Erhua morning brief is not enabled"
fi
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL}" != "approved-production-erhua-morning-brief" ]]; then
  fail "Erhua morning brief production approval is missing"
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE}" != "1" ]]; then
  fail "Xiaoman activity wrappers are not enabled"
fi
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE}" != "1" ]]; then
  fail "Xiaoman activity read-through is not enabled"
fi
if [[ ! -f "$WORKFLOW_PY" ]]; then
  fail "workflow is missing from release/current"
fi
if [[ ! -x "$SIDECAR_BIN" ]]; then
  fail "reviewed primary sidecar binary is missing"
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

report_json="${tmp_dir}/morning-brief.json"

PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" "$WORKFLOW_PY" \
  --sidecar-bin "$SIDECAR_BIN" \
  --prepare-artifact \
  --execute-artifact-create \
  --apply-artifact-create \
  --publish-plan \
  --json >"$report_json"

PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" - "$report_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)

artifact_stdout = ((report.get("artifact_create") or {}).get("stdout") or "").strip()
artifact = json.loads(artifact_stdout) if artifact_stdout else {}
success = report.get("success") is True and artifact.get("success") is True

print(json.dumps({
    "success": success,
    "worker": "erhua-morning-brief-worker",
    "workflow": report.get("workflow"),
    "brief_date": report.get("date"),
    "activity_publishable_count": report.get("activity_publishable_count"),
    "sunday_no_publishable_activity_followup": report.get(
        "sunday_no_publishable_activity_followup"
    ),
    "ai_news_item_count": report.get("ai_news_item_count"),
    "artifact_created": artifact.get("action_status") == "artifact_created",
    "artifact_id": artifact.get("artifact_id"),
    "work_item_id": artifact.get("work_item_id"),
    "artifact_type": artifact.get("artifact_type"),
    "review_status": artifact.get("review_status"),
    "content_hash": artifact.get("content_hash"),
    "idempotency_key": artifact.get("idempotency_key"),
    "requires_human_confirmation": report.get("requires_human_confirmation"),
    "database_writes": True,
    "external_send_executed": False,
    "send_request_created": False,
}, ensure_ascii=False, indent=2))

if not success:
    raise SystemExit(1)
PY
