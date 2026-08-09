#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED:-}" != "1" ]]; then
  echo "xiaoman daily case report auto-publish skipped: persistent enablement is not 1" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
WORKFLOW_PY="${RELEASE_DIR}/workflows/xiaoman-daily-case-report/daily_case_report.py"
SIDECAR_BIN="${RELEASE_DIR}/sidecar-profiles/qiwe-production/qintopia-message-sidecar"
PYTHON_BIN="/usr/bin/python3"
WORK_DIR="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_DIR:-/home/ubuntu/.local/state/qintopia-agentos/xiaoman-daily-case-report}"
BACKFILL_APPROVAL="approved-production-xiaoman-daily-case-report-auto-publish-backfill"

required_env() {
  local key="$1"
  local value="${!key:-}"
  if [[ -z "$value" ]]; then
    echo "xiaoman daily case report auto-publish requires ${key}" >&2
    exit 1
  fi
}

for key in \
  QINTOPIA_SIDECAR_DATABASE_URL \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID; do
  required_env "$key"
done

case "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND" in
  feishu-base)
    ;;
  https-public)
    for key in \
      QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_UPLOAD_ENDPOINT \
      QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL \
      QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_ALLOWED_HOSTS; do
      required_env "$key"
    done
    ;;
  *)
    echo "xiaoman daily case report storage backend is not reviewed" >&2
    exit 1
    ;;
esac

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE}" != "1" ]]; then
  echo "xiaoman daily case report production read-through must be explicitly enabled" >&2
  exit 1
fi

if [[ ! -f "$WORKFLOW_PY" ]]; then
  echo "xiaoman daily case report workflow is missing from release/current" >&2
  exit 1
fi

if [[ ! -x "$SIDECAR_BIN" ]]; then
  echo "xiaoman daily case report auto-publish requires the reviewed QiWe sidecar companion" >&2
  exit 1
fi

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "python3 is required for xiaoman daily case report rendering" >&2
  exit 1
fi

report_date_args=()
if [[ -n "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE:-}" ]]; then
  if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL:-}" != "$BACKFILL_APPROVAL" ]]; then
    echo "xiaoman daily case report date override requires explicit backfill approval" >&2
    exit 1
  fi
  if [[ ! "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE must be YYYY-MM-DD" >&2
    exit 1
  fi
  "$PYTHON_BIN" - "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE" <<'PY'
from __future__ import annotations

import sys
from datetime import datetime
from zoneinfo import ZoneInfo

try:
    requested = datetime.strptime(sys.argv[1], "%Y-%m-%d").date()
except ValueError as exc:
    raise SystemExit("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE is not a real calendar date") from exc

today = datetime.now(ZoneInfo("Asia/Shanghai")).date()
if requested > today:
    raise SystemExit("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE must not be in the future")
PY
  report_date_args=(--date "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE")
fi

mkdir -p "$WORK_DIR"
chmod 0700 "$WORK_DIR"
tmp_dir="$(mktemp -d "${WORK_DIR}/run.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

render_report="${tmp_dir}/render.json"
upload_report="${tmp_dir}/upload.json"
publish_report="${tmp_dir}/publish.json"

"$PYTHON_BIN" "$WORKFLOW_PY" \
  --render image \
  --image-format jpeg \
  "${report_date_args[@]}" \
  --chat-id "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID" \
  --output-dir "$tmp_dir" \
  --json >"$render_report"

upload_payload="$("$PYTHON_BIN" - "$render_report" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    rendered = json.load(fh)

candidate = rendered.get("artifact_candidate") or {}
image_path = rendered.get("image_path")
if not rendered.get("success") or not image_path:
    raise SystemExit("render did not produce a daily report image")
if candidate.get("mime_type") != "image/jpeg":
    raise SystemExit("rendered daily report image must be JPEG")

print(json.dumps({
    "image_path": image_path,
    "content_hash": candidate["content_hash"],
    "file_md5": candidate["file_md5"],
    "byte_size": candidate["byte_size"],
    "filename": candidate.get("filename", "xiaoman-daily-case-report.jpg"),
    "report_window": candidate.get("report_window") or {},
    "source_chat_ref": candidate.get("source_chat_ref") or {},
    "template_version": candidate.get("template_version", ""),
}, ensure_ascii=False))
PY
)"

"$SIDECAR_BIN" operations-daily-case-report-media-upload \
  --apply \
  --payload-json "$upload_payload" >"$upload_report"

publish_payload="$("$PYTHON_BIN" - "$render_report" "$upload_report" <<'PY'
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    rendered = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    uploaded = json.load(fh)

candidate = rendered.get("artifact_candidate") or {}
window = candidate.get("report_window") or {}
artifact_uri = uploaded.get("artifact_uri")
if not artifact_uri:
    raise SystemExit("media upload did not return artifact_uri")
media_upload_evidence = uploaded.get("media_upload_evidence")
if not isinstance(media_upload_evidence, dict):
    raise SystemExit("media upload did not return media_upload_evidence")

message_text = os.environ.get(
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MESSAGE_TEXT",
    "小满日报已自动生成。",
)

print(json.dumps({
    "window_start": window.get("start", ""),
    "window_end": window.get("end", ""),
    "report_date": window.get("display", rendered.get("report_date", "")),
    "time_range": window.get("time_range", rendered.get("time_range", "")),
    "artifact_uri": artifact_uri,
    "content_hash": uploaded["content_hash"],
    "file_md5": uploaded["file_md5"],
    "byte_size": uploaded["byte_size"],
    "mime_type": uploaded["mime_type"],
    "width": uploaded["width"],
    "height": uploaded["height"],
    "filename": uploaded["filename"],
    "media_upload_evidence": media_upload_evidence,
    "target_group_id": os.environ["QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID"],
    "message_text": message_text,
    "title": f"小满日报 {rendered.get('report_date', '')}".strip(),
    "summary": (
        f"消息 {rendered.get('message_count', 0)} 条 / "
        f"活跃 {rendered.get('participant_count', 0)} 人 / "
        f"案件 {rendered.get('case_count', 0)} 起"
    ),
    "source_chat_ref": candidate.get("source_chat_ref") or {},
    "template_version": candidate.get("template_version", ""),
    "metadata": {
        "created_by_command": "xiaoman-daily-case-report-auto-publish-worker",
        "render_width": (candidate.get("render") or {}).get("width"),
        "report_timezone": window.get("timezone"),
    },
}, ensure_ascii=False))
PY
)"

"$SIDECAR_BIN" operations-daily-case-report-auto-publish-create \
  --apply \
  --payload-json "$publish_payload" >"$publish_report"

"$PYTHON_BIN" - "$upload_report" "$publish_report" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    upload = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    publish = json.load(fh)

print(json.dumps({
    "success": publish.get("success") is True,
    "worker": "xiaoman-daily-case-report-auto-publish-worker",
    "media_uploaded": upload.get("action_status") == "media_uploaded",
    "auto_publish_created": publish.get("action_status") in {
        "auto_publish_created",
        "already_created",
    },
    "requires_human_final_confirmation": publish.get("requires_human_final_confirmation"),
    "send_ready_recorded": publish.get("send_ready_recorded"),
    "external_send_executed": publish.get("external_send_executed"),
    "artifact_type": publish.get("artifact_type"),
    "review_status": publish.get("review_status"),
    "content_hash": publish.get("content_hash"),
    "idempotency_key": publish.get("idempotency_key"),
}, ensure_ascii=False, indent=2))
PY
