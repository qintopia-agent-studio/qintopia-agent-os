"""Fixed-location Qintopia weather capability.

This package owns the implementation behind ``qintopia_weather_lookup``. It is
intentionally narrow: Qintopia fixed coordinates only, QWeather allowlisted calls, and
Open-Meteo as a degraded fallback.
"""

from __future__ import annotations

import gzip
import importlib
import json
import logging
import os
import re
import sys
from concurrent.futures import ThreadPoolExecutor, TimeoutError
from datetime import datetime, timezone
from threading import Lock
from typing import Any
from urllib import error as urlerror
from urllib import request as urlrequest
from urllib.parse import urlencode
from zoneinfo import ZoneInfo


DEFAULT_QINTOPIA_WEATHER_LOCATION = "108.5666545,34.0261288"
DEFAULT_QINTOPIA_WEATHER_LOCATION_NAME = "秦托邦·栗峪口"
DEFAULT_QINTOPIA_WEATHER_QWEATHER_CITY = "鄠邑区"
DEFAULT_QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS = 12
DEFAULT_OPEN_METEO_TIMEOUT_SECONDS = 8
QINTOPIA_WEATHER_TOOL = "qintopia_weather_lookup"
QWEATHER_ALLOWED_MCP_TOOLS = {
    "get_weather_now",
    "get_hourly_weather",
    "get_minutely_5m",
    "get_air_quality",
}
QWEATHER_FORBIDDEN_TOOL_PATTERNS = {
    "cyclone",
    "typhoon",
    "tropical",
    "storm_track",
    "ocean",
    "marine",
    "tide",
    "tidal",
    "ocean_current",
    "tidal_current",
    "wave",
    "seawater",
    "solar",
    "radiation",
    "poi",
    "station",
    "台风",
    "热带气旋",
    "海洋",
    "潮汐",
    "潮流",
    "浪高",
    "海温",
    "太阳辐射",
    "兴趣点",
    "监测站",
}
QWEATHER_IMPORT_LOCK = Lock()
ASIA_SHANGHAI = ZoneInfo("Asia/Shanghai")
DAILY_FORECAST_PERIODS = (
    ("midday", "中午", 11, 13),
    ("afternoon", "下午", 14, 17),
    ("evening", "晚上", 18, 22),
)


QINTOPIA_WEATHER_LOOKUP_SCHEMA = {
    "description": (
        "Look up Qintopia weather through a narrow QWeather MCP wrapper. "
        "It is fixed to Qintopia coordinates, uses Open-Meteo only as a limited "
        "fallback, and never exposes typhoon, ocean, solar-radiation, POI, or "
        "arbitrary-city weather capabilities."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Original member weather question.",
            },
            "intent": {
                "type": "string",
                "enum": [
                    "current",
                    "umbrella",
                    "thunderstorm",
                    "warning",
                    "air_quality",
                    "general",
                ],
                "description": "Weather intent. Defaults to general.",
            },
            "hours": {
                "type": "integer",
                "minimum": 1,
                "maximum": 24,
                "description": "Forecast horizon in hours. Defaults to 24 and is capped at 24.",
            },
        },
        "additionalProperties": False,
    },
}


def _json(data: dict[str, Any]) -> str:
    return json.dumps(data, ensure_ascii=False, separators=(",", ":"))


def _clean_text(value: Any, *, max_len: int = 1200) -> str:
    cleaned = re.sub(r"\s+", " ", str(value or "")).strip()
    return cleaned[:max_len]


def _session_env(name: str) -> str:
    try:
        from gateway.session_context import get_session_env

        return _clean_text(get_session_env(name, ""), max_len=4000)
    except Exception:
        return _clean_text(os.getenv(name, ""), max_len=4000)


def _qintopia_weather_location() -> str:
    return _session_env("QINTOPIA_WEATHER_LOCATION") or DEFAULT_QINTOPIA_WEATHER_LOCATION


def _qintopia_weather_location_name() -> str:
    return (
        _session_env("QINTOPIA_WEATHER_LOCATION_NAME")
        or DEFAULT_QINTOPIA_WEATHER_LOCATION_NAME
    )


def _qintopia_weather_qweather_city() -> str:
    return (
        _session_env("QINTOPIA_WEATHER_QWEATHER_CITY")
        or DEFAULT_QINTOPIA_WEATHER_QWEATHER_CITY
    )


def _qintopia_weather_mcp_timeout() -> float:
    try:
        raw = float(
            _session_env("QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS")
            or DEFAULT_QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS
        )
    except (TypeError, ValueError):
        raw = DEFAULT_QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS
    return min(max(raw, 2.0), 30.0)


def _qintopia_weather_horizon(args: dict[str, Any]) -> int:
    try:
        raw = int(args.get("hours") or 24)
    except (TypeError, ValueError):
        raw = 24
    return min(max(raw, 1), 24)


def _qintopia_weather_now_iso() -> str:
    return datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds")


def _shanghai_datetime(value: Any) -> datetime | None:
    text = _clean_text(value, max_len=80)
    if not text:
        return None
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=ASIA_SHANGHAI)
    return parsed.astimezone(ASIA_SHANGHAI)


def _forecast_date() -> str:
    current = _shanghai_datetime(_qintopia_weather_now_iso())
    if current is None:
        current = datetime.now(ASIA_SHANGHAI)
    return current.date().isoformat()


def _to_int(value: Any) -> int | None:
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return None


def _to_float(value: Any) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _qweather_forbidden_tool_names(tool_names: list[str]) -> list[str]:
    forbidden = []
    for name in tool_names:
        lowered = name.lower()
        if any(
            pattern in lowered or pattern in name
            for pattern in QWEATHER_FORBIDDEN_TOOL_PATTERNS
        ):
            forbidden.append(name)
    return sorted(set(forbidden))


def _qweather_mcp_call(tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    if tool_name not in QWEATHER_ALLOWED_MCP_TOOLS:
        return {
            "success": False,
            "error": "QWeather MCP tool is not allowlisted for Erhua",
            "tool": tool_name,
            "allowlist": sorted(QWEATHER_ALLOWED_MCP_TOOLS),
        }
    previous_disable_level = logging.root.manager.disable
    try:
        with QWEATHER_IMPORT_LOCK:
            logging.disable(logging.INFO)
            logging.getLogger("hefeng_qweather_mcp").setLevel(logging.ERROR)
            logging.getLogger("httpx").setLevel(logging.WARNING)
            sys.modules.pop("hefeng_qweather_mcp.main", None)
            module = importlib.import_module("hefeng_qweather_mcp.main")
            handler = getattr(module, tool_name)
            executor = ThreadPoolExecutor(max_workers=1)
            future = executor.submit(handler, **arguments)
            try:
                payload = future.result(timeout=_qintopia_weather_mcp_timeout())
            except TimeoutError:
                future.cancel()
                executor.shutdown(wait=False, cancel_futures=True)
                return {
                    "success": False,
                    "error": "QWeather MCP package call timed out",
                    "tool": tool_name,
                    "timeout_seconds": _qintopia_weather_mcp_timeout(),
                }
            finally:
                if future.done():
                    executor.shutdown(wait=False, cancel_futures=True)
        if payload is None:
            return {
                "success": False,
                "error": "QWeather MCP tool returned no data",
                "tool": tool_name,
            }
        return {"success": True, "tool": tool_name, "data": payload}
    except ImportError:
        return {
            "success": False,
            "error": "hefeng-qweather-mcp is not installed in the Hermes Python environment",
            "tool": tool_name,
        }
    except Exception as exc:
        return {
            "success": False,
            "error": "QWeather MCP package call failed",
            "tool": tool_name,
            "detail": _clean_text(exc, max_len=300),
        }
    finally:
        logging.disable(previous_disable_level)


def _qweather_call_bundle(location: str) -> dict[str, dict[str, Any]]:
    calls = {
        "current": ("get_weather_now", {"location": location, "lang": "zh", "unit": "m"}),
        "hourly": (
            "get_hourly_weather",
            {"location": location, "hours": "24h", "lang": "zh", "unit": "m"},
        ),
        "minutely": ("get_minutely_5m", {"location": location, "lang": "zh"}),
        "air_quality": (
            "get_air_quality",
            {"city": _qintopia_weather_qweather_city()},
        ),
    }
    with ThreadPoolExecutor(max_workers=len(calls)) as executor:
        futures = {
            name: executor.submit(_qweather_mcp_call, tool_name, arguments)
            for name, (tool_name, arguments) in calls.items()
        }
        return {name: future.result() for name, future in futures.items()}


def _qweather_weather_alert_current(location: str) -> dict[str, Any]:
    try:
        lon, lat = [float(part.strip()) for part in location.split(",", 1)]
    except (AttributeError, TypeError, ValueError):
        return {"success": False, "error": "invalid fixed lon,lat coordinates"}

    previous_disable_level = logging.root.manager.disable
    try:
        with QWEATHER_IMPORT_LOCK:
            logging.disable(logging.INFO)
            logging.getLogger("hefeng_qweather_mcp").setLevel(logging.ERROR)
            sys.modules.pop("hefeng_qweather_mcp.main", None)
            module = importlib.import_module("hefeng_qweather_mcp.main")
            api_host = _clean_text(getattr(module, "api_host", ""), max_len=200)
            auth_header = getattr(module, "auth_header", None)
        if not api_host or not isinstance(auth_header, dict):
            return {"success": False, "error": "QWeather auth context unavailable"}

        headers = {str(key): str(value) for key, value in auth_header.items()}
        headers.setdefault("Accept", "application/json")
        headers.setdefault("Accept-Encoding", "identity")
        path = f"/weatheralert/v1/current/{lat:.2f}/{lon:.2f}"
        query = urlencode({"localTime": "true", "lang": "zh"})
        request = urlrequest.Request(f"https://{api_host}{path}?{query}", headers=headers)
        with urlrequest.urlopen(request, timeout=_qintopia_weather_mcp_timeout()) as response:
            body = response.read(1_000_000)
            if response.headers.get("Content-Encoding", "").lower() == "gzip":
                body = gzip.decompress(body)
            data = json.loads(body.decode("utf-8"))
        return {"success": True, "source": "weatheralert_v1", "data": data}
    except urlerror.HTTPError as exc:
        detail = ""
        try:
            detail = exc.read(10_000).decode("utf-8", errors="replace")
        except Exception:
            detail = ""
        return {
            "success": False,
            "source": "weatheralert_v1",
            "error": "QWeather Weather Alert API returned HTTP error",
            "status": exc.code,
            "detail": _clean_text(detail, max_len=300),
        }
    except Exception as exc:
        return {
            "success": False,
            "source": "weatheralert_v1",
            "error": "QWeather Weather Alert API call failed",
            "detail": _clean_text(exc, max_len=300),
        }
    finally:
        logging.disable(previous_disable_level)


def _qweather_data(call: dict[str, Any], key: str) -> Any:
    if not call.get("success") or not isinstance(call.get("data"), dict):
        return None
    return call["data"].get(key)


def _qweather_rainy_hour(hour: dict[str, Any]) -> bool:
    text = _clean_text(hour.get("text"), max_len=80)
    try:
        pop = int(float(hour.get("pop") or 0))
    except (TypeError, ValueError):
        pop = 0
    try:
        precip = float(hour.get("precip") or 0)
    except (TypeError, ValueError):
        precip = 0.0
    return bool(re.search(r"雨|雷|阵雨|降水", text)) or pop >= 40 or precip >= 0.1


def _qweather_thunder_hour(hour: dict[str, Any]) -> bool:
    text = _clean_text(hour.get("text"), max_len=80)
    return bool(re.search(r"雷|雷阵雨|雷暴", text))


def _qweather_precip_minute(item: dict[str, Any]) -> bool:
    try:
        return float(item.get("precip") or 0) > 0
    except (TypeError, ValueError):
        return False


def _time_windows(
    items: list[dict[str, Any]], predicate, time_key: str, *, max_windows: int = 6
) -> list[dict[str, str]]:
    windows = []
    start = ""
    end = ""
    for item in items:
        when = _clean_text(item.get(time_key), max_len=40)
        if not when:
            continue
        if predicate(item):
            if not start:
                start = when
            end = when
        elif start:
            windows.append({"start": start, "end": end})
            start = ""
            end = ""
    if start:
        windows.append({"start": start, "end": end})
    return windows[:max_windows]


def _window_reason(
    items: list[dict[str, Any]], start: str, end: str, predicate, default: str
) -> str:
    weather_texts = []
    max_pop = 0
    max_precip = 0.0
    for item in items:
        when = _clean_text(item.get("fxTime") or item.get("time"), max_len=40)
        if not when or when < start or when > end or not predicate(item):
            continue
        text = _clean_text(item.get("text"), max_len=40)
        try:
            pop = int(float(item.get("pop") or 0))
        except (TypeError, ValueError):
            pop = 0
        try:
            precip = float(item.get("precip") or item.get("precipitation") or 0)
        except (TypeError, ValueError):
            precip = 0.0
        if text and re.search(r"雨|雪|雷|雹|降水", text):
            weather_texts.append(text)
        if pop:
            max_pop = max(max_pop, pop)
        if precip:
            max_precip = max(max_precip, precip)
    reasons = list(dict.fromkeys(weather_texts))[:1]
    if max_pop:
        reasons.append(f"降水概率最高{max_pop}%")
    if max_precip:
        reasons.append(f"降水最高{max_precip:g}mm")
    if not reasons:
        return default
    return "、".join(reasons[:3])


def _annotate_windows(
    windows: list[dict[str, str]],
    items: list[dict[str, Any]],
    predicate,
    default_reason: str,
) -> list[dict[str, str]]:
    annotated = []
    for window in windows:
        start = _clean_text(window.get("start"), max_len=40)
        end = _clean_text(window.get("end"), max_len=40)
        annotated.append(
            {
                "start": start,
                "end": end,
                "reason": _window_reason(items, start, end, predicate, default_reason),
            }
        )
    return annotated


def _qweather_current(now: Any) -> dict[str, Any] | None:
    if not isinstance(now, dict):
        return None
    return {
        "obs_time": _clean_text(now.get("obsTime"), max_len=40),
        "text": _clean_text(now.get("text"), max_len=80),
        "temp_c": _clean_text(now.get("temp"), max_len=20),
        "feels_like_c": _clean_text(now.get("feelsLike"), max_len=20),
        "humidity_pct": _clean_text(now.get("humidity"), max_len=20),
        "wind_dir": _clean_text(now.get("windDir"), max_len=40),
        "wind_scale": _clean_text(now.get("windScale"), max_len=20),
        "wind_speed_kmh": _clean_text(now.get("windSpeed"), max_len=20),
        "precip_mm": _clean_text(now.get("precip"), max_len=20),
    }


def _warning_level_code(level: Any) -> str:
    text = _clean_text(level, max_len=40)
    lowered = text.lower()
    if "red" in lowered or "红" in text:
        return "red"
    if "orange" in lowered or "橙" in text:
        return "orange"
    if "yellow" in lowered or "黄" in text:
        return "yellow"
    if "blue" in lowered or "蓝" in text:
        return "blue"
    return "unknown"


def _warning_level_copy(level: Any) -> str:
    return {
        "red": "红色",
        "orange": "橙色",
        "yellow": "黄色",
        "blue": "蓝色",
    }.get(_warning_level_code(level), "未标注级别")


def _qweather_weather_alerts(data: Any) -> list[dict[str, str]]:
    if not isinstance(data, dict):
        return []
    alerts = data.get("alerts")
    if not isinstance(alerts, list):
        return []
    warnings = []
    for item in alerts:
        if not isinstance(item, dict):
            continue
        event_type = item.get("eventType") if isinstance(item.get("eventType"), dict) else {}
        color = item.get("color") if isinstance(item.get("color"), dict) else {}
        message_type = (
            item.get("messageType") if isinstance(item.get("messageType"), dict) else {}
        )
        warnings.append(
            {
                "id": _clean_text(item.get("id"), max_len=80),
                "sender": _clean_text(item.get("senderName"), max_len=120),
                "title": _clean_text(
                    item.get("headline") or item.get("description"), max_len=160
                ),
                "type": _clean_text(event_type.get("name"), max_len=80),
                "type_code": _clean_text(event_type.get("code"), max_len=40),
                "level": _clean_text(color.get("code") or item.get("severity"), max_len=40),
                "status": _clean_text(message_type.get("code"), max_len=40),
                "start_time": _clean_text(
                    item.get("effectiveTime") or item.get("issuedTime"), max_len=40
                ),
                "expire_time": _clean_text(item.get("expireTime"), max_len=40),
                "description": _clean_text(item.get("description"), max_len=500),
                "instruction": _clean_text(item.get("instruction"), max_len=500),
            }
        )
    level_priority = {"red": 0, "orange": 1, "yellow": 2, "blue": 3, "unknown": 4}
    return sorted(
        warnings,
        key=lambda item: level_priority.get(_warning_level_code(item.get("level")), 4),
    )


def _weather_window_time_label(value: str) -> str:
    text = _clean_text(value, max_len=40)
    if not text:
        return ""
    match = re.search(r"T(\d{2}:\d{2})", text)
    if match:
        return match.group(1)
    match = re.search(r"\b(\d{2}:\d{2})\b", text)
    if match:
        return match.group(1)
    return text


def _weather_window_copy(windows: list[dict[str, str]], empty: str) -> str:
    if not windows:
        return empty
    parts = []
    for window in windows[:3]:
        start = _weather_window_time_label(window.get("start", ""))
        end = _weather_window_time_label(window.get("end", ""))
        reason = _clean_text(window.get("reason"), max_len=80)
        if start and end and start != end:
            label = f"{start}-{end}"
        else:
            label = start or end
        if reason:
            parts.append(f"{label} {reason}" if label else reason)
        elif label:
            parts.append(label)
    if not parts:
        return empty
    return "；".join(parts)


def _unique_join(values: list[str], separator: str = "转") -> str | None:
    deduped: list[str] = []
    seen: set[str] = set()
    for value in values:
        text = _clean_text(value, max_len=80)
        if not text or text in seen:
            continue
        seen.add(text)
        deduped.append(text)
    if not deduped:
        return None
    return separator.join(deduped)


def _wind_summary(rows: list[dict[str, Any]]) -> str | None:
    directions: list[str] = []
    scale_values: set[int] = set()
    for row in rows:
        direction = _clean_text(row.get("windDir"), max_len=40)
        if direction:
            directions.append(direction)
        scale = _to_int(row.get("windScale"))
        if scale is not None:
            scale_values.add(scale)
    direction_text = _unique_join(directions)
    sorted_scales = sorted(scale_values)
    scale_text = None
    if sorted_scales:
        scale_text = (
            f"{sorted_scales[0]}-{sorted_scales[-1]}级"
            if len(sorted_scales) > 1
            else f"{sorted_scales[0]}级"
        )
    if direction_text and scale_text:
        return f"{direction_text}{scale_text}"
    return direction_text or scale_text


def _build_daily_period(
    *,
    period_id: str,
    label: str,
    start_hour: int,
    end_hour: int,
    hourly: list[dict[str, Any]],
    forecast_date: str,
) -> dict[str, Any]:
    hourly_by_hour: dict[int, dict[str, Any]] = {}
    for row in hourly:
        if not isinstance(row, dict):
            continue
        local_time = _shanghai_datetime(row.get("fxTime") or row.get("time"))
        if local_time is None or local_time.date().isoformat() != forecast_date:
            continue
        if not start_hour <= local_time.hour <= end_hour:
            continue
        hourly_by_hour.setdefault(local_time.hour, row)

    rows = [hourly_by_hour[hour] for hour in sorted(hourly_by_hour)]
    expected_hours = end_hour - start_hour + 1
    coverage_hours = len(rows)
    if coverage_hours == 0:
        status = "unknown"
    elif coverage_hours == expected_hours:
        status = "complete"
    else:
        status = "partial"

    temperatures = [
        value for value in (_to_int(row.get("temp")) for row in rows) if value is not None
    ]
    precip_probabilities = [
        value for value in (_to_int(row.get("pop")) for row in rows) if value is not None
    ]
    precip_amounts = [
        value for value in (_to_float(row.get("precip")) for row in rows) if value is not None
    ]
    conditions = [
        _clean_text(row.get("text"), max_len=40)
        for row in rows
        if _clean_text(row.get("text"), max_len=40)
    ]

    return {
        "id": period_id,
        "label": label,
        "start_local": f"{start_hour:02d}:00",
        "end_local": f"{end_hour:02d}:59",
        "status": status,
        "condition": _unique_join(conditions),
        "temp_min_c": min(temperatures) if temperatures else None,
        "temp_max_c": max(temperatures) if temperatures else None,
        "max_precip_probability_pct": max(precip_probabilities) if precip_probabilities else None,
        "max_precip_mm": max(precip_amounts) if precip_amounts else None,
        "wind_summary": _wind_summary(rows),
        "coverage_hours": coverage_hours,
    }


def _daily_periods(hourly: list[dict[str, Any]], forecast_date: str) -> list[dict[str, Any]]:
    return [
        _build_daily_period(
            period_id=period_id,
            label=label,
            start_hour=start_hour,
            end_hour=end_hour,
            hourly=hourly,
            forecast_date=forecast_date,
        )
        for period_id, label, start_hour, end_hour in DAILY_FORECAST_PERIODS
    ]


def _weather_warning_action(warning: dict[str, str]) -> str:
    instruction = _clean_text(warning.get("instruction"), max_len=80)
    if instruction:
        return instruction
    warning_type = _clean_text(warning.get("type") or warning.get("title"), max_len=80)
    if re.search(r"雷|大风|暴雨|冰雹|强对流", warning_type):
        return "减少空旷处停留，出门带伞，留意官方更新。"
    if re.search(r"高温", warning_type):
        return "户外活动注意补水和防晒。"
    if re.search(r"寒潮|大雪|道路结冰|霜冻", warning_type):
        return "注意保暖和路面湿滑。"
    return "留意官方更新，出行把安全放前面。"


def _weather_warning_summary(
    warnings: list[dict[str, str]], warning_source: dict[str, Any]
) -> dict[str, str]:
    if warnings:
        level_priority = {"red": 0, "orange": 1, "yellow": 2, "blue": 3, "unknown": 4}
        ordered = sorted(
            warnings,
            key=lambda item: level_priority.get(_warning_level_code(item.get("level")), 4),
        )
        if len(ordered) > 1:
            rendered = []
            for item in ordered[:2]:
                sender = _clean_text(item.get("sender"), max_len=80)
                warning_type = _clean_text(
                    item.get("type") or item.get("title") or "天气", max_len=80
                )
                level = _warning_level_copy(item.get("level"))
                rendered.append(f"{sender}{warning_type}{level}预警")
            remaining = len(ordered) - len(rendered)
            if remaining:
                rendered.append(f"另有 {remaining} 条")
            first_action = _weather_warning_action(ordered[0])
            return {
                "status": "present",
                "copy": "；".join([*rendered, first_action]),
                "action": first_action,
            }

        warning = warnings[0]
        warning_type = _clean_text(
            warning.get("type") or warning.get("title") or "天气", max_len=80
        )
        level = _warning_level_copy(warning.get("level"))
        start_time = _clean_text(
            warning.get("start_time") or "生效时间未标注", max_len=40
        )
        action = _weather_warning_action(warning)
        return {
            "status": "present",
            "copy": f"{warning_type}{level}预警，{start_time}生效；{action}",
            "action": action,
        }
    if warning_source.get("success"):
        return {
            "status": "none",
            "copy": "截至早上播报时，官方暂无秦托邦天气预警",
            "action": "正常关注天气变化就好。",
        }
    return {
        "status": "unknown",
        "copy": "官方预警数据暂未确认",
        "action": "出门前再看一眼官方天气预警。",
    }


def _weather_morning_reference(current: Any, air_quality: Any) -> dict[str, Any]:
    if not isinstance(current, dict):
        current = {}
    if not isinstance(air_quality, dict):
        air_quality = {}
    parts = []
    text = _clean_text(current.get("text"), max_len=40)
    temp = _clean_text(current.get("temp_c"), max_len=20)
    feels_like = _clean_text(current.get("feels_like_c"), max_len=20)
    humidity = _clean_text(current.get("humidity_pct"), max_len=20)
    precip = _clean_text(current.get("precip_mm"), max_len=20)
    wind_dir = _clean_text(current.get("wind_dir"), max_len=40)
    wind_scale = _clean_text(current.get("wind_scale"), max_len=20)
    wind_speed = _clean_text(current.get("wind_speed_kmh"), max_len=20)
    if text or temp:
        parts.append(f"{text or '天气'}{temp}°C" if temp else text)
    if feels_like:
        parts.append(f"体感{feels_like}°C")
    if humidity:
        parts.append(f"湿度{humidity}%")
    if precip and precip not in {"0", "0.0", "0.00"}:
        parts.append(f"降水{precip}mm")
    if wind_dir or wind_scale:
        wind = f"{wind_dir}{wind_scale}级" if wind_scale else wind_dir
        parts.append(wind)
    elif wind_speed:
        parts.append(f"风速{wind_speed}km/h")
    copy = (
        "今早参考：" + "，".join(parts)
        if parts
        else "今早参考：当前温度和风暂未确认"
    )
    return {
        "current": current or None,
        "air_quality": air_quality or None,
        "copy": copy,
    }


def _weather_period_copy(period: dict[str, Any]) -> str:
    label = _clean_text(period.get("label"), max_len=20)
    if period.get("status") == "unknown":
        return f"{label}暂未确认"
    details = []
    condition = _clean_text(period.get("condition"), max_len=80)
    if condition:
        details.append(condition)
    temp_min = period.get("temp_min_c")
    temp_max = period.get("temp_max_c")
    if temp_min is not None and temp_max is not None:
        details.append(
            f"{temp_min}°C" if temp_min == temp_max else f"{temp_min}-{temp_max}°C"
        )
    max_pop = period.get("max_precip_probability_pct")
    if max_pop is not None:
        details.append(f"降水概率最高{max_pop}%")
    if period.get("status") == "partial":
        details.append("部分时段暂未确认")
    return f"{label}{'，'.join(details)}" if details else f"{label}暂未确认"


def _weather_date_label(forecast_date: str) -> str:
    try:
        parsed = datetime.fromisoformat(forecast_date)
    except ValueError:
        return forecast_date
    return f"{parsed.month}月{parsed.day}日"


def _weather_daily_forecast(
    *,
    hourly: list[dict[str, Any]],
    umbrella_windows: list[dict[str, str]],
    thunderstorm_windows: list[dict[str, str]],
    warnings: list[dict[str, str]],
    warning_source: dict[str, Any],
    current: Any,
    air_quality: Any,
    forecast_date: str | None = None,
) -> dict[str, Any]:
    forecast_date = forecast_date or _forecast_date()
    periods = _daily_periods(hourly, forecast_date)
    if all(period["coverage_hours"] == 0 for period in periods):
        daily_status = "unknown"
    elif all(period["status"] == "complete" for period in periods):
        daily_status = "complete"
    else:
        daily_status = "partial"

    warning = _weather_warning_summary(warnings, warning_source)
    if daily_status == "unknown":
        rain_copy = "全天分时预报暂不可用"
        thunder_copy = "全天分时预报暂不可用"
    elif daily_status == "partial":
        rain_copy = _weather_window_copy(umbrella_windows, "部分时段降水情况暂未确认")
        thunder_copy = _weather_window_copy(thunderstorm_windows, "部分时段雷暴情况暂未确认")
    else:
        rain_copy = _weather_window_copy(umbrella_windows, "今天白天降水信号不明显")
        thunder_copy = _weather_window_copy(thunderstorm_windows, "今天暂未看到明确雷暴窗口")
    tips = []
    if daily_status != "complete":
        tips.append("分时预报暂未完全确认，出门前再看一眼天气更新。")
    if thunderstorm_windows:
        tips.append("雷阵雨窗口少在树下、空旷处停留。")
    if umbrella_windows:
        tips.append("雨具随身，路面湿滑时慢一点。")
    if warning["status"] == "present":
        tips.append(warning["action"])
    elif warning["status"] == "unknown":
        tips.append("出门前再看一眼官方预警更新。")
    if not tips:
        tips.append("正常出行，傍晚前后再留意一次天气变化。")
    if daily_status == "unknown":
        summary = "今天天气分时预报暂不可用，出门前建议再看更新。"
    elif thunderstorm_windows and daily_status == "partial":
        summary = "今天有雷阵雨风险，但部分时段暂未确认。"
    elif umbrella_windows and daily_status == "partial":
        summary = "今天有降水窗口，但部分时段暂未确认。"
    elif daily_status == "partial":
        summary = "今天天气分时预报不完整，部分时段暂未确认。"
    elif thunderstorm_windows:
        summary = "今天有雷阵雨风险，重点看时段和官方预警。"
    elif umbrella_windows:
        summary = "今天有降水窗口，雨具建议随身。"
    else:
        summary = "今天降水信号不明显，按正常出行准备。"
    morning_reference = _weather_morning_reference(current, air_quality)
    period_copy = "；".join(_weather_period_copy(period) for period in periods)
    lines = [
        f"秦托邦今日天气：{_weather_date_label(forecast_date)}，{summary}",
        f"分时：{period_copy}",
        f"降水/带伞：{rain_copy}",
        f"预警：{warning['copy']}",
        morning_reference["copy"],
    ]
    aqi = ""
    category = ""
    if isinstance(air_quality, dict):
        aqi = _clean_text(air_quality.get("aqi"), max_len=20)
        category = _clean_text(air_quality.get("category"), max_len=40)
    if aqi or category:
        lines.append(f"空气（鄠邑区）：AQI {aqi} {category}".strip())
    else:
        lines.append("空气（鄠邑区）：AQI 暂未确认")
    lines.append(f"出行提示：{tips[0]} 二花播报完毕～")
    return {
        "forecast_date": forecast_date,
        "status": daily_status,
        "periods": periods,
        "summary": summary,
        "umbrella": rain_copy,
        "thunderstorm": thunder_copy,
        "warning_status": warning["status"],
        "warning_copy": warning["copy"],
        "outing_tips": tips,
        "morning_reference": morning_reference,
        "broadcast_lines": lines,
        "broadcast_text": "\n".join(lines),
    }


def _qweather_air_quality(data: Any) -> dict[str, Any] | None:
    if not isinstance(data, dict):
        return None
    now = data.get("now")
    if isinstance(now, dict):
        return {
            "pub_time": _clean_text(now.get("pubTime"), max_len=40),
            "aqi": _clean_text(now.get("aqi"), max_len=20),
            "category": _clean_text(now.get("category"), max_len=80),
            "primary": _clean_text(now.get("primary"), max_len=80),
        }

    indexes = data.get("indexes")
    if not isinstance(indexes, list) or not indexes:
        return None
    primary_index = next((item for item in indexes if item.get("code") == "cn-mee"), indexes[0])
    if not isinstance(primary_index, dict):
        return None
    pollutant = primary_index.get("primaryPollutant")
    if not isinstance(pollutant, dict):
        pollutant = {}
    health = primary_index.get("health")
    advice = health.get("advice") if isinstance(health, dict) else {}
    if not isinstance(advice, dict):
        advice = {}
    return {
        "pub_time": "",
        "aqi": _clean_text(
            primary_index.get("aqiDisplay") or primary_index.get("aqi"), max_len=20
        ),
        "category": _clean_text(primary_index.get("category"), max_len=80),
        "primary": _clean_text(pollutant.get("name") or pollutant.get("code"), max_len=80),
        "health_advice": _clean_text(advice.get("generalPopulation"), max_len=200),
        "source_city": _qintopia_weather_qweather_city(),
    }


def _qweather_successful(bundle: dict[str, dict[str, Any]]) -> bool:
    return any(call.get("success") for call in bundle.values())


def _qweather_payload(args: dict[str, Any], bundle: dict[str, dict[str, Any]]) -> dict[str, Any]:
    hourly = _qweather_data(bundle.get("hourly", {}), "hourly")
    minutely = _qweather_data(bundle.get("minutely", {}), "minutely")
    if not isinstance(hourly, list):
        hourly = []
    if not isinstance(minutely, list):
        minutely = []
    forecast_date = _forecast_date()

    hourly_same_day = []
    for row in hourly:
        if not isinstance(row, dict):
            continue
        local_time = _shanghai_datetime(row.get("fxTime") or row.get("time"))
        if local_time is None or local_time.date().isoformat() != forecast_date:
            continue
        hourly_same_day.append(row)

    minutely_same_day = []
    for row in minutely:
        if not isinstance(row, dict):
            continue
        local_time = _shanghai_datetime(row.get("fxTime") or row.get("time"))
        if local_time is None or local_time.date().isoformat() != forecast_date:
            continue
        minutely_same_day.append(row)

    minutely_windows = _annotate_windows(
        _time_windows(minutely_same_day, _qweather_precip_minute, "fxTime", max_windows=8),
        minutely_same_day,
        _qweather_precip_minute,
        "分钟级降水预报",
    )
    hourly_source = hourly_same_day[: _qintopia_weather_horizon(args)]
    if minutely_windows:
        minutely_start = _shanghai_datetime(minutely_windows[0].get("start"))
        if minutely_start is not None:
            near_term_end = minutely_start.timestamp() + 2 * 60 * 60
            hourly_source = [
                row
                for row in hourly_source
                if (
                    (row_time := _shanghai_datetime(row.get("fxTime") or row.get("time")))
                    is None
                    or row_time.timestamp() > near_term_end
                )
            ]
    hourly_windows = _annotate_windows(
        _time_windows(hourly_source, _qweather_rainy_hour, "fxTime", max_windows=8),
        hourly_source,
        _qweather_rainy_hour,
        "小时预报有降水信号",
    )
    umbrella_windows = sorted(
        [*minutely_windows, *hourly_windows],
        key=lambda window: (
            _shanghai_datetime(window.get("start")) or datetime.max.replace(tzinfo=ASIA_SHANGHAI)
        ),
    )[:8]

    thunderstorm_source = hourly_same_day[: _qintopia_weather_horizon(args)]
    thunderstorm_windows = _time_windows(
        thunderstorm_source, _qweather_thunder_hour, "fxTime"
    )
    thunderstorm_windows = _annotate_windows(
        thunderstorm_windows,
        thunderstorm_source,
        _qweather_thunder_hour,
        "小时预报有雷暴信号",
    )

    errors = {
        name: {
            key: value
            for key, value in call.items()
            if key in {"error", "detail", "status", "exit_code", "timeout_seconds"}
        }
        for name, call in bundle.items()
        if not call.get("success")
    }
    limitations = []
    if "weather_alert" in errors:
        limitations.append(
            "QWeather Weather Alert data unavailable; do not claim no official warning"
        )
    if "air_quality" in errors:
        limitations.append("QWeather air-quality data unavailable")

    weather_alert = bundle.get("weather_alert", {})
    warnings = _qweather_weather_alerts(weather_alert.get("data"))
    current = _qweather_current(_qweather_data(bundle.get("current", {}), "now"))
    air_quality = _qweather_air_quality(bundle.get("air_quality", {}).get("data"))
    daily_forecast = _weather_daily_forecast(
        hourly=hourly_same_day,
        umbrella_windows=umbrella_windows,
        thunderstorm_windows=thunderstorm_windows,
        warnings=warnings,
        warning_source=weather_alert,
        current=current,
        air_quality=air_quality,
        forecast_date=forecast_date,
    )

    payload = {
        "success": True,
        "skill": QINTOPIA_WEATHER_TOOL,
        "source": "qweather_mcp",
        "provider": "QWeather",
        "generated_at": _qintopia_weather_now_iso(),
        "location": {
            "name": _qintopia_weather_location_name(),
            "coordinates": _qintopia_weather_location(),
            "fixed": True,
        },
        "current": current,
        "umbrella_windows": umbrella_windows,
        "thunderstorm_windows": thunderstorm_windows,
        "warnings": warnings,
        "warning_status": daily_forecast["warning_status"],
        "warning_source": weather_alert.get("source") or "weatheralert_v1",
        "air_quality": air_quality,
        "daily_forecast": daily_forecast,
        "morning_reference": daily_forecast["morning_reference"],
        "morning_broadcast": daily_forecast["broadcast_text"],
        "limitations": limitations,
        "guardrails": {
            "allowed_mcp_tools": sorted(QWEATHER_ALLOWED_MCP_TOOLS),
            "excluded_capabilities": [
                "tropical_cyclone_typhoon",
                "ocean_marine",
                "solar_radiation",
            ],
            "fixed_location_only": True,
        },
    }
    if errors:
        payload["partial_errors"] = errors
    return payload


def _open_meteo_fallback() -> dict[str, Any]:
    lon, lat = [part.strip() for part in _qintopia_weather_location().split(",", 1)]
    params = urlencode(
        {
            "latitude": lat,
            "longitude": lon,
            "current": "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m",
            "hourly": "weather_code,precipitation_probability,precipitation",
            "timezone": "Asia/Shanghai",
            "forecast_days": "1",
        }
    )
    url = f"https://api.open-meteo.com/v1/forecast?{params}"
    request = urlrequest.Request(
        url, headers={"User-Agent": "qintopia-weather-fallback/1.0"}
    )
    try:
        with urlrequest.urlopen(request, timeout=DEFAULT_OPEN_METEO_TIMEOUT_SECONDS) as response:
            data = json.loads(response.read(1_000_000).decode("utf-8"))
    except Exception as exc:
        return {
            "success": False,
            "skill": QINTOPIA_WEATHER_TOOL,
            "source": "weather_unavailable",
            "generated_at": _qintopia_weather_now_iso(),
            "error": "QWeather MCP failed and Open-Meteo fallback failed",
            "detail": _clean_text(exc, max_len=300),
            "limitations": ["cannot confirm hourly weather now"],
        }

    hourly = data.get("hourly") if isinstance(data.get("hourly"), dict) else {}
    times = hourly.get("time") if isinstance(hourly.get("time"), list) else []
    probs = (
        hourly.get("precipitation_probability")
        if isinstance(hourly.get("precipitation_probability"), list)
        else []
    )
    precip = (
        hourly.get("precipitation") if isinstance(hourly.get("precipitation"), list) else []
    )
    rows = []
    for idx, when in enumerate(times[:24]):
        rows.append(
            {
                "time": str(when),
                "precipitation_probability": probs[idx] if idx < len(probs) else 0,
                "precipitation": precip[idx] if idx < len(precip) else 0,
            }
        )

    def rainy(row: dict[str, Any]) -> bool:
        try:
            probability = int(float(row.get("precipitation_probability") or 0))
            amount = float(row.get("precipitation") or 0)
        except (TypeError, ValueError):
            return False
        return probability >= 40 or amount >= 0.1

    current = data.get("current") if isinstance(data.get("current"), dict) else {}
    current_payload = {
        "time": _clean_text(current.get("time"), max_len=40),
        "temp_c": current.get("temperature_2m"),
        "feels_like_c": current.get("apparent_temperature"),
        "humidity_pct": current.get("relative_humidity_2m"),
        "wind_speed_kmh": current.get("wind_speed_10m"),
    }
    umbrella_windows = _time_windows(rows, rainy, "time")
    daily_forecast = _weather_daily_forecast(
        hourly=rows,
        umbrella_windows=umbrella_windows,
        thunderstorm_windows=[],
        warnings=[],
        warning_source={"success": False, "source": "open_meteo_fallback"},
        current=current_payload,
        air_quality=None,
    )
    return {
        "success": True,
        "skill": QINTOPIA_WEATHER_TOOL,
        "source": "open_meteo_fallback",
        "provider": "Open-Meteo",
        "generated_at": _qintopia_weather_now_iso(),
        "location": {
            "name": _qintopia_weather_location_name(),
            "coordinates": _qintopia_weather_location(),
            "fixed": True,
        },
        "current": current_payload,
        "umbrella_windows": umbrella_windows,
        "thunderstorm_windows": [],
        "warnings": [],
        "warning_status": "unknown",
        "air_quality": None,
        "daily_forecast": daily_forecast,
        "morning_reference": daily_forecast["morning_reference"],
        "morning_broadcast": daily_forecast["broadcast_text"],
        "limitations": [
            "Open-Meteo fallback only; no QWeather official warnings",
            "no minute-level precipitation conclusion",
            "no air-quality result",
            "no typhoon, ocean, or solar-radiation data",
        ],
        "guardrails": {
            "excluded_capabilities": [
                "tropical_cyclone_typhoon",
                "ocean_marine",
                "solar_radiation",
            ],
            "fixed_location_only": True,
        },
    }


def handle_qintopia_weather_lookup(args: dict[str, Any], **_: Any) -> str:
    location = _qintopia_weather_location()
    if "," not in location:
        return _json(
            {
                "success": False,
                "skill": QINTOPIA_WEATHER_TOOL,
                "error": "QINTOPIA_WEATHER_LOCATION must be fixed lon,lat coordinates",
                "guardrails": {
                    "fixed_location_only": True,
                    "excluded_capabilities": [
                        "tropical_cyclone_typhoon",
                        "ocean_marine",
                        "solar_radiation",
                    ],
                },
            }
        )

    bundle = _qweather_call_bundle(location)
    bundle["weather_alert"] = _qweather_weather_alert_current(location)
    if _qweather_successful(bundle):
        return _json(_qweather_payload(args, bundle))

    fallback = _open_meteo_fallback()
    fallback["qweather_errors"] = {
        name: {
            key: value
            for key, value in call.items()
            if key in {"error", "detail", "status", "exit_code"}
        }
        for name, call in bundle.items()
    }
    return _json(fallback)


def check_weather_lookup_requirements() -> bool:
    return True
