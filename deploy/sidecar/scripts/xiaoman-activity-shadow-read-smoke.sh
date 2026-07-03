#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
cd "$MONOREPO_ROOT"

if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_ENABLE:-}" != "1" ]]; then
  echo "xiaoman activity shadow read smoke skipped: set QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_ENABLE=1 to run against Feishu Base" >&2
  exit 0
fi

required_env=(
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN
  QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PROFILE_ENV_PATH
)

for name in "${required_env[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    echo "missing required shadow env: $name" >&2
    exit 1
  fi
done

if [[ ",${QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS}," != *",${QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN},"* ]]; then
  echo "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN is not in the explicit allowlist" >&2
  exit 1
fi

SHADOW_DATE="${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_DATE:-$(date +%F)}"

run_json() {
  local operation="$1"
  local payload="$2"
  cargo run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" -- xiaoman-activity "$operation" \
    --payload-json "$payload" \
    --use-feishu-base \
    --apply
}

assert_shadow_output() {
  local label="$1"
  local output="$2"
  SHADOW_LABEL="$label" \
  SHADOW_OUTPUT="$output" \
  SHADOW_BASE_TOKEN="${QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN}" \
  SHADOW_PLAN_TABLE="${QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID}" \
  SHADOW_OCCURRENCE_TABLE="${QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID}" \
  SHADOW_PLAN_RECORD="${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_PLAN_RECORD_ID:-}" \
  SHADOW_OCCURRENCE_RECORD="${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_OCCURRENCE_RECORD_ID:-}" \
  python3 - <<'PY'
import json
import os

label = os.environ["SHADOW_LABEL"]
payload = json.loads(os.environ["SHADOW_OUTPUT"])
assert payload["success"] is True, label
assert payload["source"] == "feishu_base_read_only", label
assert payload["safe_for_chat"] is False, label
assert payload["validation_status"] == "ok", label
assert payload["action_status"] in {"read_ok", "record_not_found"}, label

raw = json.dumps(payload, ensure_ascii=False)
for forbidden in [
    "Dangerous command requires approval",
    "/approve",
    "Working",
    "execute_code",
    "terminal",
    "skill_view",
    "lark-base",
    "traceback",
    "Traceback",
    os.environ["SHADOW_BASE_TOKEN"],
    os.environ["SHADOW_PLAN_TABLE"],
    os.environ["SHADOW_OCCURRENCE_TABLE"],
    os.environ.get("SHADOW_PLAN_RECORD", ""),
    os.environ.get("SHADOW_OCCURRENCE_RECORD", ""),
]:
    if forbidden and forbidden in raw:
        raise AssertionError(f"{label}: forbidden output leaked: {forbidden}")

for record in payload.get("records", []):
    assert "record_ref" in record, label
    assert record["record_ref"].startswith(record["table_role"] + ":"), label
PY
}

plan_list_payload="$(python3 - <<PY
import json
print(json.dumps({
    "date": "${SHADOW_DATE}",
    "table_role": "activity_plan",
    "actor_agent": "xiaoman",
    "operation": "list-by-date",
    "dry_run": False,
}, ensure_ascii=False))
PY
)"
plan_output="$(run_json list-by-date "$plan_list_payload")"
assert_shadow_output "activity_plan list-by-date" "$plan_output"

occurrence_list_payload="$(python3 - <<PY
import json
print(json.dumps({
    "date": "${SHADOW_DATE}",
    "table_role": "activity_occurrence",
    "actor_agent": "xiaoman",
    "operation": "list-by-date",
    "dry_run": False,
}, ensure_ascii=False))
PY
)"
occurrence_output="$(run_json list-by-date "$occurrence_list_payload")"
assert_shadow_output "activity_occurrence list-by-date" "$occurrence_output"

if [[ -n "${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_PLAN_RECORD_ID:-}" ]]; then
  plan_get_payload="$(python3 - <<PY
import json
print(json.dumps({
    "record_id": "${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_PLAN_RECORD_ID}",
    "table_role": "activity_plan",
    "actor_agent": "xiaoman",
    "operation": "record-get",
    "dry_run": False,
}, ensure_ascii=False))
PY
)"
  plan_get_output="$(run_json record-get "$plan_get_payload")"
  assert_shadow_output "activity_plan record-get" "$plan_get_output"
fi

if [[ -n "${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_OCCURRENCE_RECORD_ID:-}" ]]; then
  occurrence_get_payload="$(python3 - <<PY
import json
print(json.dumps({
    "record_id": "${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_OCCURRENCE_RECORD_ID}",
    "table_role": "activity_occurrence",
    "actor_agent": "xiaoman",
    "operation": "record-get",
    "dry_run": False,
}, ensure_ascii=False))
PY
)"
  occurrence_get_output="$(run_json record-get "$occurrence_get_payload")"
  assert_shadow_output "activity_occurrence record-get" "$occurrence_get_output"

  material_payload="$(python3 - <<PY
import json
print(json.dumps({
    "record_id": "${QINTOPIA_XIAOMAN_ACTIVITY_SHADOW_OCCURRENCE_RECORD_ID}",
    "table_role": "activity_occurrence",
    "actor_agent": "xiaoman",
    "operation": "material-summary",
    "dry_run": False,
}, ensure_ascii=False))
PY
)"
  material_output="$(run_json material-summary "$material_payload")"
  assert_shadow_output "activity_occurrence material-summary" "$material_output"
fi

echo "xiaoman activity shadow read smoke passed for ${SHADOW_DATE}"
