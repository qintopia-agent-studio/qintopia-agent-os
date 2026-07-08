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

signal_payload="$(fixture_input_json activity-signal.json)"
signal_output="$("${BIN_CMD[@]}" xiaoman-activity signal-ingest --payload-json "$signal_payload" --dry-run)"
SIGNAL_OUTPUT="$signal_output" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["SIGNAL_OUTPUT"])
assert payload["success"] is True
assert payload["source"] == "agentos_event_signal"
assert payload["action_status"] == "operations_dry_run_ok"
work_item = payload["operations_work_item"]
assert work_item["capability_key"] == "xiaoman.create_activity_request"
assert work_item["work_item_type"] == "activity_promotion_request"
assert work_item["requester_agent"] == "default"
assert work_item["target_agent"] == "xiaoman"
assert work_item["idempotency_key"] == "xiaoman_activity_signal:11111111-1111-4111-8111-111111111111"
raw = json.dumps(payload, ensure_ascii=False)
assert "22222222-2222-4222-8222-222222222222" not in raw
assert "erhua.send_group_message" not in raw
assert "Dangerous command requires approval" not in raw
assert "Working" not in raw
assert "execute_code" not in raw
assert "terminal" not in raw
assert "skill_view" not in raw
PY

duplicate_payload="$(fixture_input_json duplicate-signal.json)"
duplicate_output="$("${BIN_CMD[@]}" xiaoman-activity signal-ingest --payload-json "$duplicate_payload" --dry-run)"
DUPLICATE_OUTPUT="$duplicate_output" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["DUPLICATE_OUTPUT"])
assert payload["success"] is True
assert payload["operations_work_item"]["idempotency_key"] == "xiaoman_activity_signal:11111111-1111-4111-8111-111111111111"
assert payload["operations_work_item"]["capability_key"] == "xiaoman.create_activity_request"
assert payload["action_status"] == "operations_dry_run_ok"
PY

review_needed_payload="$(fixture_input_json missing-fields-signal.json)"
review_needed_output="$("${BIN_CMD[@]}" xiaoman-activity signal-ingest --payload-json "$review_needed_payload" --dry-run)"
REVIEW_NEEDED_OUTPUT="$review_needed_output" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["REVIEW_NEEDED_OUTPUT"])
assert payload["success"] is True
assert payload["validation_status"] == "review_needed"
assert payload["action_status"] == "review_needed"
assert payload["operations_work_item"]["capability_key"] == "xiaoman.create_activity_request"
assert any("signal_date" in item for item in payload["limitations"])
raw = json.dumps(payload, ensure_ascii=False)
assert "erhua.send_group_message" not in raw
assert "execute_code" not in raw
assert "terminal" not in raw
PY

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

write_output="$(
  "${BIN_CMD[@]}" xiaoman-activity status-update \
    --payload-json '{"record_id":"rec_plan_20260628","table_role":"activity_plan","status":"待人工确认","actor_agent":"xiaoman","operation":"status-update","dry_run":false}' \
    --apply
)"
WRITE_OUTPUT="$write_output" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["WRITE_OUTPUT"])
assert payload["success"] is True
assert payload["action_status"] == "apply_not_implemented"
assert payload["safe_for_chat"] is False
raw = json.dumps(payload, ensure_ascii=False)
assert "Dangerous command requires approval" not in raw
assert "Working" not in raw
assert "execute_code" not in raw
assert "terminal" not in raw
assert "skill_view" not in raw
PY

echo "xiaoman activity acceptance smoke passed"
