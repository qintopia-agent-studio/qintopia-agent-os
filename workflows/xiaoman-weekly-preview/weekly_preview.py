#!/usr/bin/env python3
"""Xiaoman weekly activity preview cron script.

Reads the next 7 days of Xiaoman activity records through the qintopia-tools wrapper,
filters by completeness, and prints a human-review draft. It never sends. A human must
confirm before any Erhua handoff or group send.

This script is the deterministic replacement for the old natural-language Monday cron
task. It calls `qintopia_xiaoman_activity_announcement_prepare` with
`mode=weekly_preview`.
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
_VARIANT = _THIS.parents[2] / "skills" / "qintopia-tools" / "variants" / "xiaoman" / "__init__.py"
if not _VARIANT.exists():
    print(f"ERROR: cannot locate reviewed xiaoman wrapper at {_VARIANT}", file=sys.stderr)
    raise SystemExit(2)

sys.path.insert(0, str(_VARIANT.parent))
_spec = importlib.util.spec_from_file_location("qintopia_xiaoman_wrapper", _VARIANT)
_module = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_module)


def _monday_of(date_str: str | None) -> str:
    if date_str:
        day = datetime.strptime(date_str, "%Y-%m-%d")
    else:
        day = datetime.now()
    day -= timedelta(days=day.weekday())
    return day.strftime("%Y-%m-%d")


def main() -> int:
    parser = argparse.ArgumentParser(description="Xiaoman weekly activity preview")
    parser.add_argument(
        "--date",
        help="Monday that starts the preview week (YYYY-MM-DD). Defaults to today's Monday.",
    )
    parser.add_argument("--operator-name", default="刘珊")
    parser.add_argument("--audience", default="社区群成员")
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print full JSON instead of just the operator review message.",
    )
    args = parser.parse_args()

    # The wrapper requires the Xiaoman profile and read-through to be enabled.
    os.environ.setdefault("QINTOPIA_PROFILE_ID", "xiaoman")
    for env_key in (
        "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
        "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
        "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
    ):
        if not os.environ.get(env_key):
            print(f"ERROR: {env_key} must be set in the runtime environment.", file=sys.stderr)
            return 2

    monday = _monday_of(args.date)
    result = json.loads(
        _module.handle_qintopia_xiaoman_activity_announcement_prepare(
            {
                "date": monday,
                "mode": "weekly_preview",
                "operator_name": args.operator_name,
                "community_audience": args.audience,
            }
        )
    )

    if not result.get("success"):
        print("ERROR: weekly preview failed:", result.get("error"), file=sys.stderr)
        print(json.dumps(result, ensure_ascii=False), file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(result["operator_review_message"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
