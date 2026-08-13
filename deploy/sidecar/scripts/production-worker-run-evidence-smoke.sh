#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE:-}" != "1" ]]; then
  echo "production worker-run evidence skipped: set QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE=1" >&2
  exit 0
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"

target="${1:-}"
evidence_key=""
task_name=""
log_path=""
summary_path=""
expected_worker=""

case "$target" in
  erhua-morning-brief-worker-run)
    evidence_key="erhua_morning_brief"
    task_name="erhua-morning-brief"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/erhua-morning-brief/hermes-cron.log"
    ;;
  xiaoman-daily-case-report-worker-run)
    evidence_key="xiaoman_daily_case_report"
    task_name="xiaoman-daily-case-report"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-daily-case-report/hermes-cron.log"
    ;;
  xiaoman-weekly-recruitment-worker-run)
    evidence_key="xiaoman_weekly_recruitment"
    task_name="xiaoman-weekly-recruitment"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/hermes-cron.log"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/latest-summary.json"
    expected_worker="xiaoman-weekly-recruitment-worker"
    ;;
  xiaoman-weekly-plan-confirmation-worker-run)
    evidence_key="xiaoman_weekly_plan_confirmation"
    task_name="xiaoman-weekly-plan-confirmation"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/hermes-cron.log"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/latest-summary.json"
    expected_worker="xiaoman-weekly-plan-confirmation-worker"
    ;;
  xiaoman-weekly-preview-worker-run)
    evidence_key="xiaoman_weekly_preview"
    task_name="xiaoman-weekly-preview"
    log_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview/hermes-cron.log"
    summary_path="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview/latest-summary.json"
    expected_worker="xiaoman-weekly-preview-worker"
    ;;
  *)
    echo "unsupported production worker-run evidence target: ${target}" >&2
    exit 2
    ;;
esac

fail_evidence() {
  local reason="$1"
  echo "${evidence_key}_worker_run_error=${reason}"
  exit 1
}

if [[ ! -f "$log_path" ]]; then
  echo "${evidence_key}_worker_run_result=not_started"
  exit 0
fi

if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  fail_evidence "python_unavailable"
fi

run_epoch=""
set +e
run_epoch="$("$PYTHON_BIN" - "$log_path" "$task_name" <<'PY'
import datetime as dt
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
task = sys.argv[2]
pattern = re.compile(
    r"^(?P<ts>[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z) "
    + re.escape(task)
    + r" run=(?P<status>ok|failed)(?: exit=[0-9]+)?$"
)

latest = None
try:
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for raw_line in handle:
            match = pattern.fullmatch(raw_line.strip())
            if match:
                latest = match
except OSError:
    raise SystemExit(3)

if latest is None:
    raise SystemExit(2)
if latest.group("status") != "ok":
    raise SystemExit(1)

try:
    timestamp = dt.datetime.strptime(
        latest.group("ts"), "%Y-%m-%dT%H:%M:%SZ"
    ).replace(tzinfo=dt.timezone.utc)
except ValueError:
    raise SystemExit(1)
print(int(timestamp.timestamp()))
PY
)"; parse_status=$?
set -e
case "$parse_status" in
  0)
    ;;
  2)
    echo "${evidence_key}_worker_run_result=not_started"
    exit 0
    ;;
  *)
    fail_evidence "worker_failed"
    ;;
esac

summary_date=""
daily_case_report_summary=""
if [[ "$target" == "xiaoman-daily-case-report-worker-run" ]]; then
  if ! daily_case_report_summary="$("$PYTHON_BIN" - "$log_path" "$task_name" "$evidence_key" <<'PY'
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
task = sys.argv[2]
key = sys.argv[3]
pattern = re.compile(
    r"^(?P<ts>[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z) "
    + re.escape(task)
    + r" run=(?P<status>ok|failed)(?: exit=[0-9]+)?$"
)
sentinel_pattern = re.compile(
    r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z "
    r"[A-Za-z0-9_.-]+ run=(?:ok|failed)(?: exit=[0-9]+)?$"
)

try:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
except OSError:
    raise SystemExit(1)

latest_index = None
for index, line in enumerate(lines):
    match = pattern.fullmatch(line.strip())
    if match and match.group("status") == "ok":
        latest_index = index

if latest_index is None:
    raise SystemExit(1)

body_lines = lines[latest_index + 1 :]
next_sentinel = next(
    (index for index, line in enumerate(body_lines) if sentinel_pattern.fullmatch(line.strip())),
    len(body_lines),
)
body_lines = body_lines[:next_sentinel]
json_start = next(
    (index for index, line in enumerate(body_lines) if line.lstrip().startswith("{")),
    None,
)
if json_start is None:
    print(f"{key}_worker_summary_present=false")
    raise SystemExit(0)

text = "\n".join(body_lines[json_start:])
try:
    data, _ = json.JSONDecoder().raw_decode(text)
except Exception:
    raise SystemExit(2)

if not isinstance(data, dict):
    raise SystemExit(2)
if data.get("worker") != "xiaoman-daily-case-report-auto-publish-worker":
    raise SystemExit(2)

metrics = data.get("content_metrics")
universe = data.get("character_universe")
review_bundle = data.get("private_review_bundle")
if not isinstance(metrics, dict) or not isinstance(universe, dict):
    raise SystemExit(2)
if universe.get("raw_messages_included") is not False:
    raise SystemExit(3)
if universe.get("profile_fact_text_included") is not False:
    raise SystemExit(3)
if universe.get("creative_profile_public_surface_allowed") is not False:
    raise SystemExit(3)
if review_bundle is not None:
    if not isinstance(review_bundle, dict):
        raise SystemExit(2)
    if review_bundle.get("public_surface_allowed") is not False:
        raise SystemExit(3)
    if review_bundle.get("review_required") is not True:
        raise SystemExit(3)
    if review_bundle.get("raw_message_rows_included") is not False:
        raise SystemExit(3)
    if review_bundle.get("profile_fact_text_included") is not False:
        raise SystemExit(3)

def number(name: str, default: int = 0) -> int:
    value = metrics.get(name, default)
    if isinstance(value, bool):
        raise SystemExit(2)
    try:
        parsed = int(value)
    except Exception:
        raise SystemExit(2)
    if parsed < 0 or parsed > 100000:
        raise SystemExit(2)
    return parsed

def universe_count(name: str) -> int:
    value = universe.get(name, 0)
    if isinstance(value, bool):
        raise SystemExit(2)
    try:
        parsed = int(value)
    except Exception:
        raise SystemExit(2)
    if parsed < 0 or parsed > 100000:
        raise SystemExit(2)
    return parsed

def safe_label(name: str) -> str:
    value = universe.get(name, "")
    if not isinstance(value, str):
        raise SystemExit(2)
    if value and not re.fullmatch(r"[A-Za-z0-9_.-]{1,80}", value):
        raise SystemExit(2)
    return value

def review_bundle_count(name: str) -> int:
    if not isinstance(review_bundle, dict):
        return 0
    value = review_bundle.get(name, 0)
    if isinstance(value, bool):
        raise SystemExit(2)
    try:
        parsed = int(value)
    except Exception:
        raise SystemExit(2)
    if parsed < 0 or parsed > 100000:
        raise SystemExit(2)
    return parsed

def wiki_count(name: str) -> int:
    if not isinstance(review_bundle, dict):
        return 0
    counts = review_bundle.get("wiki_counts") or {}
    if not isinstance(counts, dict):
        raise SystemExit(2)
    value = counts.get(name, 0)
    if isinstance(value, bool):
        raise SystemExit(2)
    try:
        parsed = int(value)
    except Exception:
        raise SystemExit(2)
    if parsed < 0 or parsed > 100000:
        raise SystemExit(2)
    return parsed

def draft_count(name: str) -> int:
    if not isinstance(review_bundle, dict):
        return 0
    counts = review_bundle.get("draft_counts") or {}
    if not isinstance(counts, dict):
        raise SystemExit(2)
    value = counts.get(name, 0)
    if isinstance(value, bool):
        raise SystemExit(2)
    try:
        parsed = int(value)
    except Exception:
        raise SystemExit(2)
    if parsed < 0 or parsed > 100000:
        raise SystemExit(2)
    return parsed

print(f"{key}_worker_summary_present=true")
print(f"{key}_worker_message_count={number('message_count')}")
print(f"{key}_worker_participant_count={number('participant_count')}")
print(f"{key}_worker_case_count={number('case_count')}")
print(f"{key}_worker_character_count={number('character_count')}")
print(f"{key}_worker_hot_topic_count={number('hot_topic_count')}")
print(f"{key}_worker_character_universe_schema_version={safe_label('schema_version')}")
print(f"{key}_worker_character_universe_source={safe_label('source')}")
print(f"{key}_worker_character_universe_raw_messages_included=false")
print(f"{key}_worker_character_universe_profile_fact_text_included=false")
print(f"{key}_worker_character_universe_people_count={universe_count('people_count')}")
print(f"{key}_worker_character_universe_topic_count={universe_count('topic_count')}")
print(f"{key}_worker_character_universe_event_count={universe_count('event_count')}")
print(f"{key}_worker_character_universe_meme_count={universe_count('meme_count')}")
print(f"{key}_worker_character_universe_callback_count={universe_count('callback_count')}")
print(f"{key}_worker_character_universe_relationship_count={universe_count('relationship_count')}")
print(f"{key}_worker_character_universe_creative_profile_candidate_count={universe_count('creative_profile_candidate_count')}")
print(f"{key}_worker_character_universe_creative_profile_public_surface_allowed=false")
print(f"{key}_worker_character_universe_storyline_candidate_count={universe_count('storyline_candidate_count')}")
print(f"{key}_worker_character_universe_edge_count={universe_count('edge_count')}")
print(f"{key}_worker_private_review_bundle_public_surface_allowed=false")
print(f"{key}_worker_private_review_bundle_review_required=true")
print(f"{key}_worker_private_review_bundle_raw_message_rows_included=false")
print(f"{key}_worker_private_review_bundle_profile_fact_text_included=false")
print(f"{key}_worker_private_review_bundle_quote_map_entry_count={review_bundle_count('quote_map_entry_count')}")
print(f"{key}_worker_private_review_bundle_wiki_people_count={wiki_count('people')}")
print(f"{key}_worker_private_review_bundle_wiki_event_count={wiki_count('events')}")
print(f"{key}_worker_private_review_bundle_wiki_storyline_count={wiki_count('storylines')}")
print(f"{key}_worker_private_review_bundle_draft_roast_profile_candidate_count={draft_count('roast_profile_candidate_count')}")
print(f"{key}_worker_private_review_bundle_draft_storyline_timeline_count={draft_count('storyline_timeline_count')}")
print(f"{key}_worker_private_review_bundle_draft_lookback_callback_count={draft_count('lookback_callback_count')}")
PY
)"; then
    fail_evidence "daily_case_report_summary_invalid"
  fi
fi

if [[ -n "$summary_path" ]]; then
  if [[ ! -f "$summary_path" ]]; then
    fail_evidence "summary_missing"
  fi
  if ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
    fail_evidence "python_unavailable"
  fi
  if [[ "$(wc -c <"$summary_path")" -gt 65536 ]]; then
    fail_evidence "summary_invalid"
  fi
  if ! summary_date="$("$PYTHON_BIN" - "$summary_path" "$expected_worker" <<'PY'
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
worker = sys.argv[2]
try:
    data = json.loads(path.read_text(encoding="utf-8"))
except Exception:
    raise SystemExit(1)
if not isinstance(data, dict):
    raise SystemExit(1)
if data.get("schema_version") != 1:
    raise SystemExit(1)
if data.get("worker") != worker:
    raise SystemExit(1)
if data.get("requires_human_confirmation") is not True:
    raise SystemExit(1)
if data.get("external_send_executed") is not False:
    raise SystemExit(1)
if data.get("safe_for_member_chat") is not False:
    raise SystemExit(1)
date_value = data.get("date") or data.get("week_start")
if not isinstance(date_value, str) or not re.fullmatch(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}", date_value
):
    raise SystemExit(1)
print(date_value)
PY
)"; then
    fail_evidence "summary_invalid"
  fi
fi

echo "${evidence_key}_worker_run_result=success"
echo "${evidence_key}_worker_run_epoch=${run_epoch}"
if [[ -n "$daily_case_report_summary" ]]; then
  echo "$daily_case_report_summary"
fi
if [[ -n "$summary_path" ]]; then
  echo "${evidence_key}_worker_summary_present=true"
  echo "${evidence_key}_worker_summary_date=${summary_date}"
fi
