#!/usr/bin/env python3
"""Erhua morning-brief weather via the canonical Qintopia weather capability.

Delegates to ``skills.qintopia_weather`` (``handle_qintopia_weather_lookup``),
the project's single source of truth for weather: QWeather primary with an
Open-Meteo degraded fallback, fixed to the Qintopia location. All failures
degrade to None so the morning brief never blocks on weather.

A fixture JSON shaped like the ``qintopia_weather_lookup`` payload bypasses
the network for tests and demos.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Optional


@dataclass(frozen=True)
class WeatherInfo:
    condition: str
    current_temp: Optional[float]
    temp_max: Optional[float]
    temp_min: Optional[float]
    # Human-readable one-liner used by the brief and the renderer.
    # Deliberately has NO "今日天气：" prefix: the brief block already carries
    # that title, and the card weather chip is self-describing.
    summary: str


def _skill_init_path() -> Path:
    # workflows/erhua-morning-brief/weather.py -> repo/skills/qintopia-weather/__init__.py
    return Path(__file__).resolve().parents[2] / "skills" / "qintopia-weather" / "__init__.py"


def _load_weather_handler() -> Optional[Callable[[dict[str, Any]], str]]:
    """Lazily import the project weather capability so tests using a fixture
    never pay the import or network cost."""
    skill_init = _skill_init_path()
    if not skill_init.exists():
        return None
    spec = importlib.util.spec_from_file_location("qintopia_weather_lookup_runtime", skill_init)
    if spec is None or spec.loader is None:
        return None
    module = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(module)
    except Exception:
        return None
    handler = getattr(module, "handle_qintopia_weather_lookup", None)
    return handler if callable(handler) else None


def _to_float(value: Any) -> Optional[float]:
    try:
        return round(float(value), 1)
    except (TypeError, ValueError):
        return None


def _clean(value: Any) -> str:
    return str(value or "").strip() if value is not None else ""


def _first_period_condition(periods: Any) -> str:
    if not isinstance(periods, list):
        return ""
    for period in periods:
        if isinstance(period, dict):
            text = _clean(period.get("condition"))
            if text:
                return text
    return ""


def _fmt_temp(value: Optional[float]) -> Optional[str]:
    if value is None:
        return None
    return f"{int(value)}°" if float(value).is_integer() else f"{value:g}°"


def _build_summary(
    *,
    condition: str,
    current_temp: Optional[float],
    temp_max: Optional[float],
    temp_min: Optional[float],
) -> str:
    parts = [condition]
    temp_bits: list[str] = []
    if current_temp is not None:
        temp_bits.append(f"当前 {_fmt_temp(current_temp)}")
    if temp_max is not None and temp_min is not None:
        temp_bits.append(f"最高 {_fmt_temp(temp_max)} / 最低 {_fmt_temp(temp_min)}")
    elif temp_max is not None:
        temp_bits.append(f"最高 {_fmt_temp(temp_max)}")
    elif temp_min is not None:
        temp_bits.append(f"最低 {_fmt_temp(temp_min)}")
    if temp_bits:
        parts.append("，".join(temp_bits))
    return "。".join(parts) + "。"


def _from_capability_payload(payload: dict[str, Any]) -> Optional[WeatherInfo]:
    if not isinstance(payload, dict) or payload.get("success") is not True:
        return None
    daily = payload.get("daily_forecast") or {}
    if not isinstance(daily, dict) or "periods" not in daily:
        return None

    current = payload.get("current") or {}
    condition = _clean(current.get("text")) or _first_period_condition(daily.get("periods")) or "天气"
    current_temp = _to_float(current.get("temp_c"))

    temps_max: list[float] = []
    temps_min: list[float] = []
    for period in daily.get("periods") or []:
        if not isinstance(period, dict):
            continue
        tmax = _to_float(period.get("temp_max_c"))
        tmin = _to_float(period.get("temp_min_c"))
        if tmax is not None:
            temps_max.append(tmax)
        if tmin is not None:
            temps_min.append(tmin)
    temp_max = max(temps_max) if temps_max else None
    temp_min = min(temps_min) if temps_min else None

    summary = _build_summary(
        condition=condition,
        current_temp=current_temp,
        temp_max=temp_max,
        temp_min=temp_min,
    )
    return WeatherInfo(
        condition=condition,
        current_temp=current_temp,
        temp_max=temp_max,
        temp_min=temp_min,
        summary=summary,
    )


def fetch_weather(*, fixture_path: Optional[str] = None) -> Optional[WeatherInfo]:
    """Return today's weather, or None when unavailable.

    With ``fixture_path`` the network is bypassed by loading a
    ``qintopia_weather_lookup`` payload JSON (tests/demos). Otherwise the
    canonical weather capability is invoked; any failure degrades to None.
    """
    if fixture_path:
        try:
            data = json.loads(Path(fixture_path).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None
        return _from_capability_payload(data) if isinstance(data, dict) else None

    handler = _load_weather_handler()
    if handler is None:
        return None
    try:
        raw = handler({"intent": "general", "hours": 24})
        payload = json.loads(raw) if isinstance(raw, str) else None
    except Exception:
        return None
    return _from_capability_payload(payload) if isinstance(payload, dict) else None


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Fetch Qintopia weather for the morning brief")
    parser.add_argument(
        "--fixture",
        help="qintopia_weather_lookup payload JSON fixture bypassing the network",
    )
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    info = fetch_weather(fixture_path=args.fixture)
    if info is None:
        print("WEATHER_UNAVAILABLE", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(info.__dict__, ensure_ascii=False, indent=2))
    else:
        print(info.summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
