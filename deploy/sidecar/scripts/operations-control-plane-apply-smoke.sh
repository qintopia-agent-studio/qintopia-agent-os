#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE:-}" != "1" ]]; then
  echo "operations control-plane apply smoke skipped: set QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 to write AgentOS test rows" >&2
  exit 0
fi

if [[ -z "${QINTOPIA_SIDECAR_DATABASE_URL:-}" ]]; then
  echo "QINTOPIA_SIDECAR_DATABASE_URL is required" >&2
  exit 1
fi

if ! command -v psql >/dev/null 2>&1; then
  echo "psql is required for operations control-plane apply smoke" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
cd "$MONOREPO_ROOT"
if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi
PSQL_CMD=(psql "$QINTOPIA_SIDECAR_DATABASE_URL" -v ON_ERROR_STOP=1 -X -q -t -A)

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

psql_value() {
  "${PSQL_CMD[@]}" -c "$1"
}

assert_sql_equals() {
  local name="$1"
  local expected="$2"
  local query="$3"
  local actual
  actual="$(psql_value "$query")"
  if [[ "$actual" != "$expected" ]]; then
    echo "SQL assertion failed for ${name}: expected '${expected}', got '${actual}'" >&2
    echo "query: ${query}" >&2
    exit 1
  fi
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

smoke_suffix="$(date -u +%Y%m%dT%H%M%SZ)-$$"
group_idempotency_key="operations-apply-smoke:${smoke_suffix}:group-message"
workbench_confirm_group_idempotency_key="operations-apply-smoke:${smoke_suffix}:workbench-confirm-group-message"
source_ref="operations-apply-smoke:${smoke_suffix}"
export QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES="${QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES:-community_activity_group}"
export QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS:-operations-apply-smoke-reviewer,operations-apply-smoke-reviewer-2}"
export QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS:-operations-apply-smoke-confirmer,operations-apply-smoke-reviewer}"
export QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS:-operations-apply-smoke-owner}"
export QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS="${QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS:-example.com}"
# The disposable apply smoke proves the disabled boundary. It must never contact an image
# provider or media service, even if a caller has unrelated local settings.
export QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0

"${BIN_CMD[@]}" migrate >/dev/null

run_expect_failure \
  non_allowlisted_reviewer \
  "reviewer_id is not allowed for artifact review decisions" \
  operations-artifact-review-decision \
  --dry-run \
  --payload-json '{"artifact_id":"02dd5f47-81f8-4b8c-898d-b4c926fcf9b5","reviewer_id":"operations-apply-smoke-unauthorized-reviewer","decision":"approved","reason":"should be rejected by apply smoke reviewer allowlist"}'

run_expect_failure \
  non_allowlisted_confirmer \
  "confirmer_id is not allowed for group message final confirmation" \
  operations-group-message-confirm \
  --dry-run \
  --payload-json '{"work_item_id":"12fd56fa-51a7-4637-829c-6bf77f35b3fb","confirmer_id":"operations-apply-smoke-unauthorized-confirmer","decision":"confirmed","reason":"should be rejected by apply smoke confirmer allowlist"}'

capabilities="$(run_json capabilities operations-capability-list --use-db)"
assert_json "$capabilities" "data['success'] is True"
assert_json "$capabilities" "data['capability_count'] >= 5"
assert_json "$capabilities" "any(item['capability_key'] == 'huabaosi.create_visual_asset' for item in data['capabilities'])"
assert_json "$capabilities" "any(item['capability_key'] == 'huabaosi.generate_image_asset' for item in data['capabilities'])"
assert_sql_equals \
  capability_seed_count \
  5 \
  "SELECT count(*) FROM qintopia_agent_os.capabilities WHERE capability_key IN ('huabaosi.create_visual_asset','huabaosi.generate_image_asset','erhua.send_group_message','wenyuange.retrieve_evidence','xiaoman.create_activity_request');"

xiaoman_signal_id="$(psql_value "SELECT gen_random_uuid();")"
xiaoman_signal_chat_id="operations-apply-smoke-chat-${smoke_suffix}"
xiaoman_signal_dedupe_key="operations-apply-smoke:${smoke_suffix}:xiaoman-signal"
psql_value "
INSERT INTO qintopia_agent_os.event_signals (
  id,
  platform,
  chat_id,
  signal_date,
  signal_type,
  title,
  summary,
  owner_name,
  owner_agent,
  priority,
  status,
  confidence,
  dedupe_key,
  extraction_version,
  metadata
) VALUES (
  '${xiaoman_signal_id}'::uuid,
  'qiwe',
  '${xiaoman_signal_chat_id}',
  DATE '2026-07-08',
  '活动/聚会',
  'AgentOS apply smoke 小满活动',
  'AgentOS apply smoke validates UUID-backed Xiaoman signal intake apply path',
  'operations-apply-smoke-owner',
  'xiaoman',
  '中',
  '待处理',
  0.99,
  '${xiaoman_signal_dedupe_key}',
  'operations_apply_smoke_v1',
  '{\"smoke_case\":\"xiaoman_uuid_signal\"}'::jsonb
) ON CONFLICT (platform, chat_id, signal_date, dedupe_key, extraction_version) DO NOTHING
RETURNING id;" >/dev/null
xiaoman_signal_payload="$(
  python3 - "$xiaoman_signal_id" <<'PY'
import json
import sys
signal_id = sys.argv[1]
print(json.dumps({
    "actor_agent": "xiaoman",
    "operation": "signal-ingest",
    "event_signal_id": signal_id,
    "signal_type": "活动/聚会",
    "activity_title": "AgentOS apply smoke 小满活动",
    "signal_date": "2026-07-08",
    "owner_name": "operations-apply-smoke-owner",
    "priority": "normal",
    "location": "AgentOS apply smoke fixture",
    "brief_summary": "AgentOS apply smoke validates Xiaoman signal intake apply path",
    "gap_summary": "No external send or visual asset should be created by signal-ingest itself."
}, ensure_ascii=False))
PY
)"
xiaoman_signal="$(run_json xiaoman_signal xiaoman-activity signal-ingest --apply --payload-json "$xiaoman_signal_payload")"
assert_json "$xiaoman_signal" "data['success'] is True"
assert_json "$xiaoman_signal" "data['source'] == 'agentos_event_signal'"
assert_json "$xiaoman_signal" "data['validation_status'] == 'ok'"
assert_json "$xiaoman_signal" "data['action_status'] == 'operations_created'"
assert_json "$xiaoman_signal" "data['operations_work_item']['capability_key'] == 'xiaoman.create_activity_request'"
assert_json "$xiaoman_signal" "data['operations_work_item']['work_item_type'] == 'activity_promotion_request'"
assert_json "$xiaoman_signal" "data['operations_work_item']['requester_agent'] == 'default'"
assert_json "$xiaoman_signal" "data['operations_work_item']['target_agent'] == 'xiaoman'"
assert_json "$xiaoman_signal" "data['operations_work_item']['idempotency_key'] == 'xiaoman_activity_signal:${xiaoman_signal_id}'"
assert_json "$xiaoman_signal" "data['operations_work_item']['existing'] is False"
assert_json "$xiaoman_signal" "data['safe_for_chat'] is False"

xiaoman_signal_work_item_id="$(
  python3 - "$xiaoman_signal" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["operations_work_item"]["work_item_id"])
PY
)"

assert_sql_equals \
  xiaoman_signal_work_item_created \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_signal_work_item_id}'::uuid AND capability_key = 'xiaoman.create_activity_request' AND requester_agent = 'default' AND target_agent = 'xiaoman' AND source_type = 'event_signal' AND source_event_signal_id = '${xiaoman_signal_id}'::uuid AND idempotency_key = 'xiaoman_activity_signal:${xiaoman_signal_id}';"

assert_sql_equals \
  xiaoman_signal_source_row_exists \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signals WHERE id = '${xiaoman_signal_id}'::uuid AND owner_agent = 'xiaoman' AND signal_type = '活动/聚会';"

assert_sql_equals \
  xiaoman_signal_created_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${xiaoman_signal_work_item_id}'::uuid AND event_type = 'created';"

assert_sql_equals \
  xiaoman_signal_did_not_create_children \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_signal_work_item_id}'::uuid;"

xiaoman_signal_again="$(run_json xiaoman_signal_again xiaoman-activity signal-ingest --apply --payload-json "$xiaoman_signal_payload")"
assert_json "$xiaoman_signal_again" "data['success'] is True"
assert_json "$xiaoman_signal_again" "data['action_status'] == 'operations_idempotent_existing'"
assert_json "$xiaoman_signal_again" "data['operations_work_item']['existing'] is True"
assert_json "$xiaoman_signal_again" "data['operations_work_item']['work_item_id'] == '${xiaoman_signal_work_item_id}'"

assert_sql_equals \
  xiaoman_signal_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE idempotency_key = 'xiaoman_activity_signal:${xiaoman_signal_id}';"

assert_sql_equals \
  xiaoman_signal_created_event_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${xiaoman_signal_work_item_id}'::uuid AND event_type = 'created';"

xiaoman_status_mutation_id="$(psql_value "SELECT gen_random_uuid();")"
xiaoman_status_mutation_payload="$(
  python3 - "$xiaoman_signal_id" "$xiaoman_status_mutation_id" <<'PY'
import json
import sys

print(json.dumps({
    "actor_agent": "xiaoman",
    "operation": "status-update",
    "event_signal_id": sys.argv[1],
    "mutation_id": sys.argv[2],
    "status": "处理中",
}, ensure_ascii=False))
PY
)"

xiaoman_status_preview="$(run_json xiaoman_status_preview xiaoman-activity status-update --dry-run --payload-json "$xiaoman_status_mutation_payload")"
assert_json "$xiaoman_status_preview" "data['success'] is True"
assert_json "$xiaoman_status_preview" "data['source'] == 'agentos_event_signals'"
assert_json "$xiaoman_status_preview" "data['action_status'] == 'event_signal_status_preview'"
assert_json "$xiaoman_status_preview" "data['dry_run'] is True"
assert_json "$xiaoman_status_preview" "data['mutation_applied'] is False"
assert_json "$xiaoman_status_preview" "'event_signal_id' not in data and 'mutation_id' not in data"
assert_json "$xiaoman_status_preview" "data['safe_for_chat'] is False"

xiaoman_status_apply="$(run_json xiaoman_status_apply xiaoman-activity status-update --apply --payload-json "$xiaoman_status_mutation_payload")"
assert_json "$xiaoman_status_apply" "data['success'] is True"
assert_json "$xiaoman_status_apply" "data['source'] == 'agentos_event_signals'"
assert_json "$xiaoman_status_apply" "data['action_status'] == 'event_signal_status_updated'"
assert_json "$xiaoman_status_apply" "data['dry_run'] is False"
assert_json "$xiaoman_status_apply" "data['apply_requested'] is True"
assert_json "$xiaoman_status_apply" "data['mutation_applied'] is True"
assert_json "$xiaoman_status_apply" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_status_updated_agentos_fact \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signals WHERE id = '${xiaoman_signal_id}'::uuid AND status = '处理中';"

assert_sql_equals \
  xiaoman_status_mutation_audited_once \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signal_mutations WHERE event_signal_id = '${xiaoman_signal_id}'::uuid AND mutation_id = '${xiaoman_status_mutation_id}'::uuid AND idempotency_key = 'xiaoman_event_signal:${xiaoman_signal_id}:${xiaoman_status_mutation_id}' AND operation = 'status-update' AND actor_agent = 'xiaoman' AND previous_value->>'status' = '待处理' AND new_value->>'status' = '处理中' AND metadata->>'feishu_write_executed' = 'false' AND metadata->>'external_send_executed' = 'false';"

xiaoman_status_again="$(run_json xiaoman_status_again xiaoman-activity status-update --apply --payload-json "$xiaoman_status_mutation_payload")"
assert_json "$xiaoman_status_again" "data['success'] is True"
assert_json "$xiaoman_status_again" "data['action_status'] == 'event_signal_mutation_idempotent_existing'"
assert_json "$xiaoman_status_again" "data['mutation_applied'] is False"

assert_sql_equals \
  xiaoman_status_mutation_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signal_mutations WHERE mutation_id = '${xiaoman_status_mutation_id}'::uuid;"

xiaoman_conflicting_mutation_payload="$(
  python3 - "$xiaoman_signal_id" "$xiaoman_status_mutation_id" <<'PY'
import json
import sys

print(json.dumps({
    "actor_agent": "xiaoman",
    "operation": "gap-update",
    "event_signal_id": sys.argv[1],
    "mutation_id": sys.argv[2],
    "gap_summary": "缺少冲突检查",
}, ensure_ascii=False))
PY
)"
run_expect_failure \
  xiaoman_conflicting_mutation \
  "mutation_id was already used for a different event-signal mutation" \
  xiaoman-activity gap-update --apply --payload-json "$xiaoman_conflicting_mutation_payload"

xiaoman_gap_mutation_id="$(psql_value "SELECT gen_random_uuid();")"
xiaoman_gap_mutation_payload="$(
  python3 - "$xiaoman_signal_id" "$xiaoman_gap_mutation_id" <<'PY'
import json
import sys

print(json.dumps({
    "actor_agent": "xiaoman",
    "operation": "gap-update",
    "event_signal_id": sys.argv[1],
    "mutation_id": sys.argv[2],
    "gap_summary": "缺少 报名截止时间",
}, ensure_ascii=False))
PY
)"

xiaoman_gap_apply="$(run_json xiaoman_gap_apply xiaoman-activity gap-update --apply --payload-json "$xiaoman_gap_mutation_payload")"
assert_json "$xiaoman_gap_apply" "data['success'] is True"
assert_json "$xiaoman_gap_apply" "data['source'] == 'agentos_event_signals'"
assert_json "$xiaoman_gap_apply" "data['action_status'] == 'event_signal_gap_updated'"
assert_json "$xiaoman_gap_apply" "data['mutation_applied'] is True"
assert_json "$xiaoman_gap_apply" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_gap_updated_agentos_fact \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signals WHERE id = '${xiaoman_signal_id}'::uuid AND gap_summary = '缺少 报名截止时间';"

assert_sql_equals \
  xiaoman_gap_mutation_audited_once \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signal_mutations WHERE event_signal_id = '${xiaoman_signal_id}'::uuid AND mutation_id = '${xiaoman_gap_mutation_id}'::uuid AND operation = 'gap-update' AND previous_value->>'gap_summary' = '' AND new_value->>'gap_summary' = '缺少 报名截止时间';"

xiaoman_gap_again="$(run_json xiaoman_gap_again xiaoman-activity gap-update --apply --payload-json "$xiaoman_gap_mutation_payload")"
assert_json "$xiaoman_gap_again" "data['success'] is True"
assert_json "$xiaoman_gap_again" "data['action_status'] == 'event_signal_mutation_idempotent_existing'"
assert_json "$xiaoman_gap_again" "data['mutation_applied'] is False"

assert_sql_equals \
  xiaoman_gap_mutation_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signal_mutations WHERE mutation_id = '${xiaoman_gap_mutation_id}'::uuid;"

assert_sql_equals \
  xiaoman_mutations_do_not_touch_feishu_publish_state \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.event_signals WHERE id = '${xiaoman_signal_id}'::uuid AND feishu_record_id IS NULL AND last_published_at IS NULL;"

xiaoman_worker_signal_id="$(psql_value "SELECT gen_random_uuid();")"
xiaoman_worker_signal_chat_id="operations-apply-smoke-worker-chat-${smoke_suffix}"
xiaoman_worker_signal_dedupe_key="operations-apply-smoke:${smoke_suffix}:xiaoman-worker-signal"
xiaoman_worker_source_message_id="$(psql_value "SELECT gen_random_uuid();")"
xiaoman_worker_platform_message_id="operations-apply-smoke-platform-message-${smoke_suffix}"
xiaoman_worker_sender_id="operations-apply-smoke-sender-${smoke_suffix}"
psql_value "
INSERT INTO qintopia_messages.messages (
  id,
  platform,
  message_id,
  event_id,
  chat_id,
  chat_type,
  sender_id,
  message_kind,
  text,
  sent_at,
  received_at,
  raw,
  processing_hints
) VALUES (
  '${xiaoman_worker_source_message_id}'::uuid,
  'qiwe',
  '${xiaoman_worker_platform_message_id}',
  'operations-apply-smoke-event-${smoke_suffix}',
  '${xiaoman_worker_signal_chat_id}',
  'group',
  '${xiaoman_worker_sender_id}',
  'text',
  'AgentOS worker apply smoke 小满活动将在周六举办，联系电话 13800138000，详情 https://example.com/private',
  TIMESTAMPTZ '2001-01-01 10:00:00+00',
  TIMESTAMPTZ '2001-01-01 10:00:01+00',
  '{}'::jsonb,
  '{}'::jsonb
) ON CONFLICT (platform, message_id) DO NOTHING
RETURNING id;" >/dev/null
psql_value "
INSERT INTO qintopia_agent_os.event_signals (
  id,
  platform,
  chat_id,
  signal_date,
  signal_type,
  title,
  summary,
  owner_name,
  owner_agent,
  priority,
  status,
  confidence,
  source_message_ids,
  source_window_start,
  source_window_end,
  dedupe_key,
  extraction_version,
  metadata
) VALUES (
  '${xiaoman_worker_signal_id}'::uuid,
  'qiwe',
  '${xiaoman_worker_signal_chat_id}',
  DATE '2001-01-01',
  '活动/聚会',
  'AgentOS worker apply smoke 小满活动',
  'AgentOS apply smoke validates Xiaoman activity signal worker runtime intake path',
  'operations-apply-smoke-owner',
  'xiaoman',
  '中',
  '待处理',
  0.99,
  ARRAY['${xiaoman_worker_source_message_id}'::uuid],
  TIMESTAMPTZ '2001-01-01 00:00:00+00',
  TIMESTAMPTZ '2001-01-02 00:00:00+00',
  '${xiaoman_worker_signal_dedupe_key}',
  'operations_apply_smoke_v1',
  '{\"smoke_case\":\"xiaoman_worker_signal\"}'::jsonb
) ON CONFLICT (platform, chat_id, signal_date, dedupe_key, extraction_version) DO NOTHING
RETURNING id;" >/dev/null

xiaoman_worker_preview="$(run_json xiaoman_worker_preview run-xiaoman-activity-signal-worker --check-only --batch-size 1)"
assert_json "$xiaoman_worker_preview" "data['success'] is True"
assert_json "$xiaoman_worker_preview" "data['worker'] == 'xiaoman-activity-signal-worker'"
assert_json "$xiaoman_worker_preview" "data['source'] == 'agentos_event_signals'"
assert_json "$xiaoman_worker_preview" "data['dry_run'] is True"
assert_json "$xiaoman_worker_preview" "data['check_only'] is True"
assert_json "$xiaoman_worker_preview" "data['action_status'] == 'signal_ingest_preview'"
assert_json "$xiaoman_worker_preview" "data['scanned_count'] == 1"
assert_json "$xiaoman_worker_preview" "data['work_items'][0]['capability_key'] == 'xiaoman.create_activity_request'"
assert_json "$xiaoman_worker_preview" "data['work_items'][0]['idempotency_key'] == 'xiaoman_activity_signal:${xiaoman_worker_signal_id}'"
assert_json "$xiaoman_worker_preview" "data['safe_for_chat'] is False"

xiaoman_worker_apply="$(run_json xiaoman_worker_apply run-xiaoman-activity-signal-worker --once --apply --batch-size 1)"
assert_json "$xiaoman_worker_apply" "data['success'] is True"
assert_json "$xiaoman_worker_apply" "data['worker'] == 'xiaoman-activity-signal-worker'"
assert_json "$xiaoman_worker_apply" "data['source'] == 'agentos_event_signals'"
assert_json "$xiaoman_worker_apply" "data['dry_run'] is False"
assert_json "$xiaoman_worker_apply" "data['apply_requested'] is True"
assert_json "$xiaoman_worker_apply" "data['action_status'] == 'signal_ingest_submitted'"
assert_json "$xiaoman_worker_apply" "data['scanned_count'] == 1"
assert_json "$xiaoman_worker_apply" "data['submitted_count'] == 1"
assert_json "$xiaoman_worker_apply" "data['work_items'][0]['capability_key'] == 'xiaoman.create_activity_request'"
assert_json "$xiaoman_worker_apply" "data['work_items'][0]['idempotency_key'] == 'xiaoman_activity_signal:${xiaoman_worker_signal_id}'"
assert_json "$xiaoman_worker_apply" "data['work_items'][0]['existing'] is False"
assert_json "$xiaoman_worker_apply" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_worker_created_one_work_item \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE source_event_signal_id = '${xiaoman_worker_signal_id}'::uuid AND capability_key = 'xiaoman.create_activity_request' AND requester_agent = 'default' AND target_agent = 'xiaoman' AND source_type = 'event_signal' AND idempotency_key = 'xiaoman_activity_signal:${xiaoman_worker_signal_id}';"

xiaoman_worker_again="$(run_json xiaoman_worker_again run-xiaoman-activity-signal-worker --once --apply --batch-size 1)"
assert_json "$xiaoman_worker_again" "data['success'] is True"
assert_json "$xiaoman_worker_again" "data['worker'] == 'xiaoman-activity-signal-worker'"
assert_json "$xiaoman_worker_again" "data['action_status'] in ('no_eligible_signals', 'signal_ingest_submitted')"
assert_json "$xiaoman_worker_again" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_worker_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE idempotency_key = 'xiaoman_activity_signal:${xiaoman_worker_signal_id}';"

xiaoman_worker_parent_id="$(
  psql_value "SELECT id FROM qintopia_agent_os.work_items WHERE idempotency_key = 'xiaoman_activity_signal:${xiaoman_worker_signal_id}';"
)"

xiaoman_promotion_preview="$(run_json xiaoman_promotion_preview run-xiaoman-activity-promotion-starter-worker --check-only --work-item-id "$xiaoman_worker_parent_id")"
assert_json "$xiaoman_promotion_preview" "data['success'] is True"
assert_json "$xiaoman_promotion_preview" "data['worker'] == 'xiaoman-activity-promotion-starter-worker'"
assert_json "$xiaoman_promotion_preview" "data['source'] == 'agentos_work_items'"
assert_json "$xiaoman_promotion_preview" "data['dry_run'] is True"
assert_json "$xiaoman_promotion_preview" "data['check_only'] is True"
assert_json "$xiaoman_promotion_preview" "data['action_status'] == 'activity_promotion_children_preview'"
assert_json "$xiaoman_promotion_preview" "data['requested_work_item_id'] == '${xiaoman_worker_parent_id}'"
assert_json "$xiaoman_promotion_preview" "data['scanned_count'] == 1"
assert_json "$xiaoman_promotion_preview" "data['missing_child_count'] == 2"
assert_json "$xiaoman_promotion_preview" "len(data['work_items']) == 2"
assert_json "$xiaoman_promotion_preview" "any(item['capability_key'] == 'wenyuange.retrieve_evidence' and item['work_item_type'] == 'evidence_request' for item in data['work_items'])"
assert_json "$xiaoman_promotion_preview" "any(item['capability_key'] == 'huabaosi.create_visual_asset' and item['work_item_type'] == 'visual_asset_request' for item in data['work_items'])"
assert_json "$xiaoman_promotion_preview" "data['safe_for_chat'] is False"

xiaoman_promotion_apply="$(run_json xiaoman_promotion_apply run-xiaoman-activity-promotion-starter-worker --once --apply --work-item-id "$xiaoman_worker_parent_id")"
assert_json "$xiaoman_promotion_apply" "data['success'] is True"
assert_json "$xiaoman_promotion_apply" "data['worker'] == 'xiaoman-activity-promotion-starter-worker'"
assert_json "$xiaoman_promotion_apply" "data['source'] == 'agentos_work_items'"
assert_json "$xiaoman_promotion_apply" "data['dry_run'] is False"
assert_json "$xiaoman_promotion_apply" "data['apply_requested'] is True"
assert_json "$xiaoman_promotion_apply" "data['action_status'] == 'activity_promotion_children_created'"
assert_json "$xiaoman_promotion_apply" "data['requested_work_item_id'] == '${xiaoman_worker_parent_id}'"
assert_json "$xiaoman_promotion_apply" "data['scanned_count'] == 1"
assert_json "$xiaoman_promotion_apply" "data['created_count'] == 2"
assert_json "$xiaoman_promotion_apply" "data['missing_child_count'] == 2"
assert_json "$xiaoman_promotion_apply" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_promotion_created_evidence_child \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'wenyuange.retrieve_evidence' AND work_item_type = 'evidence_request' AND idempotency_key = 'xiaoman_activity_promotion:${xiaoman_worker_parent_id}:evidence-child';"

assert_sql_equals \
  xiaoman_promotion_created_visual_child \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'huabaosi.create_visual_asset' AND work_item_type = 'visual_asset_request' AND idempotency_key = 'xiaoman_activity_promotion:${xiaoman_worker_parent_id}:visual-child';"

xiaoman_promotion_again="$(run_json xiaoman_promotion_again run-xiaoman-activity-promotion-starter-worker --once --apply --work-item-id "$xiaoman_worker_parent_id")"
assert_json "$xiaoman_promotion_again" "data['success'] is True"
assert_json "$xiaoman_promotion_again" "data['worker'] == 'xiaoman-activity-promotion-starter-worker'"
assert_json "$xiaoman_promotion_again" "data['action_status'] == 'no_eligible_activity_requests'"
assert_json "$xiaoman_promotion_again" "data['scanned_count'] == 0"
assert_json "$xiaoman_promotion_again" "data['missing_child_count'] == 0"
assert_json "$xiaoman_promotion_again" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_promotion_children_not_duplicated \
  2 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key IN ('wenyuange.retrieve_evidence','huabaosi.create_visual_asset');"

xiaoman_promotion_visual_child_id="$(
  psql_value "SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'huabaosi.create_visual_asset' AND work_item_type = 'visual_asset_request';"
)"

xiaoman_promotion_evidence_child_id="$(
  psql_value "SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'wenyuange.retrieve_evidence' AND work_item_type = 'evidence_request';"
)"

xiaoman_promotion_visual_waiting="$(run_json xiaoman_promotion_visual_waiting run-collaboration-worker --work-item-type visual_asset_request --once --work-item-id "$xiaoman_promotion_visual_child_id" --dry-run)"
assert_json "$xiaoman_promotion_visual_waiting" "data['success'] is True"
assert_json "$xiaoman_promotion_visual_waiting" "data['dry_run'] is True"
assert_json "$xiaoman_promotion_visual_waiting" "data['apply_requested'] is False"
assert_json "$xiaoman_promotion_visual_waiting" "data['action_status'] == 'waiting_for_evidence'"
assert_json "$xiaoman_promotion_visual_waiting" "data['work_item_id'] == '${xiaoman_promotion_visual_child_id}'"
assert_json "$xiaoman_promotion_visual_waiting" "len(data['artifact_ids']) == 0"
assert_json "$xiaoman_promotion_visual_waiting" "len(data['artifact_previews']) == 0"
assert_sql_equals \
  xiaoman_promotion_visual_waiting_keeps_work_item_queued \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_promotion_visual_child_id}'::uuid AND status = 'queued';"
assert_sql_equals \
  xiaoman_promotion_visual_waiting_creates_no_artifact \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${xiaoman_promotion_visual_child_id}'::uuid;"

xiaoman_promotion_evidence_dry_run="$(run_json xiaoman_promotion_evidence_dry_run run-evidence-worker --once --work-item-id "$xiaoman_promotion_evidence_child_id" --dry-run)"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['success'] is True"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['dry_run'] is True"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['apply_requested'] is False"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['fixture_mode'] is False"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['action_status'] == 'dry_run_ok'"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['work_item_id'] == '${xiaoman_promotion_evidence_child_id}'"
assert_json "$xiaoman_promotion_evidence_dry_run" "len(data['artifact_ids']) == 0"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['artifact_previews'][0]['artifact_type'] == 'evidence_summary'"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['retrieval_strategy'] == 'exact_source_messages'"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['source_message_count'] == 1"
assert_json "$xiaoman_promotion_evidence_dry_run" "data['safe_for_chat'] is False"
assert_sql_equals \
  xiaoman_promotion_evidence_dry_run_keeps_work_item_queued \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_promotion_evidence_child_id}'::uuid AND status = 'queued';"
assert_sql_equals \
  xiaoman_promotion_evidence_dry_run_creates_no_artifact \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${xiaoman_promotion_evidence_child_id}'::uuid;"

xiaoman_promotion_evidence="$(run_json xiaoman_promotion_evidence run-evidence-worker --once --work-item-id "$xiaoman_promotion_evidence_child_id" --apply)"
assert_json "$xiaoman_promotion_evidence" "data['success'] is True"
assert_json "$xiaoman_promotion_evidence" "data['action_status'] == 'evidence_artifact_created'"
assert_json "$xiaoman_promotion_evidence" "data['work_item_id'] == '${xiaoman_promotion_evidence_child_id}'"
assert_json "$xiaoman_promotion_evidence" "len(data['artifact_ids']) == 1"
assert_json "$xiaoman_promotion_evidence" "data['artifact_previews'][0]['artifact_type'] == 'evidence_summary'"
assert_json "$xiaoman_promotion_evidence" "data['artifact_previews'][0]['review_status'] == 'not_required'"
assert_json "$xiaoman_promotion_evidence" "data['retrieval_strategy'] == 'exact_source_messages'"
assert_json "$xiaoman_promotion_evidence" "data['source_message_count'] == 1"
assert_json "$xiaoman_promotion_evidence" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_promotion_created_evidence_summary \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${xiaoman_promotion_evidence_child_id}'::uuid AND artifact_type = 'evidence_summary' AND review_status = 'not_required';"

assert_sql_equals \
  xiaoman_promotion_evidence_uses_internal_source_uuid \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${xiaoman_promotion_evidence_child_id}'::uuid AND source_ids @> '[{\"message_uuid\":\"${xiaoman_worker_source_message_id}\"}]'::jsonb;"

assert_sql_equals \
  xiaoman_promotion_evidence_redacts_external_identifiers \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${xiaoman_promotion_evidence_child_id}'::uuid AND (content_text LIKE '%${xiaoman_worker_platform_message_id}%' OR content_text LIKE '%${xiaoman_worker_signal_chat_id}%' OR content_text LIKE '%${xiaoman_worker_sender_id}%' OR content_text LIKE '%13800138000%' OR content_text LIKE '%example.com%');"

xiaoman_promotion_visual_dry_run="$(run_json xiaoman_promotion_visual_dry_run run-collaboration-worker --work-item-type visual_asset_request --once --work-item-id "$xiaoman_promotion_visual_child_id" --dry-run)"
assert_json "$xiaoman_promotion_visual_dry_run" "data['success'] is True"
assert_json "$xiaoman_promotion_visual_dry_run" "data['dry_run'] is True"
assert_json "$xiaoman_promotion_visual_dry_run" "data['apply_requested'] is False"
assert_json "$xiaoman_promotion_visual_dry_run" "data['fixture_mode'] is False"
assert_json "$xiaoman_promotion_visual_dry_run" "data['action_status'] == 'dry_run_ok'"
assert_json "$xiaoman_promotion_visual_dry_run" "data['work_item_id'] == '${xiaoman_promotion_visual_child_id}'"
assert_json "$xiaoman_promotion_visual_dry_run" "len(data['artifact_ids']) == 0"
assert_json "$xiaoman_promotion_visual_dry_run" "data['artifact_previews'][0]['artifact_type'] == 'poster_brief'"
assert_sql_equals \
  xiaoman_promotion_visual_dry_run_keeps_work_item_queued \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_promotion_visual_child_id}'::uuid AND status = 'queued';"
assert_sql_equals \
  xiaoman_promotion_visual_dry_run_creates_no_artifact \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${xiaoman_promotion_visual_child_id}'::uuid;"

xiaoman_promotion_visual="$(run_json xiaoman_promotion_visual run-collaboration-worker --work-item-type visual_asset_request --once --work-item-id "$xiaoman_promotion_visual_child_id" --apply)"
assert_json "$xiaoman_promotion_visual" "data['success'] is True"
assert_json "$xiaoman_promotion_visual" "data['action_status'] == 'artifacts_created'"
assert_json "$xiaoman_promotion_visual" "data['work_item_id'] == '${xiaoman_promotion_visual_child_id}'"
assert_json "$xiaoman_promotion_visual" "len(data['artifact_ids']) == 1"
assert_json "$xiaoman_promotion_visual" "data['artifact_previews'][0]['artifact_type'] == 'poster_brief'"
assert_json "$xiaoman_promotion_visual" "data['artifact_previews'][0]['review_status'] == 'pending'"

xiaoman_promotion_artifact_id="$(
  python3 - "$xiaoman_promotion_visual" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["artifact_ids"][0])
PY
)"

xiaoman_promotion_review_payload="$(
  python3 - "$xiaoman_promotion_artifact_id" <<'PY'
import json
import sys
artifact_id = sys.argv[1]
print(json.dumps({
    "artifact_id": artifact_id,
    "reviewer_id": "operations-apply-smoke-reviewer",
    "decision": "approved",
    "reason": "xiaoman apply smoke approval; still does not publish or send",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"

xiaoman_promotion_review="$(run_json xiaoman_promotion_review operations-artifact-review-decision --apply --payload-json "$xiaoman_promotion_review_payload")"
assert_json "$xiaoman_promotion_review" "data['success'] is True"
assert_json "$xiaoman_promotion_review" "data['action_status'] == 'review_recorded'"
assert_json "$xiaoman_promotion_review" "data['artifact_id'] == '${xiaoman_promotion_artifact_id}'"
assert_json "$xiaoman_promotion_review" "data['work_item_id'] == '${xiaoman_promotion_visual_child_id}'"
assert_json "$xiaoman_promotion_review" "data['review_status'] == 'approved'"

xiaoman_image_preview="$(run_json xiaoman_image_preview run-xiaoman-activity-image-generation-starter-worker --check-only --work-item-id "$xiaoman_promotion_visual_child_id")"
assert_json "$xiaoman_image_preview" "data['success'] is True"
assert_json "$xiaoman_image_preview" "data['worker'] == 'xiaoman-activity-image-generation-starter-worker'"
assert_json "$xiaoman_image_preview" "data['source'] == 'agentos_work_items'"
assert_json "$xiaoman_image_preview" "data['dry_run'] is True"
assert_json "$xiaoman_image_preview" "data['check_only'] is True"
assert_json "$xiaoman_image_preview" "data['action_status'] == 'image_generation_requests_preview'"
assert_json "$xiaoman_image_preview" "data['scanned_count'] == 1"
assert_json "$xiaoman_image_preview" "len(data['work_items']) == 1"
assert_json "$xiaoman_image_preview" "data['work_items'][0]['capability_key'] == 'huabaosi.generate_image_asset'"
assert_json "$xiaoman_image_preview" "data['work_items'][0]['work_item_type'] == 'image_generation_request'"
assert_json "$xiaoman_image_preview" "data['safe_for_chat'] is False"

xiaoman_image_apply="$(run_json xiaoman_image_apply run-xiaoman-activity-image-generation-starter-worker --once --apply --work-item-id "$xiaoman_promotion_visual_child_id")"
assert_json "$xiaoman_image_apply" "data['success'] is True"
assert_json "$xiaoman_image_apply" "data['action_status'] == 'image_generation_requests_created'"
assert_json "$xiaoman_image_apply" "data['created_count'] == 1"
assert_json "$xiaoman_image_apply" "data['work_items'][0]['existing'] is False"

xiaoman_image_work_item_id="$(
  psql_value "SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_promotion_visual_child_id}'::uuid AND capability_key = 'huabaosi.generate_image_asset' AND work_item_type = 'image_generation_request';"
)"

assert_sql_equals \
  xiaoman_image_created_one_request \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_image_work_item_id}'::uuid AND status = 'queued' AND payload->>'approved_brief_artifact_id' = '${xiaoman_promotion_artifact_id}' AND payload->>'image_specification' = 'community_poster_1024x1024';"

xiaoman_image_worker_preview="$(run_json xiaoman_image_worker_preview run-huabaosi-image-generation-worker --once --work-item-id "$xiaoman_image_work_item_id" --dry-run)"
assert_json "$xiaoman_image_worker_preview" "data['success'] is True"
assert_json "$xiaoman_image_worker_preview" "data['dry_run'] is True"
assert_json "$xiaoman_image_worker_preview" "data['apply_requested'] is False"
assert_json "$xiaoman_image_worker_preview" "data['action_status'] == 'image_generation_preview'"
assert_json "$xiaoman_image_worker_preview" "data['artifact_preview']['artifact_type'] == 'generated_image'"
assert_json "$xiaoman_image_worker_preview" "data['artifact_preview']['review_status'] == 'pending'"
assert_json "$xiaoman_image_worker_preview" "data['safe_for_chat'] is False"

xiaoman_image_worker_disabled="$(run_json xiaoman_image_worker_disabled run-huabaosi-image-generation-worker --once --work-item-id "$xiaoman_image_work_item_id" --apply)"
assert_json "$xiaoman_image_worker_disabled" "data['success'] is True"
assert_json "$xiaoman_image_worker_disabled" "data['dry_run'] is False"
assert_json "$xiaoman_image_worker_disabled" "data['apply_requested'] is True"
assert_json "$xiaoman_image_worker_disabled" "data['action_status'] == 'image_generation_disabled'"
assert_json "$xiaoman_image_worker_disabled" "data['artifact_preview']['review_status'] == 'pending'"
assert_json "$xiaoman_image_worker_disabled" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_image_disabled_does_not_write_artifact \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${xiaoman_image_work_item_id}'::uuid AND artifact_type = 'generated_image';"
assert_sql_equals \
  xiaoman_image_disabled_keeps_request_queued \
  queued \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_image_work_item_id}'::uuid;"

# A refused loopback connection proves the retry state machine without contacting an
# external provider or reaching the media upload stage.
export QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1
export QINTOPIA_HUABAOSI_IMAGE_PROVIDER=openai-compatible
export QINTOPIA_HUABAOSI_IMAGE_MODEL=gpt-image-2
export QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL=https://127.0.0.1:1/v1
export QINTOPIA_HUABAOSI_IMAGE_API_KEY=apply-smoke-test-key
export QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT=https://127.0.0.1:1/upload
export QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL=https://127.0.0.1:1/public
export QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=127.0.0.1

xiaoman_image_retry_scheduled="$(run_json xiaoman_image_retry_scheduled run-huabaosi-image-generation-worker --once --work-item-id "$xiaoman_image_work_item_id" --apply)"
assert_json "$xiaoman_image_retry_scheduled" "data['success'] is False"
assert_json "$xiaoman_image_retry_scheduled" "data['action_status'] == 'image_generation_retry_scheduled'"
assert_json "$xiaoman_image_retry_scheduled" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_image_retry_requeues_request \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_image_work_item_id}'::uuid AND status = 'queued' AND attempts = 1 AND available_at > now() AND claimed_by IS NULL AND last_error = 'retryable image provider failure; retry scheduled';"

assert_sql_equals \
  xiaoman_image_retry_event_is_sanitized \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${xiaoman_image_work_item_id}'::uuid AND event_type = 'image_generation_retry_scheduled' AND data->>'attempt_number' = '1' AND data->>'max_attempts' = '3' AND data->>'failure_class' = 'retryable_provider' AND data->>'failure_stage' = 'provider_transport' AND data->>'retry_delay_seconds' = '60' AND data->>'retry_scheduled' = 'true' AND data->>'retry_exhausted' = 'false' AND data->>'sensitive_fields_redacted' = 'true' AND data->>'external_publish_executed' = 'false';"

psql_value "UPDATE qintopia_agent_os.work_items SET attempts = 2, available_at = now() WHERE id = '${xiaoman_image_work_item_id}'::uuid;" >/dev/null
xiaoman_image_retry_exhausted="$(run_json xiaoman_image_retry_exhausted run-huabaosi-image-generation-worker --once --work-item-id "$xiaoman_image_work_item_id" --apply)"
assert_json "$xiaoman_image_retry_exhausted" "data['success'] is False"
assert_json "$xiaoman_image_retry_exhausted" "data['action_status'] == 'image_generation_retry_exhausted'"
assert_json "$xiaoman_image_retry_exhausted" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_image_retry_exhaustion_is_terminal \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_image_work_item_id}'::uuid AND status = 'failed' AND attempts = 3 AND claimed_by IS NULL AND last_error = 'retryable image provider failure; retry attempts exhausted';"

assert_sql_equals \
  xiaoman_image_retry_exhaustion_is_sanitized \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${xiaoman_image_work_item_id}'::uuid AND event_type = 'failed' AND data->>'attempt_number' = '3' AND data->>'max_attempts' = '3' AND data->>'failure_class' = 'retryable_provider' AND data->>'failure_stage' = 'provider_transport' AND data->>'retry_scheduled' = 'false' AND data->>'retry_exhausted' = 'true' AND data->>'sensitive_fields_redacted' = 'true' AND data->>'external_publish_executed' = 'false';"

export QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0

# The network adapter is separately covered by a local fake provider/media server. This
# disposable database fixture proves that only a reviewed generated_image can unlock the
# downstream send-request path while production generation remains disabled here.
xiaoman_generated_image_id="$(psql_value "SELECT gen_random_uuid();")"
psql_value "
UPDATE qintopia_agent_os.work_items
SET status = 'awaiting_review', updated_at = now()
WHERE id = '${xiaoman_image_work_item_id}'::uuid;

INSERT INTO qintopia_agent_os.artifacts (
  id, work_item_id, artifact_type, review_status, created_by_agent, title, summary,
  artifact_uri, content_hash, source_ids, risk_labels, information_class, metadata,
  review_requested_at
) VALUES (
  '${xiaoman_generated_image_id}'::uuid,
  '${xiaoman_image_work_item_id}'::uuid,
  'generated_image',
  'pending',
  'huabaosi',
  'Apply smoke generated image',
  'Disposable fixture for reviewed image downstream dependency.',
  'https://media.example.test/apply-smoke/${smoke_suffix}.png',
  'sha256:apply-smoke-${smoke_suffix}',
  jsonb_build_array(jsonb_build_object('approved_brief_artifact_id', '${xiaoman_promotion_artifact_id}')),
  ARRAY['external_use_review_required','generated_media']::text[],
  'internal_ops',
  jsonb_build_object('fixture_mode', true, 'external_publish_executed', false),
  now()
);"

xiaoman_generated_image_review_payload="$(
  python3 - "$xiaoman_generated_image_id" <<'PY'
import json
import sys
artifact_id = sys.argv[1]
print(json.dumps({
    "artifact_id": artifact_id,
    "reviewer_id": "operations-apply-smoke-reviewer",
    "decision": "approved",
    "reason": "disposable fixture approval; does not send or publish",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"

xiaoman_generated_image_review="$(run_json xiaoman_generated_image_review operations-artifact-review-decision --apply --payload-json "$xiaoman_generated_image_review_payload")"
assert_json "$xiaoman_generated_image_review" "data['success'] is True"
assert_json "$xiaoman_generated_image_review" "data['artifact_id'] == '${xiaoman_generated_image_id}'"
assert_json "$xiaoman_generated_image_review" "data['review_status'] == 'approved'"
assert_sql_equals \
  xiaoman_generated_image_review_completes_request \
  completed \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_image_work_item_id}'::uuid;"

xiaoman_image_again="$(run_json xiaoman_image_again run-xiaoman-activity-image-generation-starter-worker --once --apply --work-item-id "$xiaoman_promotion_visual_child_id")"
assert_json "$xiaoman_image_again" "data['success'] is True"
assert_json "$xiaoman_image_again" "data['action_status'] == 'no_eligible_approved_visual_artifacts'"
assert_json "$xiaoman_image_again" "data['scanned_count'] == 0"

assert_sql_equals \
  xiaoman_image_request_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_promotion_visual_child_id}'::uuid AND capability_key = 'huabaosi.generate_image_asset' AND work_item_type = 'image_generation_request';"

xiaoman_send_preview="$(run_json xiaoman_send_preview run-xiaoman-activity-send-request-starter-worker --check-only --work-item-id "$xiaoman_worker_parent_id")"
assert_json "$xiaoman_send_preview" "data['success'] is True"
assert_json "$xiaoman_send_preview" "data['worker'] == 'xiaoman-activity-send-request-starter-worker'"
assert_json "$xiaoman_send_preview" "data['source'] == 'agentos_work_items'"
assert_json "$xiaoman_send_preview" "data['dry_run'] is True"
assert_json "$xiaoman_send_preview" "data['check_only'] is True"
assert_json "$xiaoman_send_preview" "data['action_status'] == 'group_message_requests_preview'"
assert_json "$xiaoman_send_preview" "data['requested_work_item_id'] == '${xiaoman_worker_parent_id}'"
assert_json "$xiaoman_send_preview" "data['scanned_count'] == 1"
assert_json "$xiaoman_send_preview" "data['missing_child_count'] == 1"
assert_json "$xiaoman_send_preview" "len(data['work_items']) == 1"
assert_json "$xiaoman_send_preview" "data['work_items'][0]['capability_key'] == 'erhua.send_group_message'"
assert_json "$xiaoman_send_preview" "data['work_items'][0]['work_item_type'] == 'group_message_request'"
assert_json "$xiaoman_send_preview" "data['work_items'][0]['current_status'] == 'awaiting_publish'"
assert_json "$xiaoman_send_preview" "data['safe_for_chat'] is False"

xiaoman_send_apply="$(run_json xiaoman_send_apply run-xiaoman-activity-send-request-starter-worker --once --apply --work-item-id "$xiaoman_worker_parent_id")"
assert_json "$xiaoman_send_apply" "data['success'] is True"
assert_json "$xiaoman_send_apply" "data['worker'] == 'xiaoman-activity-send-request-starter-worker'"
assert_json "$xiaoman_send_apply" "data['source'] == 'agentos_work_items'"
assert_json "$xiaoman_send_apply" "data['dry_run'] is False"
assert_json "$xiaoman_send_apply" "data['apply_requested'] is True"
assert_json "$xiaoman_send_apply" "data['action_status'] == 'group_message_requests_created'"
assert_json "$xiaoman_send_apply" "data['requested_work_item_id'] == '${xiaoman_worker_parent_id}'"
assert_json "$xiaoman_send_apply" "data['scanned_count'] == 1"
assert_json "$xiaoman_send_apply" "data['created_count'] == 1"
assert_json "$xiaoman_send_apply" "data['missing_child_count'] == 1"
assert_json "$xiaoman_send_apply" "data['work_items'][0]['current_status'] == 'awaiting_publish'"
assert_json "$xiaoman_send_apply" "data['work_items'][0]['existing'] is False"
assert_json "$xiaoman_send_apply" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_send_created_group_child \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'erhua.send_group_message' AND work_item_type = 'group_message_request' AND status = 'awaiting_publish' AND idempotency_key = 'xiaoman_activity_promotion:${xiaoman_worker_parent_id}:group-message-child' AND (payload->>'approved_artifact_id')::uuid = '${xiaoman_generated_image_id}'::uuid AND payload->>'approved_artifact_type' = 'generated_image' AND (payload->>'image_generation_work_item_id')::uuid = '${xiaoman_image_work_item_id}'::uuid AND payload->>'send_executed' = 'false';"

assert_sql_equals \
  xiaoman_send_did_not_send_or_queue \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id IN (SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'erhua.send_group_message') AND event_type IN ('group_message_final_confirmation_recorded','group_message_send_ready_recorded','send_executed','external_published');"

xiaoman_send_again="$(run_json xiaoman_send_again run-xiaoman-activity-send-request-starter-worker --once --apply --work-item-id "$xiaoman_worker_parent_id")"
assert_json "$xiaoman_send_again" "data['success'] is True"
assert_json "$xiaoman_send_again" "data['worker'] == 'xiaoman-activity-send-request-starter-worker'"
assert_json "$xiaoman_send_again" "data['action_status'] == 'no_eligible_approved_generated_images'"
assert_json "$xiaoman_send_again" "data['scanned_count'] == 0"
assert_json "$xiaoman_send_again" "data['missing_child_count'] == 0"
assert_json "$xiaoman_send_again" "data['safe_for_chat'] is False"

assert_sql_equals \
  xiaoman_send_group_child_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'erhua.send_group_message' AND work_item_type = 'group_message_request';"

xiaoman_group_work_item_id="$(
  psql_value "SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${xiaoman_worker_parent_id}'::uuid AND capability_key = 'erhua.send_group_message' AND work_item_type = 'group_message_request';"
)"

xiaoman_group_confirm_payload="$(
  python3 - "$xiaoman_group_work_item_id" <<'PY'
import json
import sys
work_item_id = sys.argv[1]
print(json.dumps({
    "work_item_id": work_item_id,
    "confirmer_id": "operations-apply-smoke-confirmer",
    "decision": "confirmed",
    "reason": "xiaoman apply smoke final confirmation; send worker must not send",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"

xiaoman_group_confirm="$(run_json xiaoman_group_confirm operations-group-message-confirm --apply --payload-json "$xiaoman_group_confirm_payload")"
assert_json "$xiaoman_group_confirm" "data['success'] is True"
assert_json "$xiaoman_group_confirm" "data['action_status'] == 'confirmation_recorded'"
assert_json "$xiaoman_group_confirm" "data['work_item_id'] == '${xiaoman_group_work_item_id}'"
assert_json "$xiaoman_group_confirm" "data['previous_status'] == 'awaiting_publish'"
assert_json "$xiaoman_group_confirm" "data['current_status'] == 'queued'"
assert_json "$xiaoman_group_confirm" "data['send_executed'] is False"

assert_sql_equals \
  xiaoman_group_confirmation_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${xiaoman_group_work_item_id}'::uuid AND event_type = 'group_message_final_confirmation_recorded' AND data->>'send_executed' = 'false';"

xiaoman_send_ready="$(run_json xiaoman_send_ready run-group-message-send-worker --once --work-item-id "$xiaoman_group_work_item_id" --apply)"
assert_json "$xiaoman_send_ready" "data['success'] is True"
assert_json "$xiaoman_send_ready" "data['action_status'] == 'send_ready_recorded'"
assert_json "$xiaoman_send_ready" "data['work_item_id'] == '${xiaoman_group_work_item_id}'"
assert_json "$xiaoman_send_ready" "data['send_executed'] is False"
assert_json "$xiaoman_send_ready" "data['approved_artifact_id'] == '${xiaoman_generated_image_id}'"

assert_sql_equals \
  xiaoman_group_send_ready_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${xiaoman_group_work_item_id}'::uuid AND event_type = 'group_message_send_ready_recorded' AND data->>'send_executed' = 'false';"

assert_sql_equals \
  xiaoman_group_request_still_queued_after_send_ready \
  queued \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${xiaoman_group_work_item_id}'::uuid;"

xiaoman_send_ready_again="$(run_json xiaoman_send_ready_again run-group-message-send-worker --once --work-item-id "$xiaoman_group_work_item_id" --apply)"
assert_json "$xiaoman_send_ready_again" "data['success'] is True"
assert_json "$xiaoman_send_ready_again" "data['action_status'] == 'no_claimable_group_message_request'"
assert_json "$xiaoman_send_ready_again" "data['send_executed'] is False"

assert_sql_equals \
  xiaoman_group_send_ready_event_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${xiaoman_group_work_item_id}'::uuid AND event_type = 'group_message_send_ready_recorded' AND data->>'send_executed' = 'false';"

xiaoman_status_tree="$(run_json xiaoman_status_tree operations-work-item-status --work-item-id "$xiaoman_worker_parent_id")"
assert_json "$xiaoman_status_tree" "data['success'] is True"
assert_json "$xiaoman_status_tree" "data['root_work_item_id'] == '${xiaoman_worker_parent_id}'"
assert_json "$xiaoman_status_tree" "data['child_count'] == 3"
assert_json "$xiaoman_status_tree" "any(item['work_item_id'] == '${xiaoman_promotion_evidence_child_id}' and item['work_item_type'] == 'evidence_request' and item['status'] == 'completed' for item in data['children'])"
assert_json "$xiaoman_status_tree" "any(item['work_item_id'] == '${xiaoman_promotion_visual_child_id}' and item['work_item_type'] == 'visual_asset_request' for item in data['children'])"
assert_json "$xiaoman_status_tree" "any(item['work_item_id'] == '${xiaoman_group_work_item_id}' and item['work_item_type'] == 'group_message_request' and item['status'] == 'queued' for item in data['children'])"
assert_json "$xiaoman_status_tree" "data['current_blocking_point'] == 'group_message_request:send_ready_waiting_for_production_send_adapter'"

planned_submit_payload="$(
  python3 - "$source_ref" <<'PY'
import json
import sys
source_ref = sys.argv[1]
print(json.dumps({
    "actor_agent": "xiaoman",
    "request_text": "请根据 AgentOS apply smoke 活动生成一张运营海报",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref + ":planned-submit"},
    "metadata": {"smoke_case": "request_submit"},
}, ensure_ascii=False))
PY
)"
planned_submit="$(run_json planned_submit operations-request-submit --apply --payload-json "$planned_submit_payload")"
assert_json "$planned_submit" "data['success'] is True"
assert_json "$planned_submit" "data['action_status'] == 'created'"
assert_json "$planned_submit" "data['work_item_result']['capability_key'] == 'huabaosi.create_visual_asset'"
assert_json "$planned_submit" "data['work_item_result']['current_status'] == 'queued'"
assert_json "$planned_submit" "data['work_item_result']['apply_requested'] is True"

planned_submit_again="$(run_json planned_submit_again operations-request-submit --apply --payload-json "$planned_submit_payload")"
assert_json "$planned_submit_again" "data['success'] is True"
assert_json "$planned_submit_again" "data['action_status'] == 'idempotent_existing'"
assert_json "$planned_submit_again" "data['work_item_result']['existing'] is True"

workflow_start_payload="$(
  python3 - "$source_ref" <<'PY'
import json
import sys
source_ref = sys.argv[1]
print(json.dumps({
    "actor_agent": "xiaoman",
    "workflow_type": "activity_promotion",
    "request_text": "请根据 AgentOS apply smoke 活动启动宣发 workflow",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref + ":workflow-start"},
    "idempotency_key": source_ref + ":workflow-start",
    "metadata": {"smoke_case": "workflow_start"},
}, ensure_ascii=False))
PY
)"
workflow_start="$(run_json workflow_start operations-workflow-start --apply --payload-json "$workflow_start_payload")"
assert_json "$workflow_start" "data['success'] is True"
assert_json "$workflow_start" "data['action_status'] == 'created'"
assert_json "$workflow_start" "data['parent_work_item']['work_item_type'] == 'activity_promotion_request'"
assert_json "$workflow_start" "data['parent_work_item']['capability_key'] == 'xiaoman.create_activity_request'"
assert_json "$workflow_start" "len(data['child_work_items']) == 2"
assert_json "$workflow_start" "data['child_work_items'][0]['work_item_type'] == 'evidence_request'"
assert_json "$workflow_start" "data['child_work_items'][0]['capability_key'] == 'wenyuange.retrieve_evidence'"
assert_json "$workflow_start" "data['child_work_items'][0]['parent_work_item_id'] == data['parent_work_item']['work_item_id']"
assert_json "$workflow_start" "data['child_work_items'][1]['work_item_type'] == 'visual_asset_request'"
assert_json "$workflow_start" "data['child_work_items'][1]['capability_key'] == 'huabaosi.create_visual_asset'"
assert_json "$workflow_start" "data['child_work_items'][1]['parent_work_item_id'] == data['parent_work_item']['work_item_id']"

workflow_start_again="$(run_json workflow_start_again operations-workflow-start --apply --payload-json "$workflow_start_payload")"
assert_json "$workflow_start_again" "data['success'] is True"
assert_json "$workflow_start_again" "data['action_status'] == 'idempotent_existing'"
assert_json "$workflow_start_again" "data['parent_work_item']['existing'] is True"
assert_json "$workflow_start_again" "data['child_work_items'][0]['existing'] is True"
assert_json "$workflow_start_again" "data['child_work_items'][1]['existing'] is True"

workflow_evidence_work_item_id="$(
  python3 - "$workflow_start" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["child_work_items"][0]["work_item_id"])
PY
)"

workflow_visual_work_item_id="$(
  python3 - "$workflow_start" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["child_work_items"][1]["work_item_id"])
PY
)"

parent_work_item_id="$(
  python3 - "$workflow_start" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["parent_work_item"]["work_item_id"])
PY
)"

assert_sql_equals \
  workflow_start_child_count \
  2 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = (SELECT parent_work_item_id FROM qintopia_agent_os.work_items WHERE id = '${workflow_evidence_work_item_id}'::uuid) AND work_item_type IN ('evidence_request','visual_asset_request');"

workflow_evidence_worker="$(run_json workflow_evidence_worker run-evidence-worker --once --work-item-id "$workflow_evidence_work_item_id" --apply)"
assert_json "$workflow_evidence_worker" "data['success'] is True"
assert_json "$workflow_evidence_worker" "data['action_status'] == 'evidence_artifact_created'"
assert_json "$workflow_evidence_worker" "data['work_item_id'] == '${workflow_evidence_work_item_id}'"
assert_json "$workflow_evidence_worker" "data['artifact_previews'][0]['artifact_type'] == 'evidence_summary'"
assert_json "$workflow_evidence_worker" "data['artifact_previews'][0]['review_status'] == 'not_required'"

assert_sql_equals \
  workflow_evidence_completed \
  completed \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${workflow_evidence_work_item_id}'::uuid;"

assert_sql_equals \
  workflow_visual_still_queued \
  queued \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${workflow_visual_work_item_id}'::uuid;"

work_item_id="$workflow_visual_work_item_id"

assert_sql_equals \
  one_workflow_visual_work_item_for_idempotency_key \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${work_item_id}'::uuid AND idempotency_key = '${source_ref}:workflow-start:visual-child';"

assert_sql_equals \
  created_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${work_item_id}'::uuid AND event_type = 'created';"

collaboration="$(run_json collaboration run-collaboration-worker --work-item-type visual_asset_request --once --work-item-id "$work_item_id" --apply)"
assert_json "$collaboration" "data['success'] is True"
assert_json "$collaboration" "data['action_status'] == 'artifacts_created'"
assert_json "$collaboration" "data['work_item_id'] == '${work_item_id}'"
assert_json "$collaboration" "len(data['artifact_ids']) == 1"
assert_json "$collaboration" "data['artifact_previews'][0]['review_status'] == 'pending'"

artifact_id="$(
  python3 - "$collaboration" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["artifact_ids"][0])
PY
)"

assert_sql_equals \
  work_item_awaiting_review \
  awaiting_review \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${work_item_id}'::uuid;"

assert_sql_equals \
  pending_artifact_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${work_item_id}'::uuid AND artifact_type = 'poster_brief' AND review_status = 'pending';"

assert_sql_equals \
  artifact_created_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${work_item_id}'::uuid AND event_type = 'artifact_created';"

visual_mirror="$(run_json visual_mirror run-workbench-mirror-worker --once --work-item-id "$work_item_id" --apply)"
assert_json "$visual_mirror" "data['success'] is True"
assert_json "$visual_mirror" "data['action_status'] == 'mirror_dry_run_recorded'"

review_payload="$(
  python3 - "$artifact_id" <<'PY'
import json
import sys
artifact_id = sys.argv[1]
print(json.dumps({
    "artifact_id": artifact_id,
    "reviewer_id": "operations-apply-smoke-reviewer",
    "decision": "approved",
    "reason": "apply smoke approval; does not publish or send",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"

review="$(run_json review operations-artifact-review-decision --apply --payload-json "$review_payload")"
assert_json "$review" "data['success'] is True"
assert_json "$review" "data['action_status'] == 'review_recorded'"
assert_json "$review" "data['artifact_id'] == '${artifact_id}'"
assert_json "$review" "data['work_item_id'] == '${work_item_id}'"
assert_json "$review" "data['previous_review_status'] == 'pending'"
assert_json "$review" "data['review_status'] == 'approved'"

assert_sql_equals \
  artifact_approved \
  approved \
  "SELECT review_status FROM qintopia_agent_os.artifacts WHERE id = '${artifact_id}'::uuid;"

assert_sql_equals \
  visual_work_item_completed_after_review \
  completed \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${work_item_id}'::uuid;"

assert_sql_equals \
  review_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${work_item_id}'::uuid AND artifact_id = '${artifact_id}'::uuid AND event_type = 'review_decision_recorded';"

assert_sql_equals \
  review_did_not_publish_or_send \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${work_item_id}'::uuid AND event_type IN ('send_executed','external_published','group_message_send_ready_recorded');"

review_allowlist_denial_before="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
review_allowlist_stdout="$tmp_dir/review-allowlist-denied.out"
review_allowlist_stderr="$tmp_dir/review-allowlist-denied.err"
if "${BIN_CMD[@]}" operations-artifact-review-decision --apply --payload-json "$(python3 - "$artifact_id" <<'PY'
import json
import sys
artifact_id = sys.argv[1]
print(json.dumps({
    "artifact_id": artifact_id,
    "reviewer_id": "operations-apply-smoke-unauthorized-reviewer",
    "decision": "approved",
    "reason": "should be rejected and audited by apply smoke reviewer allowlist",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)" >"$review_allowlist_stdout" 2>"$review_allowlist_stderr"; then
  echo "expected unauthorized reviewer apply request to fail" >&2
  cat "$review_allowlist_stdout" >&2
  cat "$review_allowlist_stderr" >&2
  exit 1
fi
if ! grep -Fq "reviewer_id is not allowed for artifact review decisions" "$review_allowlist_stdout" && ! grep -Fq "reviewer_id is not allowed for artifact review decisions" "$review_allowlist_stderr"; then
  echo "expected reviewer allowlist denial output" >&2
  cat "$review_allowlist_stdout" >&2
  cat "$review_allowlist_stderr" >&2
  exit 1
fi
review_allowlist_denial_after="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
if [[ "$review_allowlist_denial_after" -le "$review_allowlist_denial_before" ]]; then
  echo "expected denied_by_policy event for unauthorized reviewer" >&2
  echo "before=${review_allowlist_denial_before} after=${review_allowlist_denial_after}" >&2
  exit 1
fi

assert_sql_equals \
  review_allowlist_denial_policy_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy' AND artifact_id = '${artifact_id}'::uuid AND actor_id = 'operations-apply-smoke-unauthorized-reviewer' AND data->>'policy' = 'artifact_review_reviewer_allowlist';"

group_payload="$(
  python3 - "$group_idempotency_key" "$source_ref" "$artifact_id" "$parent_work_item_id" <<'PY'
import json
import sys
idempotency_key, source_ref, artifact_id, parent_work_item_id = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "erhua",
    "capability_key": "erhua.send_group_message",
    "work_item_type": "group_message_request",
    "brief_summary": "AgentOS apply smoke group message request",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref},
    "idempotency_key": idempotency_key,
    "parent_work_item_id": parent_work_item_id,
    "payload": {
        "approved_artifact_id": artifact_id,
        "target_channel": "qiwe",
        "target_group_alias": "community_activity_group",
        "message_text": "Apply smoke: approved activity poster is ready for controlled send."
    }
}, ensure_ascii=False))
PY
)"

group_created="$(run_json group_created operations-work-item-create --apply --payload-json "$group_payload")"
assert_json "$group_created" "data['success'] is True"
assert_json "$group_created" "data['action_status'] == 'created'"
assert_json "$group_created" "data['work_item_type'] == 'group_message_request'"
assert_json "$group_created" "data['capability_key'] == 'erhua.send_group_message'"
assert_json "$group_created" "data['current_status'] == 'awaiting_publish'"
assert_json "$group_created" "data['review_policy'] == 'human_final_confirmation'"
assert_json "$group_created" "data['parent_work_item_id'] == '${parent_work_item_id}'"

group_work_item_id="$(
  python3 - "$group_created" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["work_item_id"])
PY
)"

assert_sql_equals \
  group_request_awaiting_publish \
  awaiting_publish \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${group_work_item_id}'::uuid;"

confirm_allowlist_denial_before="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
confirm_allowlist_stdout="$tmp_dir/confirm-allowlist-denied.out"
confirm_allowlist_stderr="$tmp_dir/confirm-allowlist-denied.err"
if "${BIN_CMD[@]}" operations-group-message-confirm --apply --payload-json "$(python3 - "$group_work_item_id" <<'PY'
import json
import sys
work_item_id = sys.argv[1]
print(json.dumps({
    "work_item_id": work_item_id,
    "confirmer_id": "operations-apply-smoke-unauthorized-confirmer",
    "decision": "confirmed",
    "reason": "should be rejected and audited by apply smoke confirmer allowlist",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)" >"$confirm_allowlist_stdout" 2>"$confirm_allowlist_stderr"; then
  echo "expected unauthorized confirmer apply request to fail" >&2
  cat "$confirm_allowlist_stdout" >&2
  cat "$confirm_allowlist_stderr" >&2
  exit 1
fi
if ! grep -Fq "confirmer_id is not allowed for group message final confirmation" "$confirm_allowlist_stdout" && ! grep -Fq "confirmer_id is not allowed for group message final confirmation" "$confirm_allowlist_stderr"; then
  echo "expected confirmer allowlist denial output" >&2
  cat "$confirm_allowlist_stdout" >&2
  cat "$confirm_allowlist_stderr" >&2
  exit 1
fi
confirm_allowlist_denial_after="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
if [[ "$confirm_allowlist_denial_after" -le "$confirm_allowlist_denial_before" ]]; then
  echo "expected denied_by_policy event for unauthorized confirmer" >&2
  echo "before=${confirm_allowlist_denial_before} after=${confirm_allowlist_denial_after}" >&2
  exit 1
fi

assert_sql_equals \
  confirm_allowlist_denial_policy_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy' AND work_item_id = '${group_work_item_id}'::uuid AND actor_id = 'operations-apply-smoke-unauthorized-confirmer' AND data->>'policy' = 'group_message_confirmer_allowlist';"

assert_sql_equals \
  parent_has_visual_and_group_children \
  3 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${parent_work_item_id}'::uuid AND work_item_type IN ('evidence_request','visual_asset_request','group_message_request');"

assert_sql_equals \
  parent_child_capabilities_are_distinct \
  3 \
  "SELECT count(DISTINCT capability_key) FROM qintopia_agent_os.work_items WHERE parent_work_item_id = '${parent_work_item_id}'::uuid;"

group_confirm_payload="$(
  python3 - "$group_work_item_id" <<'PY'
import json
import sys
work_item_id = sys.argv[1]
print(json.dumps({
    "work_item_id": work_item_id,
    "confirmer_id": "operations-apply-smoke-confirmer",
    "decision": "confirmed",
    "reason": "apply smoke final confirmation; send worker must still not send",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"

group_confirm="$(run_json group_confirm operations-group-message-confirm --apply --payload-json "$group_confirm_payload")"
assert_json "$group_confirm" "data['success'] is True"
assert_json "$group_confirm" "data['action_status'] == 'confirmation_recorded'"
assert_json "$group_confirm" "data['work_item_id'] == '${group_work_item_id}'"
assert_json "$group_confirm" "data['previous_status'] == 'awaiting_publish'"
assert_json "$group_confirm" "data['current_status'] == 'queued'"
assert_json "$group_confirm" "data['send_executed'] is False"

assert_sql_equals \
  group_confirmation_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${group_work_item_id}'::uuid AND event_type = 'group_message_final_confirmation_recorded';"

send_ready="$(run_json send_ready run-group-message-send-worker --once --work-item-id "$group_work_item_id" --apply)"
assert_json "$send_ready" "data['success'] is True"
assert_json "$send_ready" "data['action_status'] == 'send_ready_recorded'"
assert_json "$send_ready" "data['work_item_id'] == '${group_work_item_id}'"
assert_json "$send_ready" "data['send_executed'] is False"
assert_json "$send_ready" "data['approved_artifact_id'] == '${artifact_id}'"

assert_sql_equals \
  group_send_ready_attempt_incremented \
  1 \
  "SELECT attempts FROM qintopia_agent_os.work_items WHERE id = '${group_work_item_id}'::uuid;"

assert_sql_equals \
  group_send_ready_claim_metadata_cleared \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_items WHERE id = '${group_work_item_id}'::uuid AND (locked_at IS NOT NULL OR claim_expires_at IS NOT NULL);"

assert_sql_equals \
  group_send_ready_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${group_work_item_id}'::uuid AND event_type = 'group_message_send_ready_recorded' AND data->>'send_executed' = 'false';"

send_ready_again="$(run_json send_ready_again run-group-message-send-worker --once --work-item-id "$group_work_item_id" --apply)"
assert_json "$send_ready_again" "data['success'] is True"
assert_json "$send_ready_again" "data['action_status'] == 'no_claimable_group_message_request'"
assert_json "$send_ready_again" "data['send_executed'] is False"

assert_sql_equals \
  group_send_ready_event_not_duplicated \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${group_work_item_id}'::uuid AND event_type = 'group_message_send_ready_recorded' AND data->>'send_executed' = 'false';"

assert_sql_equals \
  group_request_still_queued_after_send_ready \
  queued \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${group_work_item_id}'::uuid;"

status_tree="$(run_json status_tree operations-work-item-status --work-item-id "$parent_work_item_id")"
assert_json "$status_tree" "data['success'] is True"
assert_json "$status_tree" "data['root_work_item_id'] == '${parent_work_item_id}'"
assert_json "$status_tree" "data['child_count'] == 3"
assert_json "$status_tree" "any(item['work_item_id'] == '${workflow_evidence_work_item_id}' and item['work_item_type'] == 'evidence_request' and item['status'] == 'completed' for item in data['children'])"
assert_json "$status_tree" "any(item['work_item_id'] == '${work_item_id}' and item['work_item_type'] == 'visual_asset_request' for item in data['children'])"
assert_json "$status_tree" "any(item['work_item_id'] == '${group_work_item_id}' and item['work_item_type'] == 'group_message_request' for item in data['children'])"
assert_json "$status_tree" "data['current_blocking_point'] == 'group_message_request:send_ready_waiting_for_production_send_adapter'"

workflow_sync_dry="$(run_json workflow_sync_dry operations-workflow-sync --work-item-id "$parent_work_item_id" --dry-run)"
assert_json "$workflow_sync_dry" "data['success'] is True"
assert_json "$workflow_sync_dry" "data['action_status'] == 'dry_run_ok'"
assert_json "$workflow_sync_dry" "data['aggregate_status'] == 'processing'"
assert_json "$workflow_sync_dry" "data['current_blocking_point'] == 'group_message_request:send_ready_waiting_for_production_send_adapter'"
assert_json "$workflow_sync_dry" "len(data['child_status_refs']) == 3"

workflow_sync="$(run_json workflow_sync operations-workflow-sync --work-item-id "$parent_work_item_id" --apply)"
assert_json "$workflow_sync" "data['success'] is True"
assert_json "$workflow_sync" "data['action_status'] == 'workflow_status_synced'"
assert_json "$workflow_sync" "data['aggregate_status'] == 'processing'"
assert_json "$workflow_sync" "data['event_id'] is not None"
assert_json "$workflow_sync" "data['current_blocking_point'] == 'group_message_request:send_ready_waiting_for_production_send_adapter'"

assert_sql_equals \
  workflow_parent_processing_after_sync \
  processing \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${parent_work_item_id}'::uuid;"

assert_sql_equals \
  workflow_sync_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${parent_work_item_id}'::uuid AND event_type = 'workflow_status_synced' AND data->>'aggregate_status' = 'processing';"

assert_sql_equals \
  workflow_summary_metadata_written \
  group_message_request:send_ready_waiting_for_production_send_adapter \
  "SELECT metadata #>> '{workflow_summary,current_blocking_point}' FROM qintopia_agent_os.work_items WHERE id = '${parent_work_item_id}'::uuid;"

workflow_sync_worker_dry="$(run_json workflow_sync_worker_dry run-workflow-sync-worker --once --work-item-id "$parent_work_item_id" --dry-run)"
assert_json "$workflow_sync_worker_dry" "data['success'] is True"
assert_json "$workflow_sync_worker_dry" "data['worker'] == 'workflow-sync-worker'"
assert_json "$workflow_sync_worker_dry" "data['action_status'] == 'dry_run_ok'"
assert_json "$workflow_sync_worker_dry" "data['root_work_item_id'] == '${parent_work_item_id}'"
assert_json "$workflow_sync_worker_dry" "data['sync_report']['aggregate_status'] == 'processing'"

workflow_sync_worker="$(run_json workflow_sync_worker run-workflow-sync-worker --once --work-item-id "$parent_work_item_id" --apply)"
assert_json "$workflow_sync_worker" "data['success'] is True"
assert_json "$workflow_sync_worker" "data['worker'] == 'workflow-sync-worker'"
assert_json "$workflow_sync_worker" "data['action_status'] == 'workflow_status_synced'"
assert_json "$workflow_sync_worker" "data['root_work_item_id'] == '${parent_work_item_id}'"
assert_json "$workflow_sync_worker" "data['sync_report']['event_id'] is not None"

assert_sql_equals \
  workflow_sync_worker_event_written \
  2 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${parent_work_item_id}'::uuid AND event_type = 'workflow_status_synced' AND data->>'aggregate_status' = 'processing';"

unreviewed_payload="$(
  python3 - "$source_ref" <<'PY'
import json
import sys
source_ref = sys.argv[1]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "huabaosi",
    "capability_key": "huabaosi.create_visual_asset",
    "work_item_type": "visual_asset_request",
    "brief_summary": "AgentOS apply smoke unreviewed visual request",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref + ":unreviewed"},
    "idempotency_key": source_ref + ":unreviewed-visual",
}, ensure_ascii=False))
PY
)"
unreviewed_created="$(run_json unreviewed_created operations-work-item-create --apply --payload-json "$unreviewed_payload")"
unreviewed_work_item_id="$(
  python3 - "$unreviewed_created" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["work_item_id"])
PY
)"
unreviewed_collaboration="$(run_json unreviewed_collaboration run-collaboration-worker --work-item-type visual_asset_request --once --work-item-id "$unreviewed_work_item_id" --apply)"
unreviewed_artifact_id="$(
  python3 - "$unreviewed_collaboration" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["artifact_ids"][0])
PY
)"
unreviewed_group_payload="$(
  python3 - "$source_ref" "$unreviewed_artifact_id" <<'PY'
import json
import sys
source_ref, artifact_id = sys.argv[1], sys.argv[2]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "erhua",
    "capability_key": "erhua.send_group_message",
    "work_item_type": "group_message_request",
    "brief_summary": "AgentOS apply smoke should reject unapproved artifact",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref + ":unapproved-group"},
    "idempotency_key": source_ref + ":unapproved-group",
    "payload": {
        "approved_artifact_id": artifact_id,
        "target_channel": "qiwe",
        "target_group_alias": "community_activity_group",
        "message_text": "This should be rejected because the artifact is not approved."
    }
}, ensure_ascii=False))
PY
)"
unapproved_denial_before="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
unapproved_denial_stdout="$tmp_dir/unapproved-denied.out"
unapproved_denial_stderr="$tmp_dir/unapproved-denied.err"
if "${BIN_CMD[@]}" operations-work-item-create --apply --payload-json "$unreviewed_group_payload" >"$unapproved_denial_stdout" 2>"$unapproved_denial_stderr"; then
  echo "expected unapproved artifact group request to fail" >&2
  cat "$unapproved_denial_stdout" >&2
  cat "$unapproved_denial_stderr" >&2
  exit 1
fi
if ! grep -Fq "approved_artifact_id must reference an approved artifact" "$unapproved_denial_stdout" && ! grep -Fq "approved_artifact_id must reference an approved artifact" "$unapproved_denial_stderr"; then
  echo "expected unapproved artifact denial output" >&2
  cat "$unapproved_denial_stdout" >&2
  cat "$unapproved_denial_stderr" >&2
  exit 1
fi
unapproved_denial_after="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
if [[ "$unapproved_denial_after" -le "$unapproved_denial_before" ]]; then
  echo "expected denied_by_policy event for unapproved artifact" >&2
  echo "before=${unapproved_denial_before} after=${unapproved_denial_after}" >&2
  exit 1
fi

max_attempt_group_payload="$(
  python3 - "$source_ref" "$artifact_id" "$parent_work_item_id" <<'PY'
import json
import sys
source_ref, artifact_id, parent_work_item_id = sys.argv[1], sys.argv[2], sys.argv[3]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "erhua",
    "capability_key": "erhua.send_group_message",
    "work_item_type": "group_message_request",
    "brief_summary": "AgentOS apply smoke max-attempt group message request",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref + ":max-attempt-group"},
    "idempotency_key": source_ref + ":max-attempt-group",
    "parent_work_item_id": parent_work_item_id,
    "payload": {
        "approved_artifact_id": artifact_id,
        "target_channel": "qiwe",
        "target_group_alias": "community_activity_group",
        "message_text": "This should be skipped because attempts are already exhausted."
    }
}, ensure_ascii=False))
PY
)"
max_attempt_group="$(run_json max_attempt_group operations-work-item-create --apply --payload-json "$max_attempt_group_payload")"
assert_json "$max_attempt_group" "data['success'] is True"
assert_json "$max_attempt_group" "data['current_status'] == 'awaiting_publish'"
max_attempt_group_work_item_id="$(
  python3 - "$max_attempt_group" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["work_item_id"])
PY
)"
max_attempt_confirm_payload="$(
  python3 - "$max_attempt_group_work_item_id" <<'PY'
import json
import sys
work_item_id = sys.argv[1]
print(json.dumps({
    "work_item_id": work_item_id,
    "confirmer_id": "operations-apply-smoke-confirmer",
    "decision": "confirmed",
    "reason": "apply smoke max attempts setup",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
max_attempt_confirm="$(run_json max_attempt_confirm operations-group-message-confirm --apply --payload-json "$max_attempt_confirm_payload")"
assert_json "$max_attempt_confirm" "data['success'] is True"
assert_json "$max_attempt_confirm" "data['current_status'] == 'queued'"
psql_value "UPDATE qintopia_agent_os.work_items SET attempts = 3 WHERE id = '${max_attempt_group_work_item_id}'::uuid;" >/dev/null

max_attempt_send_ready="$(run_json max_attempt_send_ready run-group-message-send-worker --once --work-item-id "$max_attempt_group_work_item_id" --apply)"
assert_json "$max_attempt_send_ready" "data['success'] is True"
assert_json "$max_attempt_send_ready" "data['action_status'] == 'no_claimable_group_message_request'"

assert_sql_equals \
  max_attempt_group_not_processed \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${max_attempt_group_work_item_id}'::uuid AND event_type = 'group_message_send_ready_recorded';"

assert_sql_equals \
  max_attempt_group_attempts_not_incremented \
  3 \
  "SELECT attempts FROM qintopia_agent_os.work_items WHERE id = '${max_attempt_group_work_item_id}'::uuid;"

mirror="$(run_json mirror run-workbench-mirror-worker --once --work-item-id "$parent_work_item_id" --apply)"
assert_json "$mirror" "data['success'] is True"
assert_json "$mirror" "data['action_status'] == 'mirror_dry_run_recorded'"
assert_json "$mirror" "data['work_item_id'] == '${parent_work_item_id}'"
assert_json "$mirror" "data['provider'] == 'feishu_task_dry_run'"
assert_json "$mirror" "'payload' not in data['description']"
assert_json "$mirror" "'child_status_refs' in data['description']"
assert_json "$mirror" "'current_blocking_point: group_message_request:send_ready_waiting_for_production_send_adapter' in data['description']"
assert_json "$mirror" "data['sensitive_fields_redacted'] is True"

mirror_again="$(run_json mirror_again run-workbench-mirror-worker --once --work-item-id "$parent_work_item_id" --apply)"
assert_json "$mirror_again" "data['success'] is True"
assert_json "$mirror_again" "data['action_status'] == 'no_mirrorable_work_item'"

assert_sql_equals \
  one_workbench_ref_for_work_item \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.human_workbench_refs WHERE work_item_id = '${parent_work_item_id}'::uuid AND provider = 'feishu_task_dry_run';"

assert_sql_equals \
  mirror_event_written \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${parent_work_item_id}'::uuid AND event_type = 'mirror_dry_run_recorded';"

assert_sql_equals \
  mirror_description_payload_redacted \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.human_workbench_refs WHERE work_item_id = '${parent_work_item_id}'::uuid AND provider = 'feishu_task_dry_run' AND metadata::text ILIKE '%payload%';"

workbench_event_payload="$(
  python3 - "$parent_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-comment",
    "event_type": "comment_added",
    "actor_id": "operations-apply-smoke-human",
    "comment_text": "Apply smoke workbench comment; audit only, no state mutation.",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
parent_status_before_workbench_event="$(psql_value "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${parent_work_item_id}'::uuid;")"
workbench_event="$(run_json workbench_event operations-workbench-event-record --apply --payload-json "$workbench_event_payload")"
assert_json "$workbench_event" "data['success'] is True"
assert_json "$workbench_event" "data['action_status'] == 'event_recorded'"
assert_json "$workbench_event" "data['work_item_id'] == '${parent_work_item_id}'"
assert_json "$workbench_event" "data['provider'] == 'feishu_task_dry_run'"
assert_json "$workbench_event" "data['mutates_work_item_state'] is False"
assert_json "$workbench_event" "data['recommended_command'] is None"

workbench_event_again="$(run_json workbench_event_again operations-workbench-event-record --apply --payload-json "$workbench_event_payload")"
assert_json "$workbench_event_again" "data['success'] is True"
assert_json "$workbench_event_again" "data['action_status'] == 'idempotent_existing'"

assert_sql_equals \
  one_workbench_event_for_external_event \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-comment';"

assert_sql_equals \
  workbench_event_did_not_mutate_parent_status \
  "${parent_status_before_workbench_event}" \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${parent_work_item_id}'::uuid;"

status_change_payload="$(
  python3 - "$source_ref" <<'PY'
import json
import sys
source_ref = sys.argv[1]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "huabaosi",
    "capability_key": "huabaosi.create_visual_asset",
    "work_item_type": "visual_asset_request",
    "brief_summary": "AgentOS apply smoke cancellable workbench status change target",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref + ":workbench-status-change"},
    "idempotency_key": source_ref + ":workbench-status-change",
}, ensure_ascii=False))
PY
)"
status_change_target="$(run_json status_change_target operations-work-item-create --apply --payload-json "$status_change_payload")"
assert_json "$status_change_target" "data['success'] is True"
assert_json "$status_change_target" "data['action_status'] == 'created'"
assert_json "$status_change_target" "data['current_status'] == 'queued'"
status_change_work_item_id="$(
  python3 - "$status_change_target" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["work_item_id"])
PY
)"
status_change_mirror="$(run_json status_change_mirror run-workbench-mirror-worker --once --work-item-id "$status_change_work_item_id" --apply)"
assert_json "$status_change_mirror" "data['success'] is True"
assert_json "$status_change_mirror" "data['action_status'] == 'mirror_dry_run_recorded'"

status_change_event_payload="$(
  python3 - "$status_change_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-status-cancel",
    "event_type": "status_change_requested",
    "actor_id": "operations-apply-smoke-human",
    "requested_status": "cancelled",
    "comment_text": "Apply smoke cancels this work item through validated workbench status sync.",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
status_change_event="$(run_json status_change_event operations-workbench-event-record --apply --payload-json "$status_change_event_payload")"
assert_json "$status_change_event" "data['success'] is True"
assert_json "$status_change_event" "data['action_status'] == 'event_recorded'"
assert_json "$status_change_event" "data['recommended_command'] == 'operations-workbench-status-change'"
status_change_event_id="$(psql_value "SELECT id FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-status-cancel' ORDER BY created_at DESC LIMIT 1;")"
status_change_process_dry="$(run_json status_change_process_dry operations-workbench-event-process --event-id "$status_change_event_id" --dry-run)"
assert_json "$status_change_process_dry" "data['success'] is True"
assert_json "$status_change_process_dry" "data['action_status'] == 'dry_run_ok'"
assert_json "$status_change_process_dry" "data['command_executed'] == 'operations-workbench-status-change'"
assert_json "$status_change_process_dry" "data['state_mutation_recorded'] is False"
status_change_process="$(run_json status_change_process run-workbench-event-worker --once --event-id "$status_change_event_id" --apply)"
assert_json "$status_change_process" "data['success'] is True"
assert_json "$status_change_process" "data['action_status'] == 'processed'"
assert_json "$status_change_process" "data['process_report']['command_executed'] == 'operations-workbench-status-change'"
assert_json "$status_change_process" "data['process_report']['state_mutation_recorded'] is True"
assert_sql_equals \
  workbench_status_change_cancelled_work_item \
  cancelled \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${status_change_work_item_id}'::uuid;"
assert_sql_equals \
  workbench_status_change_recorded_event \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${status_change_work_item_id}'::uuid AND event_type = 'workbench_status_change_recorded';"
assert_sql_equals \
  workbench_status_change_processed_once \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_processed' AND data->>'source_event_id' = '${status_change_event_id}';"

invalid_status_change_event_payload="$(
  python3 - "$status_change_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-status-completed",
    "event_type": "status_change_requested",
    "actor_id": "operations-apply-smoke-human",
    "requested_status": "completed",
    "comment_text": "Apply smoke attempts an unsafe direct completion.",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
run_expect_failure \
  invalid_workbench_status_change_record \
  "status_change_requested can only request cancelled status" \
  operations-workbench-event-record \
  --apply \
  --payload-json "$invalid_status_change_event_payload"
assert_sql_equals \
  invalid_workbench_status_change_not_recorded \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-status-completed';"

owner_change_event_payload="$(
  python3 - "$status_change_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-owner-change",
    "event_type": "owner_changed",
    "actor_id": "operations-apply-smoke-human",
    "comment_text": "Apply smoke assigns a human owner through validated workbench owner sync.",
    "metadata": {"new_human_owner": "operations-apply-smoke-owner"},
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
owner_change_event="$(run_json owner_change_event operations-workbench-event-record --apply --payload-json "$owner_change_event_payload")"
assert_json "$owner_change_event" "data['success'] is True"
assert_json "$owner_change_event" "data['action_status'] == 'event_recorded'"
assert_json "$owner_change_event" "data['recommended_command'] == 'operations-workbench-owner-change'"
owner_change_event_id="$(psql_value "SELECT id FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-owner-change' ORDER BY created_at DESC LIMIT 1;")"
owner_change_process_dry="$(run_json owner_change_process_dry operations-workbench-event-process --event-id "$owner_change_event_id" --dry-run)"
assert_json "$owner_change_process_dry" "data['success'] is True"
assert_json "$owner_change_process_dry" "data['action_status'] == 'dry_run_ok'"
assert_json "$owner_change_process_dry" "data['command_executed'] == 'operations-workbench-owner-change'"
assert_json "$owner_change_process_dry" "data['state_mutation_recorded'] is False"
owner_change_process="$(run_json owner_change_process run-workbench-event-worker --once --event-id "$owner_change_event_id" --apply)"
assert_json "$owner_change_process" "data['success'] is True"
assert_json "$owner_change_process" "data['action_status'] == 'processed'"
assert_json "$owner_change_process" "data['process_report']['command_executed'] == 'operations-workbench-owner-change'"
assert_json "$owner_change_process" "data['process_report']['state_mutation_recorded'] is True"
assert_sql_equals \
  workbench_owner_change_updated_human_owner \
  operations-apply-smoke-owner \
  "SELECT human_owner FROM qintopia_agent_os.work_items WHERE id = '${status_change_work_item_id}'::uuid;"
assert_sql_equals \
  workbench_owner_change_recorded_event \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${status_change_work_item_id}'::uuid AND event_type = 'workbench_owner_change_recorded';"
assert_sql_equals \
  workbench_owner_change_processed_once \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_processed' AND data->>'source_event_id' = '${owner_change_event_id}';"

unauthorized_owner_event_payload="$(
  python3 - "$status_change_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-owner-change-denied",
    "event_type": "owner_changed",
    "actor_id": "operations-apply-smoke-human",
    "comment_text": "Apply smoke attempts an unauthorized owner assignment.",
    "metadata": {"new_human_owner": "operations-apply-smoke-unauthorized-owner"},
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
unauthorized_owner_event="$(run_json unauthorized_owner_event operations-workbench-event-record --apply --payload-json "$unauthorized_owner_event_payload")"
assert_json "$unauthorized_owner_event" "data['success'] is True"
assert_json "$unauthorized_owner_event" "data['action_status'] == 'event_recorded'"
unauthorized_owner_event_id="$(psql_value "SELECT id FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-owner-change-denied' ORDER BY created_at DESC LIMIT 1;")"
owner_denial_before="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
run_expect_failure \
  unauthorized_workbench_owner_change_process \
  "human_owner is not allowed for workbench owner changes" \
  operations-workbench-event-process \
  --event-id "$unauthorized_owner_event_id" \
  --apply
owner_denial_after="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
if [[ "$owner_denial_after" -le "$owner_denial_before" ]]; then
  echo "expected denied_by_policy event for unauthorized workbench owner change" >&2
  echo "before=${owner_denial_before} after=${owner_denial_after}" >&2
  exit 1
fi
assert_sql_equals \
  unauthorized_workbench_owner_change_not_processed \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_processed' AND data->>'source_event_id' = '${unauthorized_owner_event_id}';"
assert_sql_equals \
  unauthorized_workbench_owner_change_did_not_update_owner \
  operations-apply-smoke-owner \
  "SELECT human_owner FROM qintopia_agent_os.work_items WHERE id = '${status_change_work_item_id}'::uuid;"

attachment_event_payload="$(
  python3 - "$status_change_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-attachment",
    "event_type": "attachment_added",
    "actor_id": "operations-apply-smoke-human",
    "comment_text": "Apply smoke adds a human workbench attachment as a pending artifact.",
    "metadata": {
        "attachment_title": "Apply smoke reference attachment",
        "attachment_summary": "Human-supplied attachment recorded for review; not sent or published.",
        "attachment_uri": "https://example.com/agentos/apply-smoke-attachment.png",
        "attachment_text": "Internal reference only; requires artifact review before external use."
    },
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
attachment_event="$(run_json attachment_event operations-workbench-event-record --apply --payload-json "$attachment_event_payload")"
assert_json "$attachment_event" "data['success'] is True"
assert_json "$attachment_event" "data['action_status'] == 'event_recorded'"
assert_json "$attachment_event" "data['recommended_command'] == 'operations-workbench-attachment-add'"
attachment_event_id="$(psql_value "SELECT id FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-attachment' ORDER BY created_at DESC LIMIT 1;")"
attachment_process_dry="$(run_json attachment_process_dry operations-workbench-event-process --event-id "$attachment_event_id" --dry-run)"
assert_json "$attachment_process_dry" "data['success'] is True"
assert_json "$attachment_process_dry" "data['action_status'] == 'dry_run_ok'"
assert_json "$attachment_process_dry" "data['command_executed'] == 'operations-workbench-attachment-add'"
assert_json "$attachment_process_dry" "data['state_mutation_recorded'] is False"
attachment_process="$(run_json attachment_process run-workbench-event-worker --once --event-id "$attachment_event_id" --apply)"
assert_json "$attachment_process" "data['success'] is True"
assert_json "$attachment_process" "data['action_status'] == 'processed'"
assert_json "$attachment_process" "data['process_report']['command_executed'] == 'operations-workbench-attachment-add'"
assert_json "$attachment_process" "data['process_report']['state_mutation_recorded'] is True"
assert_sql_equals \
  workbench_attachment_pending_artifact \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = '${status_change_work_item_id}'::uuid AND artifact_type = 'workbench_attachment' AND review_status = 'pending';"
assert_sql_equals \
  workbench_attachment_recorded_event \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${status_change_work_item_id}'::uuid AND event_type = 'workbench_attachment_artifact_recorded';"
assert_sql_equals \
  workbench_attachment_processed_once \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_processed' AND data->>'source_event_id' = '${attachment_event_id}';"

unauthorized_attachment_event_payload="$(
  python3 - "$status_change_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-attachment-denied",
    "event_type": "attachment_added",
    "actor_id": "operations-apply-smoke-human",
    "comment_text": "Apply smoke attempts a non-allowlisted attachment host.",
    "metadata": {
        "attachment_title": "Denied apply smoke attachment",
        "attachment_summary": "This attachment host should be denied.",
        "attachment_uri": "https://not-allowlisted.example.net/agentos/apply-smoke-attachment.png"
    },
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
unauthorized_attachment_event="$(run_json unauthorized_attachment_event operations-workbench-event-record --apply --payload-json "$unauthorized_attachment_event_payload")"
assert_json "$unauthorized_attachment_event" "data['success'] is True"
assert_json "$unauthorized_attachment_event" "data['action_status'] == 'event_recorded'"
unauthorized_attachment_event_id="$(psql_value "SELECT id FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-attachment-denied' ORDER BY created_at DESC LIMIT 1;")"
attachment_denial_before="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
run_expect_failure \
  unauthorized_workbench_attachment_process \
  "attachment_uri host is not allowed for workbench attachments" \
  operations-workbench-event-process \
  --event-id "$unauthorized_attachment_event_id" \
  --apply
attachment_denial_after="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
if [[ "$attachment_denial_after" -le "$attachment_denial_before" ]]; then
  echo "expected denied_by_policy event for unauthorized workbench attachment host" >&2
  echo "before=${attachment_denial_before} after=${attachment_denial_after}" >&2
  exit 1
fi
assert_sql_equals \
  unauthorized_workbench_attachment_not_processed \
  0 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_processed' AND data->>'source_event_id' = '${unauthorized_attachment_event_id}';"

review_workbench_event_payload="$(
  python3 - "$work_item_id" "$artifact_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, artifact_id, source_ref = sys.argv[1], sys.argv[2], sys.argv[3]
print(json.dumps({
    "work_item_id": work_item_id,
    "artifact_id": artifact_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-review-approved",
    "event_type": "review_decision_requested",
    "actor_id": "operations-apply-smoke-reviewer-2",
    "review_decision": "approved",
    "comment_text": "Apply smoke records approval through workbench event processing.",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
review_workbench_event="$(run_json review_workbench_event operations-workbench-event-record --apply --payload-json "$review_workbench_event_payload")"
assert_json "$review_workbench_event" "data['success'] is True"
assert_json "$review_workbench_event" "data['action_status'] == 'event_recorded'"
assert_json "$review_workbench_event" "data['recommended_command'] == 'operations-artifact-review-decision'"

review_workbench_event_id="$(psql_value "SELECT id FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-review-approved' ORDER BY created_at DESC LIMIT 1;")"
review_workbench_process_dry="$(run_json review_workbench_process_dry operations-workbench-event-process --event-id "$review_workbench_event_id" --dry-run)"
assert_json "$review_workbench_process_dry" "data['success'] is True"
assert_json "$review_workbench_process_dry" "data['action_status'] == 'dry_run_ok'"
assert_json "$review_workbench_process_dry" "data['command_executed'] == 'operations-artifact-review-decision'"
assert_json "$review_workbench_process_dry" "data['state_mutation_recorded'] is False"

review_workbench_process="$(run_json review_workbench_process run-workbench-event-worker --once --event-id "$review_workbench_event_id" --apply)"
assert_json "$review_workbench_process" "data['success'] is True"
assert_json "$review_workbench_process" "data['action_status'] == 'processed'"
assert_json "$review_workbench_process" "data['worker'] == 'workbench-event-worker'"
assert_json "$review_workbench_process" "data['process_report']['command_executed'] == 'operations-artifact-review-decision'"
assert_json "$review_workbench_process" "data['process_report']['state_mutation_recorded'] is True"

review_workbench_process_again="$(run_json review_workbench_process_again run-workbench-event-worker --once --event-id "$review_workbench_event_id" --apply)"
assert_json "$review_workbench_process_again" "data['success'] is True"
assert_json "$review_workbench_process_again" "data['action_status'] == 'idempotent_existing'"

workbench_event_worker_empty="$(run_json workbench_event_worker_empty run-workbench-event-worker --once --dry-run)"
assert_json "$workbench_event_worker_empty" "data['success'] is True"
assert_json "$workbench_event_worker_empty" "data['action_status'] == 'no_processable_workbench_event'"

assert_sql_equals \
  review_workbench_processing_kept_artifact_approved \
  approved \
  "SELECT review_status FROM qintopia_agent_os.artifacts WHERE id = '${artifact_id}'::uuid;"

assert_sql_equals \
  review_workbench_processed_once \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_processed' AND data->>'source_event_id' = '${review_workbench_event_id}';"

workbench_confirm_group_payload="$(
  python3 - "$workbench_confirm_group_idempotency_key" "$source_ref" "$artifact_id" "$parent_work_item_id" <<'PY'
import json
import sys
idempotency_key, source_ref, artifact_id, parent_work_item_id = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "erhua",
    "capability_key": "erhua.send_group_message",
    "work_item_type": "group_message_request",
    "brief_summary": "AgentOS apply smoke workbench final confirmation request",
    "source_type": "apply_smoke",
    "source_refs": {"source_record_ref": source_ref + ":workbench-final-confirmation"},
    "idempotency_key": idempotency_key,
    "parent_work_item_id": parent_work_item_id,
    "payload": {
        "approved_artifact_id": artifact_id,
        "target_channel": "qiwe",
        "target_group_alias": "community_activity_group",
        "message_text": "Apply smoke: workbench final confirmation path."
    }
}, ensure_ascii=False))
PY
)"
workbench_confirm_group="$(run_json workbench_confirm_group operations-work-item-create --apply --payload-json "$workbench_confirm_group_payload")"
assert_json "$workbench_confirm_group" "data['success'] is True"
assert_json "$workbench_confirm_group" "data['action_status'] == 'created'"
assert_json "$workbench_confirm_group" "data['work_item_type'] == 'group_message_request'"
assert_json "$workbench_confirm_group" "data['current_status'] == 'awaiting_publish'"

workbench_confirm_group_work_item_id="$(
  python3 - "$workbench_confirm_group" <<'PY'
import json
import sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["work_item_id"])
PY
)"

workbench_confirm_group_mirror="$(run_json workbench_confirm_group_mirror run-workbench-mirror-worker --once --work-item-id "$workbench_confirm_group_work_item_id" --apply)"
assert_json "$workbench_confirm_group_mirror" "data['success'] is True"
assert_json "$workbench_confirm_group_mirror" "data['action_status'] == 'mirror_dry_run_recorded'"

confirm_workbench_event_payload="$(
  python3 - "$workbench_confirm_group_work_item_id" "$source_ref" <<'PY'
import json
import sys
work_item_id, source_ref = sys.argv[1], sys.argv[2]
print(json.dumps({
    "work_item_id": work_item_id,
    "provider": "feishu_task_dry_run",
    "external_id": "agentos-work-item-" + work_item_id,
    "external_event_id": source_ref + ":workbench-final-confirmation",
    "event_type": "final_confirmation_requested",
    "actor_id": "operations-apply-smoke-confirmer",
    "confirmation_decision": "confirmed",
    "comment_text": "Apply smoke confirms send through workbench event processing.",
    "source": "operations_apply_smoke",
}, ensure_ascii=False))
PY
)"
confirm_workbench_event="$(run_json confirm_workbench_event operations-workbench-event-record --apply --payload-json "$confirm_workbench_event_payload")"
assert_json "$confirm_workbench_event" "data['success'] is True"
assert_json "$confirm_workbench_event" "data['action_status'] == 'event_recorded'"
assert_json "$confirm_workbench_event" "data['recommended_command'] == 'operations-group-message-confirm'"

confirm_workbench_event_id="$(psql_value "SELECT id FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_recorded' AND data->>'external_event_id' = '${source_ref}:workbench-final-confirmation' ORDER BY created_at DESC LIMIT 1;")"
confirm_workbench_process_dry="$(run_json confirm_workbench_process_dry operations-workbench-event-process --event-id "$confirm_workbench_event_id" --dry-run)"
assert_json "$confirm_workbench_process_dry" "data['success'] is True"
assert_json "$confirm_workbench_process_dry" "data['action_status'] == 'dry_run_ok'"
assert_json "$confirm_workbench_process_dry" "data['command_executed'] == 'operations-group-message-confirm'"
assert_json "$confirm_workbench_process_dry" "data['state_mutation_recorded'] is False"

confirm_workbench_process="$(run_json confirm_workbench_process run-workbench-event-worker --once --event-id "$confirm_workbench_event_id" --apply)"
assert_json "$confirm_workbench_process" "data['success'] is True"
assert_json "$confirm_workbench_process" "data['action_status'] == 'processed'"
assert_json "$confirm_workbench_process" "data['process_report']['command_executed'] == 'operations-group-message-confirm'"
assert_json "$confirm_workbench_process" "data['process_report']['state_mutation_recorded'] is True"

confirm_workbench_process_again="$(run_json confirm_workbench_process_again run-workbench-event-worker --once --event-id "$confirm_workbench_event_id" --apply)"
assert_json "$confirm_workbench_process_again" "data['success'] is True"
assert_json "$confirm_workbench_process_again" "data['action_status'] == 'idempotent_existing'"

assert_sql_equals \
  confirm_workbench_processing_queued_group_request \
  queued \
  "SELECT status FROM qintopia_agent_os.work_items WHERE id = '${workbench_confirm_group_work_item_id}'::uuid;"

assert_sql_equals \
  confirm_workbench_processing_wrote_final_confirmation \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id = '${workbench_confirm_group_work_item_id}'::uuid AND event_type = 'group_message_final_confirmation_recorded';"

assert_sql_equals \
  confirm_workbench_processed_once \
  1 \
  "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'human_workbench_event_processed' AND data->>'source_event_id' = '${confirm_workbench_event_id}';"

denial_before="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
denial_stdout="$tmp_dir/denied.out"
denial_stderr="$tmp_dir/denied.err"
if "${BIN_CMD[@]}" operations-work-item-create --apply --payload-json '{"requester_agent":"unknown","target_agent":"huabaosi","capability_key":"huabaosi.create_visual_asset","work_item_type":"visual_asset_request","brief_summary":"AgentOS apply smoke denied request","source_type":"apply_smoke","source_refs":{"source_record_ref":"apply-smoke:denied-request"}}' >"$denial_stdout" 2>"$denial_stderr"; then
  echo "expected denied request to fail" >&2
  cat "$denial_stdout" >&2
  cat "$denial_stderr" >&2
  exit 1
fi
if ! grep -Fq "requester_agent is not allowed for capability" "$denial_stdout" && ! grep -Fq "requester_agent is not allowed for capability" "$denial_stderr"; then
  echo "expected denial output to mention requester policy" >&2
  cat "$denial_stdout" >&2
  cat "$denial_stderr" >&2
  exit 1
fi
denial_after="$(psql_value "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type = 'denied_by_policy';")"
if [[ "$denial_after" -le "$denial_before" ]]; then
  echo "expected denied_by_policy event count to increase" >&2
  echo "before=${denial_before} after=${denial_after}" >&2
  exit 1
fi

echo "operations control-plane apply smoke passed: work_item_id=${work_item_id}"
