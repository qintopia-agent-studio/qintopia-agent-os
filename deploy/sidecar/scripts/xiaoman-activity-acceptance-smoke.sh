#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
cd "$MONOREPO_ROOT"

FIXTURE_PATH="${QINTOPIA_XIAOMAN_ACTIVITY_FIXTURE_PATH:-${SIDECAR_DIR}/fixtures/xiaoman_activity_records.json}"
SIGNAL_FIXTURE_DIR="${QINTOPIA_XIAOMAN_SIGNAL_FIXTURE_DIR:-${MONOREPO_ROOT}/fixtures/xiaoman}"
if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi

run_json() {
  local operation="$1"
  local payload="$2"
  "${BIN_CMD[@]}" xiaoman-activity "$operation" \
    --payload-json "$payload" \
    --fixture-path "$FIXTURE_PATH" \
    --apply
}

fixture_input_json() {
  local fixture_name="$1"
  python3 -c 'import json, sys; print(json.dumps(json.load(open(sys.argv[1], encoding="utf-8"))["input"], ensure_ascii=False))' \
    "${SIGNAL_FIXTURE_DIR}/${fixture_name}"
}

run_signal_fixture_contract() {
  local fixture_name="$1"
  local signal_payload
  local signal_output
  signal_payload="$(fixture_input_json "$fixture_name")"
  signal_output="$("${BIN_CMD[@]}" xiaoman-activity signal-ingest --payload-json "$signal_payload" --dry-run)"
  CONTRACT_FIXTURE="${SIGNAL_FIXTURE_DIR}/${fixture_name}" SIGNAL_OUTPUT="$signal_output" python3 - <<'PY'
import json
import os

fixture = json.load(open(os.environ["CONTRACT_FIXTURE"], encoding="utf-8"))
expected = fixture["expected"]
payload = json.loads(os.environ["SIGNAL_OUTPUT"])

assert payload["success"] is True
assert payload["source"] == "agentos_event_signal"
assert payload["validation_status"] == expected["validation_status"]
assert payload["action_status"] == expected["action_status"]

work_item = payload["operations_work_item"]
assert work_item["capability_key"] == expected["capability_key"]
assert work_item["work_item_type"] == expected["work_item_type"]
assert work_item["requester_agent"] == expected["requester_agent"]
assert work_item["target_agent"] == expected["target_agent"]
assert work_item["idempotency_key"] == expected["idempotency_key"]

if expected["review_needed"]:
    for field in expected["missing_required_fields"]:
        assert any(field in item for item in payload["limitations"])
else:
    assert payload["validation_status"] == "ok"

raw = json.dumps(payload, ensure_ascii=False)
for message_id in fixture["input"].get("source_message_ids", []):
    assert message_id not in raw
if expected.get("external_sends") is False:
    assert "erhua.send_group_message" not in raw
assert "Dangerous command requires approval" not in raw
assert "Working" not in raw
assert "execute_code" not in raw
assert "terminal" not in raw
assert "skill_view" not in raw
PY
}

list_output="$(run_json list-by-date '{"date":"2026-06-28","table_role":"activity_plan","actor_agent":"xiaoman","operation":"list-by-date","dry_run":false}')"
LIST_OUTPUT="$list_output" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["LIST_OUTPUT"])
assert payload["success"] is True
assert payload["source"] == "fixture"
assert payload["action_status"] == "read_ok"
assert payload["record_count"] == 1
assert payload["records"][0]["record_ref"].startswith("activity_plan:")
raw = json.dumps(payload, ensure_ascii=False)
assert "rec_plan_20260628" not in raw
assert "Dangerous command requires approval" not in raw
assert "Working" not in raw
assert "execute_code" not in raw
assert "terminal" not in raw
assert "skill_view" not in raw
assert "lark-base" not in raw
PY

material_output="$(run_json material-summary '{"record_id":"rec_occurrence_20260628","table_role":"activity_occurrence","actor_agent":"xiaoman","operation":"material-summary","dry_run":false}')"
MATERIAL_OUTPUT="$material_output" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["MATERIAL_OUTPUT"])
assert payload["success"] is True
assert payload["action_status"] == "read_ok"
assert payload["record_count"] == 1
assert payload["records"][0]["material_summary"] == "现场照片 6 张，待筛选 2 张可用于复盘"
raw = json.dumps(payload, ensure_ascii=False)
assert "rec_occurrence_20260628" not in raw
assert "Dangerous command requires approval" not in raw
assert "Working" not in raw
assert "execute_code" not in raw
assert "terminal" not in raw
assert "skill_view" not in raw
assert "lark-base" not in raw
PY

run_signal_fixture_contract activity-signal.json
run_signal_fixture_contract duplicate-signal.json
run_signal_fixture_contract missing-fields-signal.json

set +e
feishu_error="$(
  "${BIN_CMD[@]}" xiaoman-activity record-get \
    --payload-json '{"record_id":"rec_plan_20260628","table_role":"activity_plan","actor_agent":"xiaoman","operation":"record-get","dry_run":false}' \
    --use-feishu-base \
    --apply 2>&1
)"
feishu_status=$?
set -e
if [[ "$feishu_status" -eq 0 ]]; then
  echo "expected Feishu Base read to fail without explicit allowlisted config" >&2
  exit 1
fi
if [[ "$feishu_error" != *"QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN is required"* ]]; then
  echo "unexpected Feishu Base config error: $feishu_error" >&2
  exit 1
fi

mutation_preview="$(
  "${BIN_CMD[@]}" xiaoman-activity status-update \
    --payload-json '{"event_signal_id":"66666666-6666-4666-8666-666666666666","mutation_id":"77777777-7777-4777-8777-777777777777","status":"处理中","actor_agent":"xiaoman","operation":"status-update"}' \
    --dry-run
)"
MUTATION_PREVIEW="$mutation_preview" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["MUTATION_PREVIEW"])
assert payload["success"] is True
assert payload["source"] == "agentos_event_signals"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["action_status"] == "event_signal_status_preview"
assert payload["mutation_applied"] is False
assert payload["safe_for_chat"] is False
raw = json.dumps(payload, ensure_ascii=False)
assert "Dangerous command requires approval" not in raw
assert "Working" not in raw
assert "execute_code" not in raw
assert "terminal" not in raw
assert "skill_view" not in raw
PY

wrapper_output="$(
  env \
    PYTHONDONTWRITEBYTECODE=1 \
    QINTOPIA_PROFILE_ID=xiaoman \
    QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1 \
    QINTOPIA_SIDECAR_BIN=/contract/qintopia-message-sidecar \
    python3 - "${MONOREPO_ROOT}/skills/qintopia-tools/variants/xiaoman/__init__.py" <<'PY'
import importlib.util
import json
import pathlib
import sys

plugin_path = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("xiaoman_activity_wrapper_contract", plugin_path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

status = json.loads(module.handle_qintopia_xiaoman_activity_status_update({
    "event_signal_id": "66666666-6666-4666-8666-666666666666",
    "mutation_id": "77777777-7777-4777-8777-777777777777",
    "status": "处理中",
}))
gap = json.loads(module.handle_qintopia_xiaoman_activity_gap_update({
    "event_signal_id": "66666666-6666-4666-8666-666666666666",
    "mutation_id": "88888888-8888-4888-8888-888888888888",
    "gap_summary": "缺少报名截止时间",
}))

for report, operation in ((status, "status-update"), (gap, "gap-update")):
    assert report["success"] is True
    assert report["dry_run"] is True
    assert report["action"]["command"][1:3] == ["xiaoman-activity", operation]
    assert report["action"]["command"][-1] == "--dry-run"
    assert "record_id" not in report["payload"]
    assert "table_role" not in report["payload"]

print(json.dumps({"status": status["payload"], "gap": gap["payload"]}, ensure_ascii=False))
PY
)"

wrapper_status_payload="$(
  WRAPPER_OUTPUT="$wrapper_output" python3 -c \
    'import json, os; print(json.dumps(json.loads(os.environ["WRAPPER_OUTPUT"])["status"], ensure_ascii=False))'
)"
wrapper_gap_payload="$(
  WRAPPER_OUTPUT="$wrapper_output" python3 -c \
    'import json, os; print(json.dumps(json.loads(os.environ["WRAPPER_OUTPUT"])["gap"], ensure_ascii=False))'
)"
wrapper_status_output="$(
  "${BIN_CMD[@]}" xiaoman-activity status-update \
    --payload-json "$wrapper_status_payload" \
    --dry-run
)"
wrapper_gap_output="$(
  "${BIN_CMD[@]}" xiaoman-activity gap-update \
    --payload-json "$wrapper_gap_payload" \
    --dry-run
)"

WRAPPER_STATUS_OUTPUT="$wrapper_status_output" \
WRAPPER_GAP_OUTPUT="$wrapper_gap_output" \
python3 - <<'PY'
import json
import os

status = json.loads(os.environ["WRAPPER_STATUS_OUTPUT"])
gap = json.loads(os.environ["WRAPPER_GAP_OUTPUT"])
assert status["success"] is True
assert status["action_status"] == "event_signal_status_preview"
assert gap["success"] is True
assert gap["action_status"] == "event_signal_gap_preview"
assert status["apply_requested"] is False
assert gap["apply_requested"] is False
PY

echo "xiaoman activity acceptance smoke passed"
