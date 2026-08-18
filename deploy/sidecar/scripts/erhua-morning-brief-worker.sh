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

if [[ -v QINTOPIA_XIAOMAN_WRAPPER_PATH ]]; then
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
if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED:-0}" == "1" ]]; then
  if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL:-}" != "$AUTO_PUBLISH_APPROVAL" ]]; then
    fail "Erhua morning brief auto-publish approval is missing"
  fi
  for key in \
    QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID \
    QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS \
    QINTOPIA_QIWE_IMAGE_SEND_ENABLED \
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL \
    QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256 \
    QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS \
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
    required_env "$key"
  done
  "$SYSTEM_PYTHON" - "$QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID" "$QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS" <<'PY'
import sys

target, allowed = sys.argv[1:3]
allowed_set = {item.strip() for item in allowed.split(",") if item.strip()}
if target not in allowed_set:
    raise SystemExit("Erhua morning brief target group id is not allowlisted")
PY
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

PYTHONDONTWRITEBYTECODE=1 "$PYTHON_BIN" "$WORKFLOW_PY" \
  --sidecar-bin "$SIDECAR_BIN" \
  --prepare-artifact \
  --execute-artifact-create \
  --apply-artifact-create \
  --publish-plan \
  --allow-news-unavailable \
  --render-image "$card_image" \
  --render-image-format jpeg \
  --json >"$report_json"

"$SYSTEM_PYTHON" - "$report_json" <<'PY' || fail "morning brief card image render failed or missing"
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    report = json.load(fh)

image_path = (report.get("rendered_image_path") or "").strip()
if not image_path:
    raise SystemExit("rendered_image_path is empty")
if not os.path.isfile(image_path):
    raise SystemExit("rendered card image file is missing")
PY

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

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED:-0}" == "1" ]]; then
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
message_text = (report.get("morning_brief_text") or "").strip()
if not message_text:
    raise SystemExit("morning brief message text missing")

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

  "$SYSTEM_PYTHON" - "$upload_json" "$publish_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    upload = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    publish = json.load(fh)

success = (
    publish.get("success") is True
    and upload.get("action_status") == "media_uploaded"
    and publish.get("action_status") == "auto_publish_send_ready_recorded"
)
print(json.dumps({
    "success": success,
    "worker": "erhua-morning-brief-auto-publish",
    "media_uploaded": upload.get("action_status") == "media_uploaded",
    "auto_publish_created": publish.get("action_status") == "auto_publish_send_ready_recorded",
    "source_work_item_id": publish.get("source_work_item_id"),
    "send_work_item_id": publish.get("send_work_item_id"),
    "artifact_id": publish.get("artifact_id"),
    "artifact_type": publish.get("artifact_type"),
    "review_status": publish.get("review_status"),
    "requires_human_final_confirmation": publish.get("requires_human_final_confirmation"),
    "send_ready_recorded": publish.get("send_ready_recorded"),
    "external_send_executed": publish.get("external_send_executed"),
    "content_hash": publish.get("content_hash"),
    "idempotency_key": publish.get("idempotency_key"),
}, ensure_ascii=False, indent=2))
if not success:
    raise SystemExit(1)
PY
fi
