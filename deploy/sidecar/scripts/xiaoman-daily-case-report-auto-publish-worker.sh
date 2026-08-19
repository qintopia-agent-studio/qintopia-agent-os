#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED:-}" != "1" ]]; then
  echo "xiaoman daily case report auto-publish skipped: persistent enablement is not 1" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
# The Rust daily-case-report pipeline resolves relative script paths (e.g.
# workflows/xiaoman-daily-case-report/rasterize.py) against the release root
# that owns the sidecar binary. The QiWe production profile binary sits at
# sidecar-profiles/<profile>/ - one level deeper than the default sidecar/
# layout - so the exe-parent fallback in resolve_release_path cannot find the
# release root on its own. Pin it explicitly so the pipeline always resolves
# scripts against release/current.
export QINTOPIA_AGENT_OS_RELEASE_CURRENT="${RELEASE_DIR}"
WORKFLOW_PY="${RELEASE_DIR}/workflows/xiaoman-daily-case-report/daily_case_report.py"
SIDECAR_BIN="${RELEASE_DIR}/sidecar-profiles/qiwe-production/qintopia-message-sidecar"
PYTHON_BIN="/usr/bin/python3"
PSQL_BIN="/usr/bin/psql"
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

if [[ ! -x "$SIDECAR_BIN" ]]; then
  echo "xiaoman daily case report auto-publish requires the reviewed QiWe sidecar companion" >&2
  exit 1
fi

if [[ "${USE_PYTHON_PIPELINE:-}" == "1" ]]; then
  if [[ ! -f "$WORKFLOW_PY" ]]; then
    echo "xiaoman daily case report workflow is missing from release/current" >&2
    exit 1
  fi

  if [[ ! -x "$PYTHON_BIN" ]]; then
    echo "python3 is required for xiaoman daily case report rendering" >&2
    exit 1
  fi
  if [[ ! -x "$PSQL_BIN" ]]; then
    echo "psql is required for xiaoman daily case report database read-through" >&2
    exit 1
  fi
  if ! "$PYTHON_BIN" - <<'PY' >/dev/null 2>&1; then
from PIL import Image, ImageDraw, ImageFont
PY
    echo "Pillow is required for xiaoman daily case report local image rendering" >&2
    exit 1
  fi
else
  "$SIDECAR_BIN" run-daily-case-report-auto-publish-worker --once --apply
  exit 0
fi

report_date_args=()
if [[ -n "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE:-}" ]]; then  if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL:-}" != "$BACKFILL_APPROVAL" ]]; then
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

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE:-}" == "1" ]]; then
  echo "xiaoman daily case report: using Python fallback pipeline" >&2

  "$PYTHON_BIN" "$WORKFLOW_PY" \
    --render image \
    --image-format jpeg \
    --template "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TEMPLATE:-roast-long-image}" \
    --narrative "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_NARRATIVE:-roast}" \
    "${report_date_args[@]}" \
    --chat-id "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID" \
    --output-dir "$tmp_dir" \
    --json \
    --json-summary-only >"$render_report"

  # Send quality gate: refuse to auto-publish a report that should never reach the
  # group. A render that is empty or off-template must stop here instead of being
  # uploaded and sent. This prevents blank/wrong-template reports from being
  # flushed to the group after an outage. Override only for an explicitly approved
  # recovery: ..._SEND_GATE_BYPASS=1.
  if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_SEND_GATE_BYPASS:-}" != "1" ]]; then
    SEND_GATE_TEMPLATE="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TEMPLATE:-roast-long-image}" \
    "$PYTHON_BIN" - "$render_report" <<'PY'
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    rendered = json.load(fh)

if not rendered.get("success"):
    raise SystemExit("send gate: render did not succeed")

template = os.environ.get("SEND_GATE_TEMPLATE", "roast-long-image")
message_count = int(rendered.get("message_count") or 0)
participant_count = int(rendered.get("participant_count") or 0)

if template != "roast-long-image":
    raise SystemExit(f"send gate: template {template!r} is not the approved roast-long-image")

if message_count <= 0:
    raise SystemExit("send gate: report has 0 messages; refusing to publish a blank report")
if participant_count <= 0:
    raise SystemExit("send gate: report has 0 participants; refusing to publish a blank report")
PY
  fi

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
content_metrics = candidate.get("content_metrics") or {}
character_universe = rendered.get("character_universe_summary") or {}
private_review_bundle = rendered.get("private_review_bundle") or {}
public_output_style = rendered.get("public_output_style") or {}
artifact_uri = uploaded.get("artifact_uri")
if not artifact_uri:
    raise SystemExit("media upload did not return artifact_uri")
media_upload_evidence = uploaded.get("media_upload_evidence")
if not isinstance(media_upload_evidence, dict):
    raise SystemExit("media upload did not return media_upload_evidence")

def _default_intro_text() -> str:
    report_date = rendered.get("report_date", "").strip()
    group_name = rendered.get("group_name", "").strip()
    message_count = rendered.get("message_count", 0)
    participant_count = rendered.get("participant_count", 0)
    date_part = report_date or "昨天"
    group_part = f"「{group_name}」" if group_name else "咱们群"
    return (
        f"小满日报来啦 📰 {date_part} {group_part}的群聊，"
        f"共 {message_count} 条消息、{participant_count} 位邻居发言。"
        f"昨天的新鲜事都在下面这张长图里，点开看看 👇"
    )


# Intro chat text delivered immediately before the report image. Operators may
# override with QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MESSAGE_TEXT; otherwise a
# dynamic line is built from the rendered report so the group knows what the
# image is before opening it.
message_text = os.environ.get(
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MESSAGE_TEXT",
    _default_intro_text(),
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
        f"案件 {rendered.get('case_count', 0)} 起 / "
        f"人物 {rendered.get('character_count', 0)} 位"
    ),
    "source_chat_ref": candidate.get("source_chat_ref") or {},
    "template_version": candidate.get("template_version", ""),
    "metadata": {
        "created_by_command": "xiaoman-daily-case-report-auto-publish-worker",
        "render_width": (candidate.get("render") or {}).get("width"),
        "report_timezone": window.get("timezone"),
        "content_metrics": {
            "message_count": content_metrics.get("message_count", rendered.get("message_count", 0)),
            "participant_count": content_metrics.get("participant_count", rendered.get("participant_count", 0)),
            "case_count": content_metrics.get("case_count", rendered.get("case_count", 0)),
            "character_count": content_metrics.get("character_count", rendered.get("character_count", 0)),
            "hot_topic_count": content_metrics.get("hot_topic_count", 0),
        },
        "character_universe": {
            "schema_version": character_universe.get("schema_version", ""),
            "source": character_universe.get("source", ""),
            "retained_source_policy": character_universe.get("retained_source_policy", ""),
            "raw_messages_included": character_universe.get("raw_messages_included") is True,
            "profile_fact_text_included": character_universe.get("profile_fact_text_included") is True,
            "people_count": character_universe.get("people_count", 0),
            "topic_count": character_universe.get("topic_count", 0),
            "event_count": character_universe.get("event_count", 0),
            "meme_count": character_universe.get("meme_count", 0),
            "callback_count": character_universe.get("callback_count", 0),
            "relationship_count": character_universe.get("relationship_count", 0),
            "expressive_label_candidate_count": character_universe.get("expressive_label_candidate_count", 0),
            "reviewed_public_expressive_label_count": character_universe.get("reviewed_public_expressive_label_count", 0),
            "creative_profile_candidate_count": character_universe.get("creative_profile_candidate_count", 0),
            "creative_profile_public_surface_allowed": character_universe.get("creative_profile_public_surface_allowed") is True,
            "creative_universe_candidate_count": character_universe.get("creative_universe_candidate_count", 0),
            "creative_universe_public_surface_allowed": character_universe.get("creative_universe_public_surface_allowed") is True,
            "unreviewed_expressive_labels_public_surface_allowed": character_universe.get("unreviewed_expressive_labels_public_surface_allowed") is True,
            "storyline_candidate_count": character_universe.get("storyline_candidate_count", 0),
            "edge_count": character_universe.get("edge_count", 0),
        },
        "public_output_style": {
            "schema_version": public_output_style.get("schema_version", ""),
            "character_daily_layout": public_output_style.get("character_daily_layout") is True,
            "storyline_first": public_output_style.get("storyline_first") is True,
            "cast_notes_enabled": public_output_style.get("cast_notes_enabled") is True,
            "meme_callback_section_enabled": public_output_style.get("meme_callback_section_enabled") is True,
            "relationship_section_enabled": public_output_style.get("relationship_section_enabled") is True,
            "owner_reviewed_expressive_labels_only": public_output_style.get("owner_reviewed_expressive_labels_only") is True,
            "image_first_delivery": public_output_style.get("image_first_delivery") is True,
            "pdf_default_delivery": public_output_style.get("pdf_default_delivery") is False,
            "roast_review_boundary": public_output_style.get("roast_review_boundary") is True,
            "private_draft_only": public_output_style.get("private_draft_only") is True,
            "public_surface_contains_private_draft": public_output_style.get("public_surface_contains_private_draft") is False,
        },
        "private_review_bundle": {
            "schema_version": private_review_bundle.get("schema_version", ""),
            "source": private_review_bundle.get("source", ""),
            "public_surface_allowed": private_review_bundle.get("public_surface_allowed") is True,
            "review_required": private_review_bundle.get("review_required") is True,
            "raw_message_rows_included": private_review_bundle.get("raw_message_rows_included") is True,
            "profile_fact_text_included": private_review_bundle.get("profile_fact_text_included") is True,
            "raw_message_payload_read": private_review_bundle.get("raw_message_payload_read") is True,
            "attachment_public_surface_allowed": private_review_bundle.get("attachment_public_surface_allowed") is True,
            "quote_map_entry_count": private_review_bundle.get("quote_map_entry_count", 0),
            "wiki_counts": private_review_bundle.get("wiki_counts") or {},
            "draft_counts": private_review_bundle.get("draft_counts") or {},
        },
    },
}, ensure_ascii=False))
PY
  )"

  "$SIDECAR_BIN" operations-daily-case-report-auto-publish-create \
    --apply \
    --payload-json "$publish_payload" >"$publish_report"

  "$PYTHON_BIN" - "$render_report" "$upload_report" "$publish_report" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    rendered = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    upload = json.load(fh)
with open(sys.argv[3], encoding="utf-8") as fh:
    publish = json.load(fh)

candidate = rendered.get("artifact_candidate") or {}
content_metrics = candidate.get("content_metrics") or {}
character_universe = rendered.get("character_universe_summary") or {}
private_review_bundle = rendered.get("private_review_bundle") or {}

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
    "content_metrics": {
        "message_count": content_metrics.get("message_count", rendered.get("message_count", 0)),
        "participant_count": content_metrics.get("participant_count", rendered.get("participant_count", 0)),
        "case_count": content_metrics.get("case_count", rendered.get("case_count", 0)),
        "character_count": content_metrics.get("character_count", rendered.get("character_count", 0)),
        "hot_topic_count": content_metrics.get("hot_topic_count", 0),
    },
    "character_universe": {
        "schema_version": character_universe.get("schema_version", ""),
        "source": character_universe.get("source", ""),
        "retained_source_policy": character_universe.get("retained_source_policy", ""),
        "raw_messages_included": character_universe.get("raw_messages_included") is True,
        "profile_fact_text_included": character_universe.get("profile_fact_text_included") is True,
        "people_count": character_universe.get("people_count", 0),
        "topic_count": character_universe.get("topic_count", 0),
        "event_count": character_universe.get("event_count", 0),
        "meme_count": character_universe.get("meme_count", 0),
        "callback_count": character_universe.get("callback_count", 0),
        "relationship_count": character_universe.get("relationship_count", 0),
        "creative_profile_candidate_count": character_universe.get("creative_profile_candidate_count", 0),
        "creative_profile_public_surface_allowed": character_universe.get("creative_profile_public_surface_allowed") is True,
        "storyline_candidate_count": character_universe.get("storyline_candidate_count", 0),
        "edge_count": character_universe.get("edge_count", 0),
    },
    "private_review_bundle": {
        "schema_version": private_review_bundle.get("schema_version", ""),
        "source": private_review_bundle.get("source", ""),
        "public_surface_allowed": private_review_bundle.get("public_surface_allowed") is True,
        "review_required": private_review_bundle.get("review_required") is True,
        "raw_message_rows_included": private_review_bundle.get("raw_message_rows_included") is True,
        "profile_fact_text_included": private_review_bundle.get("profile_fact_text_included") is True,
        "raw_message_payload_read": private_review_bundle.get("raw_message_payload_read") is True,
        "attachment_public_surface_allowed": private_review_bundle.get("attachment_public_surface_allowed") is True,
        "quote_map_entry_count": private_review_bundle.get("quote_map_entry_count", 0),
        "wiki_counts": private_review_bundle.get("wiki_counts") or {},
        "draft_counts": private_review_bundle.get("draft_counts") or {},
    },
}, ensure_ascii=False, indent=2))
PY
else
  echo "xiaoman daily case report: using Rust pipeline" >&2
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_DIR="$tmp_dir" \
    "$SIDECAR_BIN" run-daily-case-report-auto-publish-worker --once --apply
fi
