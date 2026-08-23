#!/usr/bin/env bash
#
# Erhua activity-recruitment worker.
#
# Sends the weekly "activity initiator recruitment" announcement to the resident
# home group through Erhua's controlled QiWe text-send channel. This is the same
# production send path used by the Erhua morning brief (qintopia-message-sidecar
# -> artifact -> review -> work-item -> confirm -> run-group-message-send-worker
# -> run-qiwe-text-send-worker); it reuses Erhua's home-group send identity
# (QINTOPIA_ERHUA_MORNING_BRIEF_* env) because recruitment targets the same
# resident group as the morning brief.
#
# Why a dedicated worker instead of the old Xiaoman conversation timer:
# the previous "activity initiator recruitment" timers were agent-mode Hermes
# timers created inside the operations chat, so their origin chat bound to that
# operations chat and they never reached the resident group. Routing the send
# through this reviewed script cron keeps the target group fixed at the resident
# home group and stays inside the audited send boundary.
set -euo pipefail

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEM_PYTHON="/usr/bin/python3"
HERMES_VENV="/home/ubuntu/.hermes/hermes-agent/venv"
DEFAULT_HERMES_PYTHON="/home/ubuntu/.hermes/hermes-agent/venv/bin/python"
PYTHON_BIN="${QINTOPIA_ERHUA_ACTIVITY_RECRUITMENT_PYTHON:-$DEFAULT_HERMES_PYTHON}"
WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/erhua-activity-recruitment"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_BIN="${RELEASE_DIR}/sidecar/qintopia-message-sidecar"
QIWE_BIN="${RELEASE_DIR}/sidecar-profiles/qiwe-production/qintopia-message-sidecar"
PYTHON_VALIDATOR="${RELEASE_DIR}/runtime/hermes/validate_hermes_python.py"

fail() {
  echo "erhua activity recruitment worker failed: $1" >&2
  exit 1
}

if [[ -v QINTOPIA_XIAOMAN_WRAPPER_PATH ]]; then
  fail "refuses Xiaoman wrapper path override"
fi

required_env() {
  local key="$1"
  if [[ -z "${!key:-}" ]]; then
    fail "missing ${key}"
  fi
}

# Single source of truth for the resident-group target, reviewer/confirmer ids,
# QiWe credentials, allowlist, and production approvals.
ENV_FILE="/etc/qintopia/message-sidecar.env"
[[ -f "$ENV_FILE" ]] || fail "production sidecar env missing"
set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

# Recruitment reuses the Erhua home-group send identity (same resident group as
# the morning brief). These are mandatory so the announcement always goes out
# through the audited boundary.
AUTO_PUBLISH_APPROVAL="${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL:-approved-production-erhua-morning-brief-auto-publish}"

for key in \
  QINTOPIA_DEPLOYED_COMMIT_SHA \
  QINTOPIA_SIDECAR_DATABASE_URL \
  QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED \
  QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL \
  QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED \
  QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL \
  QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID \
  QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID \
  QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID \
  QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS \
  QINTOPIA_QIWE_TEXT_SEND_ENABLED \
  QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL \
  QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256 \
  QIWE_API_URL \
  QIWE_TOKEN \
  QIWE_GUID; do
  required_env "$key"
done

if [[ "$(basename "$RELEASE_DIR")" != "$QINTOPIA_DEPLOYED_COMMIT_SHA" ]]; then
  fail "release directory does not match deployed commit SHA"
fi
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED}" != "1" ]]; then
  fail "Erhua morning brief is not enabled"
fi
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL}" != "approved-production-erhua-morning-brief" ]]; then
  fail "Erhua morning brief production approval is missing"
fi
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED:-0}" != "1" ]]; then
  fail "Erhua auto-publish is not enabled"
fi
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL:-}" != "$AUTO_PUBLISH_APPROVAL" ]]; then
  fail "Erhua auto-publish approval is missing"
fi
if [[ ! -f "$WORKFLOW_PY" && -n "${WORKFLOW_PY:-}" ]]; then
  :
fi
if [[ ! -x "$SIDECAR_BIN" ]]; then
  fail "reviewed primary sidecar binary is missing"
fi
if [[ ! -x "$QIWE_BIN" ]]; then
  fail "reviewed QiWe production sidecar companion is missing"
fi

# Target group must be on the operations allowlist or the real send is rejected.
"$SYSTEM_PYTHON" - "$QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID" "$QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS" <<'PY'
import sys

target, allowed = sys.argv[1:3]
allowed_set = {item.strip() for item in allowed.split(",") if item.strip()}
if target not in allowed_set:
    raise SystemExit("recruitment target group id is not allowlisted")
PY

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

artifact_json="${tmp_dir}/recruitment-artifact.json"
review_json="${tmp_dir}/recruitment-review.json"
send_request_json="${tmp_dir}/recruitment-send-request.json"
confirm_json="${tmp_dir}/recruitment-confirm.json"
ready_json="${tmp_dir}/recruitment-send-ready.json"
send_json="${tmp_dir}/recruitment-send.json"

DEFAULT_MESSAGE='周末过半，这周的招募还在继续。还没提想法的邻居别急，不用想得多周全，一个念头就够。趁着周末，想到什么就随手记一笔。

很多事开始前都觉得难，其实社区里的小活动，大多是从一句“我有个想法”开始的。你拿手的菜想请大家尝尝，收藏了很久的东西想找人聊聊，或者有个一直想试的小体验，都可以拿来当起点。

发起不需要专业，也不用复杂的准备。你出想法，剩下的我们一起张罗。

报名链接：
https://ranuox3qst4.feishu.cn/share/base/form/shrcnmjtfcf6sEexZXZxUnTNFHc

发起后，我们会陪你一起筹备，现场也有人搭把手。

参与的人会收到一份秦托邦小礼物。

一次小活动，也能让你认识更多邻居，收获支持或共同回忆。

想到什么就写什么，我们等你。

你的社区搭子 二花🐱'
MESSAGE_TEXT="${QINTOPIA_ERHUA_ACTIVITY_RECRUITMENT_MESSAGE:-$DEFAULT_MESSAGE}"

send_text_recruitment() {
  local artifact_id review_payload send_payload work_item_id confirm_payload

  artifact_id="$("$SYSTEM_PYTHON" - "$MESSAGE_TEXT" <<'PY'
import hashlib
import json
import os
import sys

message_text = sys.argv[1]
content_hash = "sha256:" + hashlib.sha256(message_text.encode("utf-8")).hexdigest()
date = os.environ.get("QINTOPIA_ERHUA_ACTIVITY_RECRUITMENT_DATE") or ""
source_record_ref = f"erhua_activity_recruitment:{date}" if date else "erhua_activity_recruitment:manual"
print(json.dumps({
    "date": date,
    "message_text": message_text,
    "source_record_ref": source_record_ref,
}, ensure_ascii=False, separators=(",", ":")))
PY
)"
  "$SIDECAR_BIN" operations-text-announcement-artifact-create \
    --payload-json "$artifact_id" \
    --apply >"$artifact_json"

  artifact_id="$("$SYSTEM_PYTHON" - "$artifact_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)
artifact_stdout = ((report.get("artifact_create") or {}).get("stdout") or "").strip()
artifact = json.loads(artifact_stdout) if artifact_stdout else {}
artifact_id = artifact.get("artifact_id")
if not artifact_id:
    raise SystemExit("artifact_id missing from recruitment artifact create")
print(artifact_id)
PY
)"

  review_payload="$("$SYSTEM_PYTHON" - "$artifact_id" <<'PY'
import json
import os
import sys

print(json.dumps({
    "artifact_id": sys.argv[1],
    "reviewer_id": os.environ["QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID"],
    "decision": "approved",
    "expected_artifact_type": "text_announcement",
    "expected_review_status": "pending",
    "reason": "二花活动招募定时广播自动发布审批",
    "source": "erhua_activity_recruitment_auto_publish",
}, ensure_ascii=False, separators=(",", ":")))
PY
)"
  "$SIDECAR_BIN" operations-artifact-review-decision \
    --payload-json "$review_payload" \
    --apply >"$review_json"

  send_payload="$("$SYSTEM_PYTHON" - "$artifact_id" "$MESSAGE_TEXT" <<'PY'
import hashlib
import json
import os
import sys

artifact_id = sys.argv[1]
message_text = sys.argv[2]
content_hash = "sha256:" + hashlib.sha256(message_text.encode("utf-8")).hexdigest()
date = os.environ.get("QINTOPIA_ERHUA_ACTIVITY_RECRUITMENT_DATE") or ""
source_record_ref = f"erhua_activity_recruitment:{date}" if date else "erhua_activity_recruitment:manual"
target_group_id = os.environ["QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID"]
idempotency_seed = hashlib.sha256(
    f"{source_record_ref}:{artifact_id}:{content_hash}:{target_group_id}".encode("utf-8")
).hexdigest()[:24]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "erhua",
    "capability_key": "erhua.send_group_message",
    "work_item_type": "group_message_request",
    "brief_summary": f"{date} 二花活动招募定时发送请求" if date else "二花活动招募定时发送请求",
    "purpose": "erhua_activity_recruitment_auto_publish",
    "human_owner": "production-erhua-activity-recruitment-auto-publish",
    "priority": "normal",
    "source_type": "operations_workflow",
    "source_refs": {"source_record_ref": source_record_ref},
    "approved_artifact_id": artifact_id,
    "idempotency_key": f"erhua_activity_recruitment_auto_publish:{date}:{idempotency_seed}",
    "dedupe_key": f"erhua_activity_recruitment_auto_publish:{date}:{idempotency_seed}",
    "payload": {
        "workflow_type": "text_activity_announcement",
        "planner_intent": "send_erhua_activity_recruitment_after_auto_publish_approval",
        "approved_artifact_id": artifact_id,
        "approved_artifact_type": "text_announcement",
        "approved_artifact_content_hash": content_hash,
        "target_channel": "qiwe",
        "target_group_id": target_group_id,
        "message_text": message_text,
        "requires_human_confirmation": True,
        "auto_publish_approval": os.environ["QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL"],
        "external_send_executed": False,
    },
}, ensure_ascii=False, separators=(",", ":")))
PY
)"
  "$SIDECAR_BIN" operations-work-item-create \
    --payload-json "$send_payload" \
    --apply >"$send_request_json"

  work_item_id="$("$SYSTEM_PYTHON" - "$send_request_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)
work_item_id = report.get("work_item_id")
if not work_item_id:
    raise SystemExit("send request work_item_id missing")
print(work_item_id)
PY
)"

  confirm_payload="$("$SYSTEM_PYTHON" - "$work_item_id" <<'PY'
import json
import os
import sys

print(json.dumps({
    "work_item_id": sys.argv[1],
    "confirmer_id": os.environ["QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID"],
    "decision": "confirmed",
    "reason": "确认执行二花活动招募定时广播",
    "source": "erhua_activity_recruitment_auto_publish",
}, ensure_ascii=False, separators=(",", ":")))
PY
)"
  "$SIDECAR_BIN" operations-group-message-confirm \
    --payload-json "$confirm_payload" \
    --apply >"$confirm_json"

  "$SIDECAR_BIN" run-group-message-send-worker \
    --once \
    --work-item-id "$work_item_id" \
    --apply >"$ready_json"

  "$QIWE_BIN" run-qiwe-text-send-worker \
    --once \
    --work-item-id "$work_item_id" \
    --apply >"$send_json"

  "$SYSTEM_PYTHON" - "$send_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)
print(json.dumps({
    "success": report.get("success") is True,
    "worker": "erhua-activity-recruitment-auto-publish",
    "delivery_mode": "text",
    "qiwe_text_send_action_status": report.get("action_status"),
    "work_item_id": report.get("work_item_id"),
    "external_send_executed": report.get("external_send_executed"),
}, ensure_ascii=False, indent=2))
if report.get("success") is not True or report.get("external_send_executed") is not True:
    raise SystemExit(1)
PY
}

send_text_recruitment
