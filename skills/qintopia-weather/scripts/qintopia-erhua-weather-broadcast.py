#!/usr/bin/env python3
"""Render Erhua's canonical morning weather broadcast to stdout.

Hermes owns scheduling and QiWe delivery. This entrypoint only calls the existing
weather capability and selects its reviewed ``morning_broadcast`` field.
"""

from __future__ import annotations

import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any, Callable


WEATHER_ARGUMENTS = {"intent": "general", "hours": 24}
EXPECTED_PERIOD_IDS = ["midday", "afternoon", "evening"]
CURRENT_FIRST_PATTERN = re.compile(r"^(?:现在|此时)")


class BroadcastContractError(RuntimeError):
    """Raised when the weather payload is unsafe for scheduled delivery."""


def _load_weather_handler() -> Callable[[dict[str, Any]], str]:
    skill_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location(
        "qintopia_weather_broadcast_runtime", skill_path
    )
    if spec is None or spec.loader is None:
        raise BroadcastContractError("weather capability loader is unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    handler = getattr(module, "handle_qintopia_weather_lookup", None)
    if not callable(handler):
        raise BroadcastContractError("weather capability handler is unavailable")
    return handler


def extract_morning_broadcast(raw_payload: str) -> str:
    if not isinstance(raw_payload, str):
        raise BroadcastContractError("weather capability returned a non-text payload")
    try:
        payload = json.loads(raw_payload)
    except json.JSONDecodeError as exc:
        raise BroadcastContractError("weather capability returned malformed JSON") from exc

    if not isinstance(payload, dict) or payload.get("success") is not True:
        raise BroadcastContractError("weather capability did not report success")

    broadcast = payload.get("morning_broadcast")
    if not isinstance(broadcast, str) or not broadcast.strip():
        raise BroadcastContractError("morning_broadcast is missing")
    if broadcast != broadcast.strip():
        raise BroadcastContractError("morning_broadcast has surrounding whitespace")

    lines = [line.strip() for line in broadcast.splitlines() if line.strip()]
    if len(lines) > 8:
        raise BroadcastContractError("morning_broadcast exceeds the eight-line limit")
    if CURRENT_FIRST_PATTERN.match(lines[0]):
        raise BroadcastContractError("morning_broadcast regressed to current-first copy")
    if not any(line.startswith("分时：") for line in lines):
        raise BroadcastContractError("morning_broadcast is missing the day-period line")

    daily_forecast = payload.get("daily_forecast")
    if not isinstance(daily_forecast, dict):
        raise BroadcastContractError("daily_forecast is missing")
    periods = daily_forecast.get("periods")
    if not isinstance(periods, list):
        raise BroadcastContractError("daily_forecast periods are missing")
    period_ids = [
        period.get("id") if isinstance(period, dict) else None for period in periods
    ]
    if period_ids != EXPECTED_PERIOD_IDS:
        raise BroadcastContractError("daily_forecast periods do not match the contract")

    return broadcast


def main() -> int:
    try:
        handler = _load_weather_handler()
        raw_payload = handler(dict(WEATHER_ARGUMENTS))
        broadcast = extract_morning_broadcast(raw_payload)
    except BroadcastContractError as exc:
        sys.stderr.write(f"weather broadcast contract error: {exc}\n")
        return 1
    except Exception:
        sys.stderr.write("weather broadcast generation failed\n")
        return 1

    sys.stdout.write(broadcast)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
