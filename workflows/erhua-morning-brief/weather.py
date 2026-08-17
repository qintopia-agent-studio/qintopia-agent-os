#!/usr/bin/env python3
"""Open-Meteo weather fetch for the Erhua morning brief.

Free, no API key required (https://api.open-meteo.com). Returns a sanitized
public summary for the resident-facing brief. All failures degrade to None so
the morning brief never blocks on weather.
"""
from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from typing import Any, Optional

try:
    import urllib.request
except ImportError:  # pragma: no cover - stdlib always present
    urllib = None  # type: ignore

DEFAULT_LATITUDE = 22.78  # Pu'er, Yunnan (秦托邦 default; override per deployment)
DEFAULT_LONGITUDE = 101.04
DEFAULT_TIMEZONE = "Asia/Shanghai"
DEFAULT_WEATHER_TIMEOUT_SECONDS = 8
OPEN_METEO_URL = "https://api.open-meteo.com/v1/forecast"

# WMO weather interpretation codes -> resident-friendly Chinese.
_WMO_CODES: dict[int, str] = {
    0: "晴",
    1: "晴间多云",
    2: "多云",
    3: "阴",
    45: "雾",
    48: "雾凇",
    51: "小毛毛雨",
    53: "毛毛雨",
    55: "大毛毛雨",
    56: "冻毛毛雨",
    57: "强冻毛毛雨",
    61: "小雨",
    63: "中雨",
    65: "大雨",
    66: "冻雨",
    67: "强冻雨",
    71: "小雪",
    73: "中雪",
    75: "大雪",
    77: "雪粒",
    80: "阵雨",
    81: "中阵雨",
    82: "强阵雨",
    85: "阵雪",
    86: "强阵雪",
    95: "雷阵雨",
    96: "雷阵雨伴小冰雹",
    99: "雷阵雨伴大冰雹",
}


@dataclass(frozen=True)
class WeatherInfo:
    condition: str
    current_temp: Optional[float]
    temp_max: Optional[float]
    temp_min: Optional[float]
    # Human-readable one-liner used by the brief and the renderer.
    summary: str

    def as_block_body(self) -> str:
        return self.summary


def _condition_for(code: Any) -> str:
    try:
        code_int = int(code)
    except (TypeError, ValueError):
        return "天气未知"
    return _WMO_CODES.get(code_int, "天气未知")


def _fmt_temp(value: Any) -> Optional[float]:
    try:
        return round(float(value), 1)
    except (TypeError, ValueError):
        return None


def _build_summary(
    *,
    condition: str,
    current_temp: Optional[float],
    temp_max: Optional[float],
    temp_min: Optional[float],
) -> str:
    parts = [f"今日天气：{condition}"]
    temp_bits: list[str] = []
    if current_temp is not None:
        temp_bits.append(f"当前 {current_temp:g}°")
    if temp_max is not None and temp_min is not None:
        temp_bits.append(f"最高 {temp_max:g}° / 最低 {temp_min:g}°")
    elif temp_max is not None:
        temp_bits.append(f"最高 {temp_max:g}°")
    elif temp_min is not None:
        temp_bits.append(f"最低 {temp_min:g}°")
    if temp_bits:
        parts.append("，".join(temp_bits))
    return "。".join(parts) + "。"


def _from_payload(data: dict[str, Any]) -> WeatherInfo:
    current = data.get("current") or {}
    daily = data.get("daily") or {}
    code = current.get("weather_code", (daily.get("weather_code") or [None])[0] if daily.get("weather_code") else None)
    condition = _condition_for(code)
    current_temp = _fmt_temp(current.get("temperature_2m"))
    temp_max = None
    temp_min = None
    daily_max = daily.get("temperature_2m_max") or []
    daily_min = daily.get("temperature_2m_min") or []
    if daily_max:
        temp_max = _fmt_temp(daily_max[0])
    if daily_min:
        temp_min = _fmt_temp(daily_min[0])
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


def fetch_weather(
    *,
    latitude: float = DEFAULT_LATITUDE,
    longitude: float = DEFAULT_LONGITUDE,
    timezone: str = DEFAULT_TIMEZONE,
    timeout_seconds: int = DEFAULT_WEATHER_TIMEOUT_SECONDS,
    fixture_path: Optional[str] = None,
) -> Optional[WeatherInfo]:
    """Return today's weather, or None when unavailable.

    A fixture JSON ({"current": {...}, "daily": {...}}) bypasses the network for
    tests and demos, exactly like the news fixture.
    """
    if fixture_path:
        try:
            data = json.loads(__import__("pathlib").Path(fixture_path).read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None
        if not isinstance(data, dict):
            return None
        return _from_payload(data)

    if urllib is None:  # pragma: no cover
        return None
    params = (
        f"latitude={latitude}&longitude={longitude}"
        f"&current=temperature_2m,weather_code"
        f"&daily=weather_code,temperature_2m_max,temperature_2m_min"
        f"&timezone={urllib.parse.quote(timezone)}&forecast_days=1"
    )
    url = f"{OPEN_METEO_URL}?{params}"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "qintopia-erhua-morning-brief/1.0"})
        with urllib.request.urlopen(req, timeout=max(1, timeout_seconds)) as resp:
            data = json.loads(resp.read(1_048_576).decode("utf-8", errors="replace"))
    except Exception:
        return None
    if not isinstance(data, dict):
        return None
    return _from_payload(data)


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Fetch Open-Meteo weather for the morning brief")
    parser.add_argument("--latitude", type=float, default=DEFAULT_LATITUDE)
    parser.add_argument("--longitude", type=float, default=DEFAULT_LONGITUDE)
    parser.add_argument("--timezone", default=DEFAULT_TIMEZONE)
    parser.add_argument("--timeout-seconds", type=int, default=DEFAULT_WEATHER_TIMEOUT_SECONDS)
    parser.add_argument("--fixture", help="JSON fixture bypassing the network")
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    info = fetch_weather(
        latitude=args.latitude,
        longitude=args.longitude,
        timezone=args.timezone,
        timeout_seconds=args.timeout_seconds,
        fixture_path=args.fixture,
    )
    if info is None:
        print("WEATHER_UNAVAILABLE", file=__import__("sys").stderr)
        return 1
    if args.json:
        print(json.dumps(info.__dict__, ensure_ascii=False, indent=2))
    else:
        print(info.summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
