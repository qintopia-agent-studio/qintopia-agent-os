#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any


COMPARABLE_FIELDS = [
    "table_role",
    "title",
    "activity_date",
    "start_time",
    "end_time",
    "location",
    "status",
    "promotion_status",
    "owner_name",
    "initiator_name",
    "material_summary",
    "gap_summary",
]

FORBIDDEN_OUTPUT = [
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
]
FORBIDDEN_WRAPPER_KEYS = {
    "record_id",
    "source_record_id",
    "base_token",
    "app_token",
    "tenant_access_token",
    "authorization",
    "secret",
    "api_key",
}


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def normalize_text(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        return ", ".join(normalize_text(item) for item in value if normalize_text(item))
    if isinstance(value, dict):
        for key in ["text", "name", "value", "url", "link"]:
            if key in value:
                text = normalize_text(value.get(key))
                if text:
                    return text
        return ""
    return str(value).strip()


def normalize_datetime_text(value: Any) -> str:
    if isinstance(value, bool):
        return normalize_text(value)
    if isinstance(value, (int, float)):
        millis = int(value)
    elif isinstance(value, str) and value.isdigit():
        millis = int(value)
    else:
        return normalize_text(value)
    if abs(millis) < 100_000_000_000:
        return normalize_text(value)
    dt = datetime.fromtimestamp(millis / 1000, tz=timezone.utc).astimezone(
        timezone(timedelta(hours=8))
    )
    return dt.strftime("%Y-%m-%d %H:%M")


def canonical_record(record: dict[str, Any]) -> dict[str, str]:
    fields = record.get("fields") if isinstance(record.get("fields"), dict) else record
    aliases = {
        "table_role": ["table_role"],
        "title": ["title", "name", "活动名称", "活动标题", "活动信息", "活动内容", "标题"],
        "activity_date": ["activity_date", "date", "活动日期", "日期", "计划日期", "发生日期"],
        "start_time": ["start_time", "startTime", "开始时间", "活动时间", "计划时间", "活动计划时间"],
        "end_time": ["end_time", "endTime", "结束时间"],
        "location": ["location", "地点", "活动地点"],
        "status": ["status", "小满运营状态", "活动状态", "状态"],
        "promotion_status": ["promotion_status", "宣发判断", "宣发状态"],
        "owner_name": ["owner_name", "owner", "负责人", "负责同学"],
        "initiator_name": ["initiator_name", "initiator", "发起人", "组织者"],
        "material_summary": ["material_summary", "素材照片", "活动照片", "素材", "素材情况"],
        "gap_summary": ["gap_summary", "补录缺口", "缺口"],
    }
    datetime_fields = {"start_time", "end_time"}
    out: dict[str, str] = {}
    for field, names in aliases.items():
        value = ""
        for name in names:
            if name in fields:
                normalizer = normalize_datetime_text if field in datetime_fields else normalize_text
                value = normalizer(fields.get(name))
                if value:
                    break
            if name in record:
                normalizer = normalize_datetime_text if field in datetime_fields else normalize_text
                value = normalizer(record.get(name))
                if value:
                    break
        out[field] = value
    return out


def extract_records(payload: Any, *, source_name: str) -> list[dict[str, str]]:
    if isinstance(payload, list):
        raw_records = payload
    elif isinstance(payload, dict) and isinstance(payload.get("records"), list):
        raw_records = payload["records"]
    elif isinstance(payload, dict) and isinstance(payload.get("data"), dict) and isinstance(payload["data"].get("records"), list):
        raw_records = payload["data"]["records"]
    elif isinstance(payload, dict) and isinstance(payload.get("data"), dict) and isinstance(payload["data"].get("items"), list):
        raw_records = payload["data"]["items"]
    elif isinstance(payload, dict) and isinstance(payload.get("items"), list):
        raw_records = payload["items"]
    else:
        raise AssertionError(f"{source_name}: cannot find records list")

    records = []
    for item in raw_records:
        if not isinstance(item, dict):
            raise AssertionError(f"{source_name}: record is not an object")
        records.append(canonical_record(item))
    return records


def comparable_key(record: dict[str, str]) -> tuple[str, str, str, str]:
    return (
        record.get("table_role", ""),
        record.get("title", ""),
        record.get("activity_date", "") or record.get("start_time", "")[:10],
        record.get("start_time", ""),
    )


def assert_no_forbidden_output(wrapper_payload: Any) -> None:
    raw = json.dumps(wrapper_payload, ensure_ascii=False)
    for forbidden in FORBIDDEN_OUTPUT:
        if forbidden in raw:
            raise AssertionError(f"wrapper output leaks forbidden text: {forbidden}")
    for key_path in forbidden_key_paths(wrapper_payload):
        raise AssertionError(f"wrapper output leaks forbidden internal key: {key_path}")


def forbidden_key_paths(value: Any, prefix: str = "$") -> list[str]:
    paths: list[str] = []
    if isinstance(value, dict):
        for key, item in value.items():
            key_text = str(key)
            item_path = f"{prefix}.{key_text}"
            if key_text.lower() in FORBIDDEN_WRAPPER_KEYS:
                paths.append(item_path)
            paths.extend(forbidden_key_paths(item, item_path))
    elif isinstance(value, list):
        for index, item in enumerate(value):
            paths.extend(forbidden_key_paths(item, f"{prefix}[{index}]"))
    return paths


def duplicate_keys(records: list[dict[str, str]]) -> list[tuple[str, str, str, str]]:
    seen: set[tuple[str, str, str, str]] = set()
    duplicates: set[tuple[str, str, str, str]] = set()
    for record in records:
        key = comparable_key(record)
        if key in seen:
            duplicates.add(key)
        seen.add(key)
    return sorted(duplicates)


def compare_records(legacy: list[dict[str, str]], wrapper: list[dict[str, str]]) -> list[str]:
    mismatches: list[str] = []
    legacy_duplicates = duplicate_keys(legacy)
    wrapper_duplicates = duplicate_keys(wrapper)
    if legacy_duplicates:
        mismatches.append(f"duplicate legacy comparable keys: {legacy_duplicates}")
    if wrapper_duplicates:
        mismatches.append(f"duplicate wrapper comparable keys: {wrapper_duplicates}")
    if legacy_duplicates or wrapper_duplicates:
        return mismatches

    legacy_by_key = {comparable_key(item): item for item in legacy}
    wrapper_by_key = {comparable_key(item): item for item in wrapper}
    if set(legacy_by_key) != set(wrapper_by_key):
        missing = sorted(set(legacy_by_key) - set(wrapper_by_key))
        extra = sorted(set(wrapper_by_key) - set(legacy_by_key))
        if missing:
            mismatches.append(f"missing wrapper records for keys: {missing}")
        if extra:
            mismatches.append(f"extra wrapper records for keys: {extra}")
        return mismatches

    for key, legacy_record in legacy_by_key.items():
        wrapper_record = wrapper_by_key[key]
        for field in COMPARABLE_FIELDS:
            if legacy_record.get(field, "") != wrapper_record.get(field, ""):
                mismatches.append(
                    f"{key}: field {field} differs: legacy={legacy_record.get(field, '')!r} wrapper={wrapper_record.get(field, '')!r}"
                )
    return mismatches


def run_check(legacy_path: Path, wrapper_path: Path) -> dict[str, Any]:
    legacy_payload = load_json(legacy_path)
    wrapper_payload = load_json(wrapper_path)
    assert_no_forbidden_output(wrapper_payload)
    legacy_records = extract_records(legacy_payload, source_name=str(legacy_path))
    wrapper_records = extract_records(wrapper_payload, source_name=str(wrapper_path))
    mismatches = compare_records(legacy_records, wrapper_records)
    return {
        "success": not mismatches,
        "legacy_record_count": len(legacy_records),
        "wrapper_record_count": len(wrapper_records),
        "compared_fields": COMPARABLE_FIELDS,
        "mismatches": mismatches,
    }


def selftest() -> None:
    fixture = (
        Path(__file__).resolve().parents[3]
        / "runtime/sidecar/fixtures/xiaoman_activity_records.json"
    )
    payload = load_json(fixture)
    legacy = extract_records(payload, source_name="fixture legacy")
    wrapper = extract_records(payload, source_name="fixture wrapper")
    assert compare_records(legacy, wrapper) == []
    leaked = {"records": [{"title": "ok"}], "debug": "execute_code"}
    try:
        assert_no_forbidden_output(leaked)
    except AssertionError:
        pass
    else:
        raise AssertionError("selftest expected forbidden output detection")
    leaked_key = {"records": [{"record_id": "rec_internal"}]}
    try:
        assert_no_forbidden_output(leaked_key)
    except AssertionError:
        pass
    else:
        raise AssertionError("selftest expected forbidden key detection")
    duplicated = legacy + [legacy[0]]
    duplicate_mismatches = compare_records(duplicated, wrapper)
    assert len(duplicate_mismatches) == 1
    assert duplicate_mismatches[0].startswith("duplicate legacy comparable keys:")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compare legacy Xiaoman Base read output with xiaoman-activity wrapper output."
    )
    parser.add_argument("--legacy-json", type=Path, help="JSON captured from the current legacy Base read path.")
    parser.add_argument("--wrapper-json", type=Path, help="JSON captured from xiaoman-activity shadow read output.")
    parser.add_argument("--selftest", action="store_true", help="Run local fixture self-test without production data.")
    args = parser.parse_args()

    if args.selftest:
        selftest()
        print("xiaoman activity parity selftest passed")
        return 0

    if not args.legacy_json or not args.wrapper_json:
        parser.error("--legacy-json and --wrapper-json are required unless --selftest is used")

    try:
        report = run_check(args.legacy_json, args.wrapper_json)
    except Exception as exc:
        print(json.dumps({"success": False, "error": str(exc)}, ensure_ascii=False, indent=2), file=sys.stderr)
        return 1

    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["success"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
