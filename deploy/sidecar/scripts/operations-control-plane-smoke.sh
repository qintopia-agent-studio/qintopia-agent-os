#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

run_json() {
  local name="$1"
  shift
  local output="$tmp_dir/${name}.json"
  "${BIN_CMD[@]}" "$@" >"$output"
  python3 - "$output" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    json.load(fh)
PY
  printf '%s\n' "$output"
}

assert_json() {
  local file="$1"
  local expr="$2"
  python3 - "$file" "$expr" <<'PY'
import json
import sys

path, expr = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)
if not eval(expr, {"__builtins__": {}}, {"data": data, "any": any, "len": len}):
    print(f"assertion failed: {expr}", file=sys.stderr)
    print(json.dumps(data, ensure_ascii=False, indent=2), file=sys.stderr)
    sys.exit(1)
PY
}

run_expect_failure() {
  local name="$1"
  local expected="$2"
  shift 2
  local stdout="$tmp_dir/${name}.stdout"
  local stderr="$tmp_dir/${name}.stderr"
  if "${BIN_CMD[@]}" "$@" >"$stdout" 2>"$stderr"; then
    echo "expected command to fail: $name" >&2
    cat "$stdout" >&2
    cat "$stderr" >&2
    exit 1
  fi
  if ! grep -Fq "$expected" "$stdout" && ! grep -Fq "$expected" "$stderr"; then
    echo "expected failure output to contain: $expected" >&2
    echo "--- stdout ---" >&2
    cat "$stdout" >&2
    echo "--- stderr ---" >&2
    cat "$stderr" >&2
    exit 1
  fi
}

capabilities="$(run_json capabilities operations-capability-list)"
assert_json "$capabilities" "data['success'] is True"
assert_json "$capabilities" "data['capability_count'] == 4"
assert_json "$capabilities" "any(item['capability_key'] == 'huabaosi.create_visual_asset' for item in data['capabilities'])"
assert_json "$capabilities" "any(item['capability_key'] == 'erhua.send_group_message' and item['risk_level'] == 'high' for item in data['capabilities'])"

readiness_missing="$(run_json readiness_missing operations-readiness-check --profile production)"
assert_json "$readiness_missing" "data['success'] is False"
assert_json "$readiness_missing" "data['action_status'] == 'missing_required_configuration'"
assert_json "$readiness_missing" "'allowed_group_targets' in data['missing_required']"
assert_json "$readiness_missing" "'allowed_reviewers' in data['missing_required']"
assert_json "$readiness_missing" "'allowed_confirmers' in data['missing_required']"
assert_json "$readiness_missing" "'allowed_owners' in data['missing_required']"
assert_json "$readiness_missing" "'allowed_attachment_hosts' in data['missing_required']"

run_expect_failure \
  readiness_strict_missing \
  "operations readiness check failed" \
  operations-readiness-check \
  --profile production \
  --strict

readiness_apply="$(
  QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 run_json \
    readiness_apply \
    --database-url "postgres://example.invalid/qintopia" \
    operations-readiness-check \
    --profile apply_smoke \
    --strict
)"
assert_json "$readiness_apply" "data['success'] is True"
assert_json "$readiness_apply" "data['ready_for_apply_smoke'] is True"

readiness_bot_reviewer="$(
  QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES=ops_test_group \
  QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS=cli_xiaoman_app \
  QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS=human-confirmer \
  QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS=human-owner \
  QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS=example.com \
  run_json \
    readiness_bot_reviewer \
    --database-url "postgres://example.invalid/qintopia" \
    operations-readiness-check \
    --profile production
)"
assert_json "$readiness_bot_reviewer" "data['success'] is False"
assert_json "$readiness_bot_reviewer" "'allowed_reviewers' in data['missing_required']"

poster_plan="$(run_json poster_plan operations-request-plan --payload-json '{"actor_agent":"xiaoman","request_text":"请根据周末活动生成一张运营海报","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}')"
assert_json "$poster_plan" "data['action_status'] == 'planned'"
assert_json "$poster_plan" "data['work_item_request']['capability_key'] == 'huabaosi.create_visual_asset'"
assert_json "$poster_plan" "data['work_item_preview']['current_status'] == 'queued'"

poster_submit="$(run_json poster_submit operations-request-submit --dry-run --payload-json '{"actor_agent":"xiaoman","request_text":"请根据周末活动生成一张运营海报","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}')"
assert_json "$poster_submit" "data['action_status'] == 'dry_run_ok'"
assert_json "$poster_submit" "data['work_item_request']['capability_key'] == 'huabaosi.create_visual_asset'"
assert_json "$poster_submit" "data['work_item_result']['current_status'] == 'queued'"
assert_json "$poster_submit" "data['work_item_result']['dry_run'] is True"

evidence_plan="$(run_json evidence_plan operations-request-plan --payload-json '{"actor_agent":"xiaoman","request_text":"请让文渊阁整理这个活动的背景资料和证据","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}')"
assert_json "$evidence_plan" "data['action_status'] == 'planned'"
assert_json "$evidence_plan" "data['work_item_request']['capability_key'] == 'wenyuange.retrieve_evidence'"
assert_json "$evidence_plan" "data['work_item_request']['work_item_type'] == 'evidence_request'"

evidence_submit="$(run_json evidence_submit operations-request-submit --dry-run --payload-json '{"actor_agent":"xiaoman","request_text":"请让文渊阁整理这个活动的背景资料和证据","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}')"
assert_json "$evidence_submit" "data['action_status'] == 'dry_run_ok'"
assert_json "$evidence_submit" "data['work_item_request']['capability_key'] == 'wenyuange.retrieve_evidence'"
assert_json "$evidence_submit" "data['work_item_result']['review_policy'] == 'not_required'"

workflow_start="$(run_json workflow_start operations-workflow-start --dry-run --payload-json '{"actor_agent":"xiaoman","workflow_type":"activity_promotion","request_text":"请根据周末活动生成一张运营海报","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}')"
assert_json "$workflow_start" "data['action_status'] == 'dry_run_ok'"
assert_json "$workflow_start" "data['parent_work_item']['work_item_type'] == 'activity_promotion_request'"
assert_json "$workflow_start" "len(data['child_work_items']) == 2"
assert_json "$workflow_start" "data['child_work_items'][0]['work_item_type'] == 'evidence_request'"
assert_json "$workflow_start" "data['child_work_items'][0]['capability_key'] == 'wenyuange.retrieve_evidence'"
assert_json "$workflow_start" "data['child_work_items'][1]['work_item_type'] == 'visual_asset_request'"
assert_json "$workflow_start" "data['child_work_items'][1]['capability_key'] == 'huabaosi.create_visual_asset'"

send_clarify="$(run_json send_clarify operations-request-plan --payload-json '{"actor_agent":"xiaoman","request_text":"请让二花把活动海报发群","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}')"
assert_json "$send_clarify" "data['action_status'] == 'needs_clarification'"
assert_json "$send_clarify" "data['work_item_request'] is None"

visual_request="$(run_json visual_request operations-work-item-create --dry-run --payload-json '{"requester_agent":"xiaoman","target_agent":"huabaosi","capability_key":"huabaosi.create_visual_asset","work_item_type":"visual_asset_request","brief_summary":"周末共创晚餐活动运营海报","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}')"
assert_json "$visual_request" "data['action_status'] == 'dry_run_ok'"
assert_json "$visual_request" "data['current_status'] == 'queued'"
assert_json "$visual_request" "data['human_workbench']['provider'] == 'feishu_task'"

collaboration="$(run_json collaboration run-collaboration-worker --work-item-type visual_asset_request --once --dry-run --fixture-mode)"
assert_json "$collaboration" "data['action_status'] == 'fixture_dry_run_ok'"
assert_json "$collaboration" "data['artifact_previews'][0]['review_status'] == 'pending'"

evidence_worker="$(run_json evidence_worker run-evidence-worker --once --dry-run --fixture-mode)"
assert_json "$evidence_worker" "data['action_status'] == 'fixture_dry_run_ok'"
assert_json "$evidence_worker" "data['artifact_previews'][0]['artifact_type'] == 'evidence_summary'"
assert_json "$evidence_worker" "data['artifact_previews'][0]['review_status'] == 'not_required'"

review="$(run_json review operations-artifact-review-decision --dry-run --payload-json '{"artifact_id":"02dd5f47-81f8-4b8c-898d-b4c926fcf9b5","reviewer_id":"human-owner-1","decision":"approved","reason":"可用于活动宣发"}')"
assert_json "$review" "data['action_status'] == 'dry_run_ok'"
assert_json "$review" "data['review_status'] == 'approved'"

QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS=ops-reviewer run_expect_failure \
  non_allowlisted_reviewer \
  "reviewer_id is not allowed for artifact review decisions" \
  operations-artifact-review-decision \
  --dry-run \
  --payload-json '{"artifact_id":"02dd5f47-81f8-4b8c-898d-b4c926fcf9b5","reviewer_id":"human-owner-1","decision":"approved","reason":"可用于活动宣发"}'

run_expect_failure \
  bot_reviewer \
  "reviewer_id must be a human actor id" \
  operations-artifact-review-decision \
  --dry-run \
  --payload-json '{"artifact_id":"02dd5f47-81f8-4b8c-898d-b4c926fcf9b5","reviewer_id":"cli_xiaoman_app","decision":"approved","reason":"可用于活动宣发"}'

confirm="$(run_json confirm operations-group-message-confirm --dry-run --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","confirmer_id":"human-owner-1","decision":"confirmed","reason":"确认发送窗口和内容"}')"
assert_json "$confirm" "data['action_status'] == 'dry_run_ok'"
assert_json "$confirm" "data['current_status'] == 'queued'"
assert_json "$confirm" "data['send_executed'] is False"

QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS=ops-confirmer run_expect_failure \
  non_allowlisted_confirmer \
  "confirmer_id is not allowed for group message final confirmation" \
  operations-group-message-confirm \
  --dry-run \
  --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","confirmer_id":"human-owner-1","decision":"confirmed","reason":"确认发送窗口和内容"}'

run_expect_failure \
  bot_confirmer \
  "confirmer_id must be a human actor id" \
  operations-group-message-confirm \
  --dry-run \
  --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","confirmer_id":"cli_erhua_app","decision":"confirmed","reason":"确认发送窗口和内容"}'

send_worker="$(run_json send_worker run-group-message-send-worker --once --dry-run --fixture-mode)"
assert_json "$send_worker" "data['action_status'] == 'fixture_dry_run_ok'"
assert_json "$send_worker" "data['send_executed'] is False"

workbench="$(run_json workbench run-workbench-mirror-worker --once --dry-run --fixture-mode)"
assert_json "$workbench" "data['action_status'] == 'fixture_dry_run_ok'"
assert_json "$workbench" "data['provider'] == 'feishu_task_dry_run'"
assert_json "$workbench" "'payload' not in data['description']"
assert_json "$workbench" "data['sensitive_fields_redacted'] is True"

workbench_event="$(run_json workbench_event operations-workbench-event-record --dry-run --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","provider":"feishu_task","external_id":"task_fixture_1","external_event_id":"comment_fixture_1","event_type":"comment_added","actor_id":"human-owner-1","comment_text":"请把海报标题再收紧一点","source":"feishu_task_fixture"}')"
assert_json "$workbench_event" "data['action_status'] == 'dry_run_ok'"
assert_json "$workbench_event" "data['mutates_work_item_state'] is False"
assert_json "$workbench_event" "data['recommended_command'] is None"

workbench_review_event="$(run_json workbench_review_event operations-workbench-event-record --dry-run --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","artifact_id":"02dd5f47-81f8-4b8c-898d-b4c926fcf9b5","provider":"feishu_task","external_id":"task_fixture_1","external_event_id":"review_fixture_1","event_type":"review_decision_requested","actor_id":"human-owner-1","review_decision":"approved","comment_text":"审核通过","source":"feishu_task_fixture"}')"
assert_json "$workbench_review_event" "data['action_status'] == 'dry_run_ok'"
assert_json "$workbench_review_event" "data['recommended_command'] == 'operations-artifact-review-decision'"

workbench_confirm_event="$(run_json workbench_confirm_event operations-workbench-event-record --dry-run --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","provider":"feishu_task","external_id":"task_fixture_1","external_event_id":"confirm_fixture_1","event_type":"final_confirmation_requested","actor_id":"human-owner-1","confirmation_decision":"confirmed","comment_text":"确认发送","source":"feishu_task_fixture"}')"
assert_json "$workbench_confirm_event" "data['action_status'] == 'dry_run_ok'"
assert_json "$workbench_confirm_event" "data['recommended_command'] == 'operations-group-message-confirm'"

workbench_status_event="$(run_json workbench_status_event operations-workbench-event-record --dry-run --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","provider":"feishu_task","external_id":"task_fixture_1","external_event_id":"status_fixture_1","event_type":"status_change_requested","actor_id":"human-owner-1","requested_status":"cancelled","comment_text":"活动取消，停止继续执行","source":"feishu_task_fixture"}')"
assert_json "$workbench_status_event" "data['action_status'] == 'dry_run_ok'"
assert_json "$workbench_status_event" "data['recommended_command'] == 'operations-workbench-status-change'"

workbench_owner_event="$(run_json workbench_owner_event operations-workbench-event-record --dry-run --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","provider":"feishu_task","external_id":"task_fixture_1","external_event_id":"owner_fixture_1","event_type":"owner_changed","actor_id":"human-owner-1","comment_text":"改由运营 A 跟进","metadata":{"new_human_owner":"ops-owner-a"},"source":"feishu_task_fixture"}')"
assert_json "$workbench_owner_event" "data['action_status'] == 'dry_run_ok'"
assert_json "$workbench_owner_event" "data['recommended_command'] == 'operations-workbench-owner-change'"

run_expect_failure \
  bot_workbench_owner \
  "metadata.new_human_owner must be a human actor id" \
  operations-workbench-event-record \
  --dry-run \
  --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","provider":"feishu_task","external_id":"task_fixture_1","external_event_id":"owner_fixture_bot_1","event_type":"owner_changed","actor_id":"human-owner-1","comment_text":"不能改给应用身份","metadata":{"new_human_owner":"cli_huabaosi_app"},"source":"feishu_task_fixture"}'

workbench_attachment_event="$(run_json workbench_attachment_event operations-workbench-event-record --dry-run --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","provider":"feishu_task","external_id":"task_fixture_1","external_event_id":"attachment_fixture_1","event_type":"attachment_added","actor_id":"human-owner-1","comment_text":"补充活动现场图","metadata":{"attachment_title":"活动现场参考图","attachment_summary":"人工补充的视觉参考素材，待审核后使用。","attachment_uri":"https://example.com/workbench/attachment.png"},"source":"feishu_task_fixture"}')"
assert_json "$workbench_attachment_event" "data['action_status'] == 'dry_run_ok'"
assert_json "$workbench_attachment_event" "data['recommended_command'] == 'operations-workbench-attachment-add'"

run_expect_failure \
  workbench_event_process_without_db \
  "QINTOPIA_SIDECAR_DATABASE_URL is required" \
  operations-workbench-event-process \
  --event-id 02dd5f47-81f8-4b8c-898d-b4c926fcf9b5 \
  --dry-run

run_expect_failure \
  workbench_event_worker_without_db \
  "QINTOPIA_SIDECAR_DATABASE_URL is required" \
  run-workbench-event-worker \
  --once \
  --dry-run

run_expect_failure \
  workflow_sync_worker_without_db \
  "QINTOPIA_SIDECAR_DATABASE_URL is required" \
  run-workflow-sync-worker \
  --once \
  --dry-run

run_expect_failure \
  daily_digest_source \
  "source_type is not allowed for operations work items" \
  operations-work-item-create \
  --dry-run \
  --payload-json '{"requester_agent":"xiaoman","target_agent":"huabaosi","capability_key":"huabaosi.create_visual_asset","work_item_type":"visual_asset_request","brief_summary":"从日报生成海报","source_type":"daily_digest","source_refs":{"source_record_ref":"daily_digests.markdown"}}'

run_expect_failure \
  manual_source_without_record_ref \
  "source_refs.source_record_ref is required for this source_type" \
  operations-request-plan \
  --payload-json '{"actor_agent":"xiaoman","request_text":"请根据周末活动生成一张运营海报","source_type":"manual_request","source_refs":{}}'

run_expect_failure \
  event_signal_without_event_ref \
  "event_signal source requires event_signal_id" \
  operations-work-item-create \
  --dry-run \
  --payload-json '{"requester_agent":"xiaoman","target_agent":"huabaosi","capability_key":"huabaosi.create_visual_asset","work_item_type":"visual_asset_request","brief_summary":"活动信号触发海报","source_type":"event_signal","source_refs":{}}'

run_expect_failure \
  sensitive_work_item \
  "payload contains disallowed sensitive or raw internal content" \
  operations-work-item-create \
  --dry-run \
  --payload-json '{"requester_agent":"xiaoman","target_agent":"huabaosi","capability_key":"huabaosi.create_visual_asset","work_item_type":"visual_asset_request","brief_summary":"周末共创晚餐活动运营海报","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"},"payload":{"app_token":"secret"}}'

run_expect_failure \
  kanban_raw_handoff \
  "payload contains disallowed sensitive or raw internal content" \
  operations-work-item-create \
  --dry-run \
  --payload-json '{"requester_agent":"xiaoman","target_agent":"huabaosi","capability_key":"huabaosi.create_visual_asset","work_item_type":"visual_asset_request","brief_summary":"请通过 Hermes Kanban 把 raw private chat 直接交给画报司","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}'

run_expect_failure \
  non_allowlisted_group \
  "target group is not allowlisted for group message requests" \
  operations-work-item-create \
  --dry-run \
  --payload-json '{"requester_agent":"xiaoman","target_agent":"erhua","capability_key":"erhua.send_group_message","work_item_type":"group_message_request","brief_summary":"发送审核后的活动海报到测试群","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"},"payload":{"approved_artifact_id":"02dd5f47-81f8-4b8c-898d-b4c926fcf9b5","target_channel":"qiwe","target_group_alias":"unknown_group","message_text":"活动海报已审核，请发送。"}}'

run_expect_failure \
  cancel_without_reason \
  "reason is required when cancelling a group message request" \
  operations-group-message-confirm \
  --dry-run \
  --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","confirmer_id":"human-owner-1","decision":"cancelled"}'

run_expect_failure \
  sensitive_request_plan \
  "request plan payload contains disallowed sensitive or raw internal content" \
  operations-request-plan \
  --payload-json '{"actor_agent":"xiaoman","request_text":"请根据 app_token 生成活动海报","source_type":"manual_request","source_refs":{"source_record_ref":"activity_occurrence:test"}}'

run_expect_failure \
  sensitive_workbench_event \
  "workbench event contains disallowed sensitive or raw internal content" \
  operations-workbench-event-record \
  --dry-run \
  --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","provider":"feishu_task","external_id":"task_fixture_1","event_type":"comment_added","actor_id":"human-owner-1","comment_text":"这里包含 app_token"}'

echo "operations control-plane smoke passed"
