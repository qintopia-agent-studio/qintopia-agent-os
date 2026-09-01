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
QIWE_BIN="${RELEASE_DIR}/sidecar-profiles/qiwe-production/qintopia-message-sidecar"
PYTHON_VALIDATOR="${RELEASE_DIR}/runtime/hermes/validate_hermes_python.py"
AUTO_PUBLISH_APPROVAL="approved-production-erhua-morning-brief-auto-publish"

fail() {
  echo "erhua morning brief worker failed: $1" >&2
  exit 1
}

if [[ "${QINTOPIA_XIAOMAN_WRAPPER_PATH+x}" == "x" ]]; then
  fail "refuses Xiaoman wrapper path override"
fi

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
  QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE \
  QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE; do
  required_env "$key"
done

export QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE
export QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE
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
if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE}" != "1" ]]; then
  fail "Xiaoman activity Feishu Base mode is not enabled"
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
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED:-0}" == "1" && ! -x "$QIWE_BIN" ]]; then
  fail "reviewed QiWe production sidecar companion is missing"
fi

# Card delivery is an enhancement on top of the text brief. The text fallback
# env (QiWe text-send gate, auto reviewer/confirmer, QiWe credentials) stays
# mandatory so the brief can always go out; the card-only env (image-send gate
# plus the Huabaosi Feishu mirror set) is optional and only gates whether we
# attempt the card at all. Missing card env degrades to text, never fails.
card_env_ready=false
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED:-0}" == "1" ]]; then
  if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL:-}" != "$AUTO_PUBLISH_APPROVAL" ]]; then
    fail "Erhua morning brief auto-publish approval is missing"
  fi
  for key in \
    QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID \
    QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_REVIEWER_ID \
    QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_CONFIRMER_ID \
    QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS \
    QINTOPIA_QIWE_TEXT_SEND_ENABLED \
    QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL \
    QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256 \
    QIWE_API_URL \
    QIWE_TOKEN \
    QIWE_GUID \
    QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS; do
    required_env "$key"
  done
  "$SYSTEM_PYTHON" - "$QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID" "$QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS" <<'PY'
import sys

target, allowed = sys.argv[1:3]
allowed_set = {item.strip() for item in allowed.split(",") if item.strip()}
if target not in allowed_set:
    raise SystemExit("Erhua morning brief target group id is not allowlisted")
PY
  card_env_ready=true
  for key in \
    QINTOPIA_QIWE_IMAGE_SEND_ENABLED \
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL \
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256 \
    QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED \
    QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL \
    QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA \
    QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256 \
    QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN \
    QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS \
    QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID \
    QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS \
    QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH \
    QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION; do
    if [[ -z "${!key:-}" ]]; then
      echo "erhua morning brief worker: ${key} missing; card image disabled, falling back to text brief" >&2
      card_env_ready=false
    fi
  done
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
card_image="${tmp_dir}/morning-brief-card.jpg"
upload_json="${tmp_dir}/media-upload.json"
publish_json="${tmp_dir}/card-publish.json"
send_request_json="${tmp_dir}/send-request.json"
review_json="${tmp_dir}/artifact-review.json"
confirm_json="${tmp_dir}/final-confirmation.json"
ready_json="${tmp_dir}/send-ready.json"
send_json="${tmp_dir}/qiwe-text-send.json"

# Rendering is fail-closed: on any failure (or a card taller than the storage
# cap) the workflow leaves rendered_image_path empty and still succeeds, so the
# brief itself never dies here.
PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" "$WORKFLOW_PY" \
  --sidecar-bin "$SIDECAR_BIN" \
  --prepare-artifact \
  --execute-artifact-create \
  --apply-artifact-create \
  --publish-plan \
  --allow-news-unavailable \
  --news-limit 8 \
  --news-recency-days 14 \
  --news-dedup-days 7 \
  --news-history-path "$WORK_DIR/news-history.json" \
  --render-image "$card_image" \
  --render-image-format jpeg \
  --json >"$report_json"

# Card-first gate: attempt the card only when every card prerequisite held.
# A failed/missing/oversized render degrades to the text brief instead of
# aborting the run.
use_card=false
if [[ "$card_env_ready" == "true" ]]; then
  card_image_check_output="$("$SYSTEM_PYTHON" - "$report_json" <<'PY'
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)

image_path = (report.get("rendered_image_path") or "").strip()
if not image_path:
    print("rendered_image_path is empty")
    raise SystemExit(1)
if not os.path.isfile(image_path):
    print(f"rendered card image file is missing: {image_path}")
    raise SystemExit(1)
print(f"rendered card image exists: {image_path}")
PY
  )" || true
  if [[ "$card_image_check_output" == rendered*exists* ]]; then
    use_card=true
  else
    echo "erhua morning brief worker: card image unavailable (${card_image_check_output:-unknown reason}); falling back to text brief" >&2
  fi
fi

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

send_text_brief() {
  local artifact_id review_payload send_payload work_item_id confirm_payload

  artifact_id="$("$SYSTEM_PYTHON" - "$report_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)
artifact_stdout = ((report.get("artifact_create") or {}).get("stdout") or "").strip()
artifact = json.loads(artifact_stdout) if artifact_stdout else {}
artifact_id = artifact.get("artifact_id")
if not artifact_id:
    raise SystemExit("artifact_id missing from morning brief artifact create")
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
    "reason": "二花早报 08:10 生产自动发布审批",
    "source": "erhua_morning_brief_auto_publish",
}, ensure_ascii=False, separators=(",", ":")))
PY
)"
  "$SIDECAR_BIN" operations-artifact-review-decision \
    --payload-json "$review_payload" \
    --apply >"$review_json"

  send_payload="$("$SYSTEM_PYTHON" - "$report_json" "$artifact_id" <<'PY'
import hashlib
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)

artifact_id = sys.argv[2]
message_text = report["morning_brief_text"]
content_hash = "sha256:" + hashlib.sha256(message_text.encode("utf-8")).hexdigest()
date = report["date"]
source_record_ref = f"erhua_morning_brief:{date}"
target_group_id = os.environ["QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID"]
idempotency_seed = hashlib.sha256(
    f"{source_record_ref}:{artifact_id}:{content_hash}:{target_group_id}".encode("utf-8")
).hexdigest()[:24]
print(json.dumps({
    "requester_agent": "xiaoman",
    "target_agent": "erhua",
    "capability_key": "erhua.send_group_message",
    "work_item_type": "group_message_request",
    "brief_summary": f"{date} 二花早报自动发送请求",
    "purpose": "erhua_morning_brief_auto_publish",
    "human_owner": "production-erhua-morning-brief-auto-publish",
    "priority": "normal",
    "source_type": "operations_workflow",
    "source_refs": {"source_record_ref": source_record_ref},
    "approved_artifact_id": artifact_id,
    "idempotency_key": f"erhua_morning_brief_auto_publish:{date}:{idempotency_seed}",
    "dedupe_key": f"erhua_morning_brief_auto_publish:{date}:{idempotency_seed}",
    "payload": {
        "workflow_type": "text_activity_announcement",
        "planner_intent": "send_erhua_morning_brief_after_auto_publish_approval",
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
    "reason": "确认执行二花早报 08:10 自动发布",
    "source": "erhua_morning_brief_auto_publish",
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
    "worker": "erhua-morning-brief-auto-publish",
    "delivery_mode": "text",
    "qiwe_text_send_action_status": report.get("action_status"),
    "work_item_id": report.get("work_item_id"),
    "external_send_executed": report.get("external_send_executed"),
}, ensure_ascii=False, indent=2))
if report.get("success") is not True or report.get("external_send_executed") is not True:
    raise SystemExit(1)
PY
}

send_card_brief() {
  local upload_payload publish_payload

  upload_payload="$("$SYSTEM_PYTHON" - "$report_json" <<'PY'
import hashlib
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)

image_path = (report.get("rendered_image_path") or "").strip()
if not image_path or not os.path.isfile(image_path):
    raise SystemExit("rendered card image is missing")
brief_date = (report.get("date") or "").strip()
if not brief_date:
    raise SystemExit("brief date missing from morning brief report")

with open(image_path, "rb") as fh:
    blob = fh.read()
if not blob:
    raise SystemExit("rendered card image is empty")

print(json.dumps({
    "image_path": image_path,
    "content_hash": "sha256:" + hashlib.sha256(blob).hexdigest(),
    "file_md5": hashlib.md5(blob).hexdigest(),
    "byte_size": len(blob),
    "filename": f"erhua-morning-brief-card-{brief_date}.jpg",
    "brief_date": brief_date,
    "source_record_ref": f"erhua_morning_brief:{brief_date}",
}, ensure_ascii=False, separators=(",", ":")))
PY
)"
  "$SIDECAR_BIN" operations-erhua-morning-brief-media-upload \
    --payload-json "$upload_payload" \
    --apply >"$upload_json"

  if ! "$SYSTEM_PYTHON" - "$upload_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    upload = json.load(fh)

if upload.get("success") is True and upload.get("action_status") == "media_uploaded":
    sys.exit(0)

print(
    f"media-upload success={upload.get('success')} "
    f"action_status={upload.get('action_status')} "
    f"error={upload.get('error') or upload.get('message') or 'unknown'}"
)
sys.exit(1)
PY
  then
    echo "erhua morning brief worker: card media-upload did not succeed; falling back to text brief" >&2
    return 1
  fi

  publish_payload="$("$SYSTEM_PYTHON" - "$report_json" "$upload_json" <<'PY'
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    upload = json.load(fh)

if upload.get("success") is not True or upload.get("action_status") != "media_uploaded":
    raise SystemExit("card media upload did not succeed")
artifact_uri = upload.get("artifact_uri")
if not artifact_uri:
    raise SystemExit("media upload did not return artifact_uri")
media_upload_evidence = upload.get("media_upload_evidence")
if not isinstance(media_upload_evidence, dict):
    raise SystemExit("media upload did not return media_upload_evidence")

brief_date = (report.get("date") or "").strip()
if not brief_date:
    raise SystemExit("brief date missing from morning brief report")
# The full brief lives inside the rendered card image; message_text is only a
# short caption that card-publish-create caps at 500 characters. Passing the
# whole morning_brief_text here (5+ translated news items easily exceeds 500
# chars) made the publish fail with "message_text must be 500 characters or
# fewer" and the worker degrade the card to the text brief.
message_text = f"二花早报 {brief_date} 已生成，完整内容见卡片。"
if len(message_text) > 500:
    raise SystemExit("card caption exceeds the 500-character sidecar cap")

print(json.dumps({
    "brief_date": brief_date,
    "source_record_ref": f"erhua_morning_brief:{brief_date}",
    "artifact_uri": artifact_uri,
    "content_hash": upload["content_hash"],
    "file_md5": upload["file_md5"],
    "byte_size": upload["byte_size"],
    "mime_type": upload["mime_type"],
    "width": upload["width"],
    "height": upload["height"],
    "filename": upload["filename"],
    "target_group_id": os.environ["QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID"],
    "message_text": message_text,
    "title": f"二花早报 {brief_date}",
    "media_upload_evidence": media_upload_evidence,
    "metadata": {"created_by_command": "erhua-morning-brief-worker"},
}, ensure_ascii=False, separators=(",", ":")))
PY
)"
  "$SIDECAR_BIN" operations-erhua-morning-brief-card-publish-create \
    --payload-json "$publish_payload" \
    --apply >"$publish_json"

  if ! "$SYSTEM_PYTHON" - "$publish_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    publish = json.load(fh)

if publish.get("send_work_item_id"):
    sys.exit(0)

print(
    f"card-publish success={publish.get('success')} "
    f"action_status={publish.get('action_status')} "
    f"error={publish.get('error') or publish.get('message') or 'unknown'} "
    f"send_work_item_id={publish.get('send_work_item_id')}"
)
sys.exit(1)
PY
  then
    echo "erhua morning brief worker: card publish-create did not commit send work item; falling back to text brief" >&2
    return 1
  fi

  # The publish-create command commits the card send work item only after its
  # transaction succeeds; send_work_item_id is present in the report ONLY then
  # (dry-run / failure return no committed send work item). Once committed, the
  # card is queued for the QiWe image-send worker and WILL be sent regardless of
  # what this script does next. The ONLY safe signal that the card is already in
  # flight is a present send_work_item_id. If it is missing the publish clearly
  # did not create a send work item, so text fallback is safe. We must never fall
  # back to text when send_work_item_id is present, or the group would receive
  # both the card and the text brief on the same day. This intentionally does NOT
  # re-validate publish/success action_status strings, which could drift and
  # cause a misjudged fallback while the card is already queued.
  local card_sent_ok=false
  if "$SYSTEM_PYTHON" - "$upload_json" "$publish_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    upload = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    publish = json.load(fh)

# Authoritative signal: the card send work item was durably committed.
send_work_item_id = publish.get("send_work_item_id")
if not send_work_item_id:
    raise SystemExit(1)

print(json.dumps({
    "success": True,
    "worker": "erhua-morning-brief-auto-publish",
    "delivery_mode": "card",
    "card_send_work_item_id": send_work_item_id,
    "source_work_item_id": publish.get("source_work_item_id"),
    "artifact_id": publish.get("artifact_id"),
    "artifact_type": publish.get("artifact_type"),
    "review_status": publish.get("review_status"),
    "idempotency_key": publish.get("idempotency_key"),
    "content_hash": publish.get("content_hash"),
    "send_ready_recorded": publish.get("send_ready_recorded"),
}, ensure_ascii=False, indent=2))
PY
  then
    card_sent_ok=true
  fi

  if [[ "$card_sent_ok" == "true" ]]; then
    return 0
  fi
  return 1
}

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED:-0}" == "1" ]]; then
  card_sent=false
  if [[ "$use_card" == "true" ]]; then
    # The card is an enhancement: any failure in the card chain degrades to
    # the text brief so the morning brief always goes out.
    if send_card_brief; then
      card_sent=true
    else
      echo "erhua morning brief worker: card delivery failed; falling back to text brief" >&2
    fi
  fi
  if [[ "$card_sent" != "true" ]]; then
    send_text_brief
  fi
fi
