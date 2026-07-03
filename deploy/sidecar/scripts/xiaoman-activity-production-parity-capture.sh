#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
cd "$MONOREPO_ROOT"

if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_PARITY_ENABLE:-}" != "1" ]]; then
  echo "xiaoman activity production parity capture skipped: set QINTOPIA_XIAOMAN_ACTIVITY_PARITY_ENABLE=1 to run read-only Feishu/Base parity" >&2
  exit 0
fi

required_env=(
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN
  QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID
  QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PROFILE_ENV_PATH
)

for name in "${required_env[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    echo "missing required parity env: $name" >&2
    exit 1
  fi
done

if [[ ",${QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS}," != *",${QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN},"* ]]; then
  echo "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN is not in the explicit allowlist" >&2
  exit 1
fi

PYTHON_BIN="${PYTHON_BIN:-python3}"
PARITY_DATE="${QINTOPIA_XIAOMAN_ACTIVITY_PARITY_DATE:-$(date +%F)}"
PARITY_DIR="${QINTOPIA_XIAOMAN_ACTIVITY_PARITY_DIR:-/tmp/qintopia-xiaoman-activity-parity}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

umask 077
mkdir -p "$PARITY_DIR"

LEGACY_JSON="$PARITY_DIR/${STAMP}-${PARITY_DATE}-legacy-base-source.json"
WRAPPER_JSON="$PARITY_DIR/${STAMP}-${PARITY_DATE}-wrapper-shadow.json"
REPORT_JSON="$PARITY_DIR/${STAMP}-${PARITY_DATE}-parity-report.json"
export PARITY_DATE LEGACY_JSON WRAPPER_JSON REPORT_JSON

"$PYTHON_BIN" - <<'PY'
from __future__ import annotations

import json
import os
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone, timedelta
from pathlib import Path
from typing import Any


AUTH_URL = "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal"
BASE_URL = "https://open.feishu.cn/open-apis/bitable/v1/apps"


def read_profile_env(path: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        values[key.strip()] = value.strip().strip('"').strip("'")
    app_id = values.get("FEISHU_APP_ID") or values.get("LARK_APP_ID") or values.get("APP_ID")
    app_secret = (
        values.get("FEISHU_APP_SECRET")
        or values.get("LARK_APP_SECRET")
        or values.get("APP_SECRET")
    )
    if not app_id:
        raise SystemExit("missing FEISHU_APP_ID in profile env")
    if not app_secret:
        raise SystemExit("missing FEISHU_APP_SECRET in profile env")
    return {"app_id": app_id, "app_secret": app_secret}


def request_json(method: str, url: str, *, token: str | None = None, body: Any | None = None) -> Any:
    data = None if body is None else json.dumps(body).encode("utf-8")
    headers = {"Accept": "application/json", "Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        raise SystemExit(f"Feishu HTTP error {exc.code}") from exc


def tenant_token(profile_env_path: str) -> str:
    profile = read_profile_env(profile_env_path)
    payload = request_json("POST", AUTH_URL, body=profile)
    if payload.get("code") != 0:
        raise SystemExit(f"Feishu token response error code={payload.get('code')}")
    token = payload.get("tenant_access_token")
    if not token:
        raise SystemExit("Feishu token response missing tenant_access_token")
    return str(token)


def list_records(base_token: str, table_id: str, token: str) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    page_token = ""
    while True:
        query = {"page_size": "200"}
        if page_token:
            query["page_token"] = page_token
        url = f"{BASE_URL}/{base_token}/tables/{table_id}/records?{urllib.parse.urlencode(query)}"
        payload = request_json("GET", url, token=token)
        if payload.get("code") != 0:
            raise SystemExit(f"Feishu Base response error code={payload.get('code')}")
        data = payload.get("data") or {}
        items = data.get("items") or []
        if not isinstance(items, list):
            raise SystemExit("Feishu Base response data.items is not a list")
        out.extend(item for item in items if isinstance(item, dict))
        if data.get("has_more") is not True:
            break
        page_token = str(data.get("page_token") or "")
        if not page_token:
            break
    return out


def cell_as_text(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, list):
        return ", ".join(item for item in (cell_as_text(item) for item in value) if item)
    if isinstance(value, dict):
        for key in ["text", "name", "value", "url", "link"]:
            text = cell_as_text(value.get(key))
            if text:
                return text
    return ""


def field_cell_as_text(name: str, value: Any) -> str:
    if name in {
        "开始时间",
        "活动时间",
        "计划时间",
        "活动计划时间",
        "结束时间",
        "更新时间",
        "start_time",
        "startTime",
        "end_time",
        "endTime",
        "updated_at",
    }:
        timestamp = timestamp_as_shanghai_datetime(value)
        if timestamp:
            return timestamp
    return cell_as_text(value)


def timestamp_as_shanghai_datetime(value: Any) -> str:
    if isinstance(value, bool):
        return ""
    if isinstance(value, (int, float)):
        millis = int(value)
    elif isinstance(value, str) and value.isdigit():
        millis = int(value)
    else:
        return ""
    if abs(millis) < 100_000_000_000:
        return ""
    dt = datetime.fromtimestamp(millis / 1000, tz=timezone.utc).astimezone(
        timezone(timedelta(hours=8))
    )
    return dt.strftime("%Y-%m-%d %H:%M")


def field_text(fields: dict[str, Any], names: list[str]) -> str:
    for name in names:
        text = field_cell_as_text(name, fields.get(name))
        if text:
            return text
    return ""


def matches_date(record: dict[str, Any], date: str) -> bool:
    fields = record.get("fields") if isinstance(record.get("fields"), dict) else {}
    activity_date = field_text(
        fields,
        ["活动日期", "日期", "计划日期", "发生日期", "date", "activity_date"],
    )
    start_time = field_text(
        fields,
        ["开始时间", "活动时间", "计划时间", "活动计划时间", "start_time", "startTime"],
    )
    return activity_date == date or start_time.startswith(date)


def canonical_legacy_record(record: dict[str, Any], table_role: str) -> dict[str, Any]:
    fields = record.get("fields") if isinstance(record.get("fields"), dict) else {}
    return {
        "table_role": table_role,
        "title": field_text(
            fields,
            ["活动名称", "活动标题", "活动信息", "活动内容", "标题", "name", "title"],
        )
        or "未命名活动",
        "activity_date": field_text(
            fields,
            ["活动日期", "日期", "计划日期", "发生日期", "date", "activity_date"],
        ),
        "start_time": field_text(
            fields,
            ["开始时间", "活动时间", "计划时间", "活动计划时间", "start_time", "startTime"],
        ),
        "end_time": field_text(fields, ["结束时间", "end_time", "endTime"]),
        "location": field_text(fields, ["地点", "活动地点", "location"]),
        "status": field_text(fields, ["小满运营状态", "活动状态", "状态", "status"]),
        "promotion_status": field_text(fields, ["宣发判断", "宣发状态", "promotion_status"]),
        "owner_name": field_text(fields, ["负责人", "负责同学", "owner", "owner_name"]),
        "initiator_name": field_text(fields, ["发起人", "组织者", "initiator"]),
        "material_summary": field_text(
            fields,
            ["素材照片", "活动照片", "素材", "素材情况", "material_summary"],
        ),
        "gap_summary": field_text(fields, ["补录缺口", "缺口", "gap_summary"]),
    }


profile_env_path = os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PROFILE_ENV_PATH"]
base_token = os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN"]
date = os.environ["PARITY_DATE"]
output_path = Path(os.environ["LEGACY_JSON"])
tables = [
    ("activity_plan", os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID"]),
    ("activity_occurrence", os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID"]),
]

token = tenant_token(profile_env_path)
records: list[dict[str, Any]] = []
for table_role, table_id in tables:
    for record in list_records(base_token, table_id, token):
        if not matches_date(record, date):
            continue
        records.append(canonical_legacy_record(record, table_role))

payload = {
    "source": "feishu_base_source_legacy_shape",
    "date": date,
    "records": records,
    "record_count": len(records),
}
raw = json.dumps(payload, ensure_ascii=False)
for forbidden in [
    base_token,
    os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID"],
    os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID"],
    "record_id",
    "fields",
]:
    if forbidden and forbidden in raw:
        raise SystemExit("legacy capture leaked internal Base identifier")
output_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
PY

run_worker_json() {
  local table_role="$1"
  local payload
  payload="$("$PYTHON_BIN" - <<PY
import json
print(json.dumps({
    "date": "${PARITY_DATE}",
    "table_role": "${table_role}",
    "actor_agent": "xiaoman",
    "operation": "list-by-date",
    "dry_run": False,
}, ensure_ascii=False))
PY
)"
  if [[ -n "${QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN:-}" ]]; then
    "$QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN" xiaoman-activity list-by-date \
      --payload-json "$payload" \
      --use-feishu-base \
      --apply
  else
    cargo run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" -- xiaoman-activity list-by-date \
      --payload-json "$payload" \
      --use-feishu-base \
      --apply
  fi
}

PLAN_WRAPPER_OUTPUT="$(run_worker_json activity_plan)"
OCCURRENCE_WRAPPER_OUTPUT="$(run_worker_json activity_occurrence)"

PLAN_WRAPPER_OUTPUT="$PLAN_WRAPPER_OUTPUT" \
OCCURRENCE_WRAPPER_OUTPUT="$OCCURRENCE_WRAPPER_OUTPUT" \
"$PYTHON_BIN" - <<'PY'
from __future__ import annotations

import json
import os
from pathlib import Path

date = os.environ["PARITY_DATE"]
payloads = [
    json.loads(os.environ["PLAN_WRAPPER_OUTPUT"]),
    json.loads(os.environ["OCCURRENCE_WRAPPER_OUTPUT"]),
]
records = []
summaries = []
role_reports = []
for payload in payloads:
    if payload.get("success") is not True:
        raise SystemExit("wrapper shadow read returned success=false")
    if payload.get("source") != "feishu_base_read_only":
        raise SystemExit("wrapper shadow read did not use feishu_base_read_only source")
    if payload.get("safe_for_chat") is not False:
        raise SystemExit("wrapper report must remain safe_for_chat=false")
    if payload.get("action_status") not in {"read_ok", "record_not_found"}:
        raise SystemExit(f"unexpected wrapper action_status={payload.get('action_status')}")
    records.extend(payload.get("records") or [])
    summaries.extend(payload.get("summaries") or [])
    role_reports.append(
        {
            "operation": payload.get("operation"),
            "source": payload.get("source"),
            "action_status": payload.get("action_status"),
            "record_count": payload.get("record_count"),
        }
    )

combined = {
    "source": "xiaoman_activity_wrapper_shadow",
    "date": date,
    "records": records,
    "record_count": len(records),
    "summaries": summaries,
    "role_reports": role_reports,
}
raw = json.dumps(combined, ensure_ascii=False)
for forbidden in [
    "Dangerous command requires approval",
    "/approve",
    "Working",
    "execute_code",
    "terminal",
    "skill_view",
    "clarify",
    "lark-base",
    "traceback",
    "Traceback",
    os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN"],
    os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID"],
    os.environ["QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID"],
]:
    if forbidden and forbidden in raw:
        raise SystemExit(f"wrapper capture leaked forbidden text: {forbidden}")
Path(os.environ["WRAPPER_JSON"]).write_text(
    json.dumps(combined, ensure_ascii=False, indent=2),
    encoding="utf-8",
)
PY

if ! "$PYTHON_BIN" "$SCRIPT_DIR/xiaoman-activity-parity-check.py" \
  --legacy-json "$LEGACY_JSON" \
  --wrapper-json "$WRAPPER_JSON" >"$REPORT_JSON"; then
  echo "xiaoman activity production parity failed; report=$REPORT_JSON legacy=$LEGACY_JSON wrapper=$WRAPPER_JSON" >&2
  exit 1
fi

chmod 600 "$LEGACY_JSON" "$WRAPPER_JSON" "$REPORT_JSON"

echo "xiaoman activity production parity passed for ${PARITY_DATE}"
echo "parity_report=${REPORT_JSON}"
echo "legacy_capture=${LEGACY_JSON}"
echo "wrapper_capture=${WRAPPER_JSON}"
