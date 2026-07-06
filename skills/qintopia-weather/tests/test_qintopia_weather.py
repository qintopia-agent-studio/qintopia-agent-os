from __future__ import annotations

import importlib.util
import json
import os
import unittest
from pathlib import Path


def load_plugin():
    plugin_path = Path(__file__).resolve().parents[1] / "__init__.py"
    spec = importlib.util.spec_from_file_location("qintopia_weather_plugin", plugin_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class QintopiaWeatherTest(unittest.TestCase):
    def setUp(self) -> None:
        self.old_env = {
            name: os.environ.get(name)
            for name in [
                "QINTOPIA_WEATHER_LOCATION",
                "QINTOPIA_WEATHER_LOCATION_NAME",
                "QINTOPIA_WEATHER_QWEATHER_CITY",
                "QINTOPIA_WEATHER_MCP_TIMEOUT_SECONDS",
            ]
        }
        for name in self.old_env:
            os.environ.pop(name, None)
        self.module = load_plugin()

    def tearDown(self) -> None:
        for name, value in self.old_env.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value

    def test_weather_lookup_uses_qweather_allowlisted_tools(self):
        calls = []

        def fake_call(tool_name, arguments):
            calls.append((tool_name, arguments))
            if tool_name == "get_weather_now":
                return {
                    "success": True,
                    "tool": tool_name,
                    "data": {
                        "now": {
                            "obsTime": "2026-06-28T08:00+08:00",
                            "text": "多云",
                            "temp": "26",
                            "feelsLike": "27",
                            "humidity": "70",
                            "windDir": "东风",
                            "windScale": "2",
                            "windSpeed": "8",
                            "precip": "0.0",
                        }
                    },
                }
            if tool_name == "get_hourly_weather":
                return {
                    "success": True,
                    "tool": tool_name,
                    "data": {
                        "hourly": [
                            {
                                "fxTime": "2026-06-28T09:00+08:00",
                                "text": "多云",
                                "pop": "20",
                                "precip": "0.0",
                            },
                            {
                                "fxTime": "2026-06-28T10:00+08:00",
                                "text": "雷阵雨",
                                "pop": "75",
                                "precip": "1.2",
                            },
                            {
                                "fxTime": "2026-06-28T11:00+08:00",
                                "text": "小雨",
                                "pop": "60",
                                "precip": "0.5",
                            },
                            {
                                "fxTime": "2026-06-28T12:00+08:00",
                                "text": "阴",
                                "pop": "20",
                                "precip": "0.0",
                            },
                        ]
                    },
                }
            if tool_name == "get_minutely_5m":
                return {
                    "success": True,
                    "tool": tool_name,
                    "data": {
                        "minutely": [
                            {"fxTime": "2026-06-28T08:05+08:00", "precip": "0.0"},
                            {"fxTime": "2026-06-28T08:10+08:00", "precip": "0.1"},
                            {"fxTime": "2026-06-28T08:15+08:00", "precip": "0.2"},
                            {"fxTime": "2026-06-28T08:20+08:00", "precip": "0.0"},
                        ]
                    },
                }
            if tool_name == "get_air_quality":
                return {
                    "success": True,
                    "tool": tool_name,
                    "data": {
                        "now": {
                            "pubTime": "2026-06-28T08:00+08:00",
                            "aqi": "45",
                            "category": "优",
                            "primary": "NA",
                        }
                    },
                }
            return {"success": False, "tool": tool_name, "error": "unexpected"}

        self.module._qweather_mcp_call = fake_call
        self.module._qweather_weather_alert_current = lambda location: {
            "success": True,
            "source": "weatheralert_v1",
            "data": {
                "alerts": [
                    {
                        "headline": "西安市气象台发布雷雨大风黄色预警",
                        "eventType": {"name": "雷雨大风"},
                        "color": {"code": "Yellow"},
                        "messageType": {"code": "Alert"},
                        "effectiveTime": "2026-06-28T08:00+08:00",
                    }
                ]
            },
        }

        payload = json.loads(
            self.module.handle_qintopia_weather_lookup({"intent": "umbrella", "hours": 24})
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["source"], "qweather_mcp")
        self.assertEqual(payload["current"]["text"], "多云")
        self.assertEqual(payload["warnings"][0]["type"], "雷雨大风")
        self.assertEqual(payload["warning_status"], "present")
        self.assertEqual(payload["daily_forecast"]["warning_status"], "present")
        self.assertIn("雷雨大风Yellow预警", payload["daily_forecast"]["warning_copy"])
        self.assertTrue(payload["morning_broadcast"].startswith("秦托邦今日天气："))
        self.assertIn("降水/带伞：", payload["morning_broadcast"])
        self.assertIn("预警：雷雨大风Yellow预警", payload["morning_broadcast"])
        self.assertIn("今早参考：多云26°C", payload["morning_broadcast"])
        self.assertIn("湿度70%", payload["morning_broadcast"])
        self.assertNotRegex(payload["morning_broadcast"].splitlines()[0], r"现在|此时")
        self.assertEqual(payload["air_quality"]["category"], "优")
        self.assertEqual(
            payload["umbrella_windows"],
            [
                {
                    "start": "2026-06-28T08:10+08:00",
                    "end": "2026-06-28T08:15+08:00",
                    "reason": "降水最高0.2mm",
                }
            ],
        )
        self.assertEqual(
            payload["thunderstorm_windows"],
            [
                {
                    "start": "2026-06-28T10:00+08:00",
                    "end": "2026-06-28T10:00+08:00",
                    "reason": "雷阵雨、降水概率最高75%、降水最高1.2mm",
                }
            ],
        )
        self.assertEqual(
            sorted(name for name, _args in calls),
            [
                "get_air_quality",
                "get_hourly_weather",
                "get_minutely_5m",
                "get_weather_now",
            ],
        )
        call_args = {name: args for name, args in calls}
        self.assertEqual(call_args["get_weather_now"]["location"], "108.5876,33.9996")
        self.assertEqual(call_args["get_hourly_weather"]["location"], "108.5876,33.9996")
        self.assertEqual(call_args["get_minutely_5m"]["location"], "108.5876,33.9996")
        self.assertEqual(call_args["get_air_quality"]["city"], "鄠邑区")

    def test_weather_lookup_renders_no_warning_as_official_none(self):
        self.module._qweather_call_bundle = lambda location: {
            "current": {
                "success": True,
                "data": {
                    "now": {
                        "text": "晴",
                        "temp": "26",
                        "feelsLike": "27",
                        "windDir": "东风",
                        "windScale": "2",
                    }
                },
            },
            "hourly": {"success": True, "data": {"hourly": []}},
            "minutely": {"success": True, "data": {"minutely": []}},
            "air_quality": {
                "success": True,
                "data": {"now": {"aqi": "42", "category": "优"}},
            },
        }
        self.module._qweather_weather_alert_current = lambda location: {
            "success": True,
            "source": "weatheralert_v1",
            "data": {"alerts": []},
        }

        payload = json.loads(self.module.handle_qintopia_weather_lookup({"intent": "general"}))

        self.assertEqual(payload["warning_status"], "none")
        self.assertEqual(payload["daily_forecast"]["warning_status"], "none")
        self.assertIn("截至早上播报时，官方暂无秦托邦天气预警", payload["morning_broadcast"])
        self.assertIn("今早参考：晴26°C", payload["morning_broadcast"])

    def test_weather_lookup_renders_missing_warning_data_as_unknown(self):
        self.module._qweather_call_bundle = lambda location: {
            "current": {"success": True, "data": {"now": {"text": "多云", "temp": "25"}}},
            "hourly": {"success": True, "data": {"hourly": []}},
            "minutely": {"success": True, "data": {"minutely": []}},
            "air_quality": {"success": False, "error": "missing air quality"},
        }
        self.module._qweather_weather_alert_current = lambda location: {
            "success": False,
            "source": "weatheralert_v1",
            "error": "timeout",
        }

        payload = json.loads(self.module.handle_qintopia_weather_lookup({"intent": "general"}))

        self.assertEqual(payload["warning_status"], "unknown")
        self.assertIn("weather_alert", payload["partial_errors"])
        self.assertIn("官方预警数据暂未确认", payload["morning_broadcast"])
        self.assertNotIn("官方暂无秦托邦天气预警", payload["morning_broadcast"])

    def test_weather_lookup_parses_new_qweather_air_quality_shape(self):
        data = {
            "indexes": [
                {
                    "code": "cn-mee",
                    "aqi": 67,
                    "aqiDisplay": "67",
                    "category": "良",
                    "primaryPollutant": {"code": "pm10", "name": "PM 10"},
                    "health": {"advice": {"generalPopulation": "一般人群可正常活动。"}},
                }
            ],
            "stations": [{"id": "P510416", "name": "西苑北路977号"}],
        }

        air_quality = self.module._qweather_air_quality(data)

        self.assertEqual(air_quality["aqi"], "67")
        self.assertEqual(air_quality["category"], "良")
        self.assertEqual(air_quality["primary"], "PM 10")
        self.assertEqual(air_quality["health_advice"], "一般人群可正常活动。")
        self.assertNotIn("stations", air_quality)

    def test_weather_lookup_forbids_paid_qweather_capabilities(self):
        allowed = self.module.QWEATHER_ALLOWED_MCP_TOOLS
        forbidden_names = [
            "get_tropical_cyclone",
            "get_typhoon_track",
            "get_ocean_tide",
            "get_marine_weather",
            "get_solar_radiation",
            "search_poi",
            "get_air_quality_stations",
        ]

        for name in forbidden_names:
            self.assertNotIn(name, allowed)
        self.assertEqual(
            self.module._qweather_forbidden_tool_names(forbidden_names),
            sorted(forbidden_names),
        )

    def test_weather_lookup_falls_back_to_open_meteo_as_limited_trend(self):
        self.module._qweather_call_bundle = lambda location: {
            "current": {"success": False, "error": "missing qweather credentials"},
            "hourly": {"success": False, "error": "missing qweather credentials"},
            "minutely": {"success": False, "error": "missing qweather credentials"},
            "air_quality": {"success": False, "error": "missing qweather credentials"},
        }
        self.module._qweather_weather_alert_current = lambda location: {
            "success": False,
            "error": "missing qweather credentials",
        }
        self.module._open_meteo_fallback = lambda: {
            "success": True,
            "skill": "qintopia_weather_lookup",
            "source": "open_meteo_fallback",
            "provider": "Open-Meteo",
            "umbrella_windows": [
                {"start": "2026-06-28T10:00", "end": "2026-06-28T11:00"}
            ],
            "thunderstorm_windows": [],
            "warnings": [],
            "air_quality": None,
            "limitations": [
                "Open-Meteo fallback only; no QWeather official warnings",
                "no minute-level precipitation conclusion",
                "no air-quality result",
                "no typhoon, ocean, or solar-radiation data",
            ],
        }

        payload = json.loads(
            self.module.handle_qintopia_weather_lookup({"intent": "thunderstorm"})
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["source"], "open_meteo_fallback")
        self.assertEqual(payload["warnings"], [])
        self.assertIsNone(payload["air_quality"])
        self.assertIn("no minute-level precipitation conclusion", payload["limitations"])
        self.assertIn("qweather_errors", payload)

    def test_open_meteo_fallback_includes_forecast_first_broadcast(self):
        self.module._qweather_call_bundle = lambda location: {
            "current": {"success": False, "error": "missing qweather credentials"},
            "hourly": {"success": False, "error": "missing qweather credentials"},
            "minutely": {"success": False, "error": "missing qweather credentials"},
            "air_quality": {"success": False, "error": "missing qweather credentials"},
        }
        self.module._qweather_weather_alert_current = lambda location: {
            "success": False,
            "error": "missing qweather credentials",
        }
        self.module._open_meteo_fallback = lambda: {
            "success": True,
            "skill": "qintopia_weather_lookup",
            "source": "open_meteo_fallback",
            "provider": "Open-Meteo",
            "current": {
                "time": "2026-06-28T08:00",
                "temp_c": 24,
                "feels_like_c": 25,
                "humidity_pct": 72,
                "wind_speed_kmh": 8,
            },
            "umbrella_windows": [
                {"start": "2026-06-28T10:00", "end": "2026-06-28T11:00"}
            ],
            "thunderstorm_windows": [],
            "warnings": [],
            "warning_status": "unknown",
            "air_quality": None,
            "daily_forecast": {
                "warning_status": "unknown",
                "warning_copy": "官方预警数据暂未确认",
                "morning_reference": {
                    "copy": "今早参考：24°C，体感25°C，湿度72%，风速8km/h"
                },
            },
            "morning_reference": {
                "copy": "今早参考：24°C，体感25°C，湿度72%，风速8km/h"
            },
            "morning_broadcast": (
                "秦托邦今日天气：今天有降水窗口，雨具建议随身。\n"
                "预警：官方预警数据暂未确认\n"
                "今早参考：24°C，体感25°C，湿度72%，风速8km/h"
            ),
            "limitations": [
                "Open-Meteo fallback only; no QWeather official warnings",
                "no minute-level precipitation conclusion",
                "no air-quality result",
                "no typhoon, ocean, or solar-radiation data",
            ],
        }

        payload = json.loads(self.module.handle_qintopia_weather_lookup({}))

        self.assertEqual(payload["warning_status"], "unknown")
        self.assertTrue(payload["morning_broadcast"].startswith("秦托邦今日天气："))
        self.assertIn("官方预警数据暂未确认", payload["morning_broadcast"])
        self.assertNotIn("官方暂无秦托邦天气预警", payload["morning_broadcast"])

    def test_weather_lookup_rejects_non_fixed_location(self):
        os.environ["QINTOPIA_WEATHER_LOCATION"] = "西安"

        payload = json.loads(self.module.handle_qintopia_weather_lookup({}))

        self.assertFalse(payload["success"])
        self.assertTrue(payload["guardrails"]["fixed_location_only"])
        self.assertIn("fixed lon,lat", payload["error"])


if __name__ == "__main__":
    unittest.main()
