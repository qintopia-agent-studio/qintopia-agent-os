#!/usr/bin/env python3
"""Xiaoman weekly minimum-loop draft runner.

This is the deterministic release-managed path for the Saturday recruitment draft and
Sunday plan-confirmation draft. It prepares operations-review text only and never sends.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
from datetime import datetime, timedelta
from pathlib import Path

_THIS = Path(__file__).resolve()
_VARIANT = (
    _THIS.parents[2]
    / "skills"
    / "qintopia-tools"
    / "variants"
    / "xiaoman"
    / "__init__.py"
)
if not _VARIANT.exists():
    print(f"ERROR: cannot locate reviewed xiaoman wrapper at {_VARIANT}", file=sys.stderr)
    raise SystemExit(2)

sys.path.insert(0, str(_VARIANT.parent))
_spec = importlib.util.spec_from_file_location("qintopia_xiaoman_wrapper", _VARIANT)
_module = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_module)


def _next_iso_week_label(date_str: str | None) -> str:
    if date_str:
        return date_str

    today = datetime.now()
    days_until_next_monday = 7 - today.weekday()
    monday = today + timedelta(days=days_until_next_monday)
    iso_year, iso_week, _ = monday.isocalendar()
    return f"{iso_year}-W{iso_week:02d}"


def main() -> int:
    parser = argparse.ArgumentParser(description="Xiaoman weekly minimum-loop draft")
    parser.add_argument(
        "--mode",
        required=True,
        choices=("weekly_recruitment_form", "weekly_plan_confirmation"),
        help="Draft mode to prepare.",
    )
    parser.add_argument(
        "--date",
        help="Target week label (YYYY-Www) or date. Defaults to the next ISO week.",
    )
    parser.add_argument("--operator-name", default="刘珊")
    parser.add_argument("--audience")
    parser.add_argument("--form-label", default="活动招募表单")
    parser.add_argument("--confirmation-owner-name", default="张百忍")
    parser.add_argument("--plan-sheet-label", default="下周活动计划表")
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print full JSON instead of just the operator review message.",
    )
    args = parser.parse_args()

    os.environ.setdefault("QINTOPIA_PROFILE_ID", "xiaoman")
    if not os.environ.get("QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE"):
        print(
            "ERROR: QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE must be set.",
            file=sys.stderr,
        )
        return 2

    payload = {
        "date": _next_iso_week_label(args.date),
        "mode": args.mode,
        "operator_name": args.operator_name,
        "community_audience": args.audience
        or ("居民群" if args.mode == "weekly_recruitment_form" else "营造司群"),
        "actor_agent": "xiaoman",
    }
    if args.mode == "weekly_recruitment_form":
        payload["form_label"] = args.form_label
    else:
        payload["confirmation_owner_name"] = args.confirmation_owner_name
        payload["plan_sheet_label"] = args.plan_sheet_label

    result = json.loads(
        _module.handle_qintopia_xiaoman_activity_announcement_prepare(payload)
    )

    if not result.get("success"):
        print(
            f"ERROR: Xiaoman weekly loop {args.mode} failed: {result.get('error')}",
            file=sys.stderr,
        )
        print(json.dumps(result, ensure_ascii=False), file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(result["operator_review_message"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
