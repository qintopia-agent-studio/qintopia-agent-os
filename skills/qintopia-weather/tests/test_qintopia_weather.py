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

    def _load_weather_fixture(self) -> dict:
        path = (
            Path(__file__).resolve().parents[3]
            / "fixtures"
            / "weather"
            / "qweather-full-day.json"
        )
        return json.loads(path.read_text(encoding="utf-8"))

    def _qweather_fixture_call_bundle(self, fixture: dict) -> dict[str, Any]:
        return json.loads(json.dumps(fixture["bundle"]))

    def _stub_qweather(self, fixture: dict, alerts: list[dict[str, Any]]):
        self.module._qweather_call_bundle = lambda location: self._qweather_fixture_call_bundle(
            fixture
        )
        self.module._qweather_weather_alert_current = lambda location: {
            "success": True,
            "source": "weatheralert_v1",
            "data": {"alerts": alerts},
        }
        self.module._qintopia_weather_now_iso = lambda: fixture["generated_at"]

    def _extract_non_empty_broadcast_lines(self, payload: dict[str, Any]) -> list[str]:
        return [line.strip() for line in payload["morning_broadcast"].splitlines() if line.strip()]

    def _periods_by_id(self, payload: dict[str, Any]) -> dict[str, Any]:
        return {period["id"]: period for period in payload["daily_forecast"]["periods"]}

    def test_weather_lookup_full_day_fixture_day_segments_and_no_next_day_contamination(self):
        fixture = self._load_weather_fixture()
        bundle = self._qweather_fixture_call_bundle(fixture)
        bundle["hourly"]["data"]["hourly"][-1]["text"] = "雷阵雨"
        self._stub_qweather(
            {"generated_at": fixture["generated_at"], "bundle": bundle},
            fixture["bundle"]["weather_alert"]["data"]["alerts"],
        )

        payload = json.loads(
            self.module.handle_qintopia_weather_lookup({"intent": "general"})
        )
        daily = payload["daily_forecast"]
        expected = fixture["expected"]

        self.assertIn("forecast_date", daily)
        self.assertEqual(daily["forecast_date"], expected["forecast_date"])
        self.assertIn("status", daily)
        self.assertEqual(daily["status"], expected["status"])
        self.assertIn("periods", daily)
        self.assertEqual([period["id"] for period in daily["periods"]], expected["period_ids"])
        self.assertEqual(
            [period["coverage_hours"] for period in daily["periods"]],
            expected["period_coverage_hours"],
        )

        periods = self._periods_by_id(payload)
        self.assertEqual(
            [periods["midday"]["temp_min_c"], periods["midday"]["temp_max_c"]],
            expected["midday_temp_range_c"],
        )
        self.assertEqual(
            [periods["afternoon"]["temp_min_c"], periods["afternoon"]["temp_max_c"]],
            expected["afternoon_temp_range_c"],
        )
        self.assertEqual(
            [periods["evening"]["temp_min_c"], periods["evening"]["temp_max_c"]],
            expected["evening_temp_range_c"],
        )
        self.assertEqual(
            periods["afternoon"]["max_precip_probability_pct"],
            expected["afternoon_max_precip_probability_pct"],
        )
        self.assertEqual(
            periods["evening"]["max_precip_mm"],
            expected["evening_max_precip_mm"],
        )
        self.assertFalse(
            any(window["start"].startswith("2026-07-07") for window in payload["umbrella_windows"])
        )
        self.assertFalse(
            any(window["end"].startswith("2026-07-07") for window in payload["umbrella_windows"])
        )
        self.assertFalse(
            any(window["start"].startswith("2026-07-07") for window in payload["thunderstorm_windows"])
        )
        self.assertFalse(
            any(window["end"].startswith("2026-07-07") for window in payload["thunderstorm_windows"])
        )
        self.assertNotIn("2026-07-07", payload["morning_broadcast"])
        self.assertNotEqual(
            daily["periods"][2]["temp_max_c"],
            expected["excluded_next_day_temp_c"],
        )

    def test_weather_lookup_minutely_overlap_keeps_later_hourly_rain_window(self):
        fixture = self._load_weather_fixture()
        bundle = self._qweather_fixture_call_bundle(fixture)
        bundle["minutely"]["data"]["minutely"] = [
            {"fxTime": "2026-07-06T07:10+08:00", "precip": "0.3"},
            {"fxTime": "2026-07-06T07:20+08:00", "precip": "0.2"},
        ]
        bundle["hourly"]["data"]["hourly"] = [
            {
                "fxTime": "2026-07-06T08:00+08:00",
                "text": "阴",
                "pop": "60",
                "precip": "0.7",
                "windDir": "东北风",
                "windScale": "2",
            },
            {
                "fxTime": "2026-07-06T09:00+08:00",
                "text": "中雨",
                "pop": "75",
                "precip": "1.1",
                "windDir": "东北风",
                "windScale": "3",
            },
            {
                "fxTime": "2026-07-06T10:00+08:00",
                "text": "小雨",
                "pop": "50",
                "precip": "0.6",
                "windDir": "东北风",
                "windScale": "3",
            },
            {
                "fxTime": "2026-07-06T11:00+08:00",
                "text": "阴",
                "pop": "10",
                "precip": "0.0",
                "windDir": "东风",
                "windScale": "2",
            },
        ]
        self._stub_qweather(
            {"generated_at": fixture["generated_at"], "bundle": bundle},
            [],
        )

        payload = json.loads(
            self.module.handle_qintopia_weather_lookup({"intent": "umbrella"})
        )
        starts = [window["start"] for window in payload["umbrella_windows"]]
        self.assertIn("2026-07-06T07:10+08:00", starts)
        self.assertTrue(
            any(window["start"] == "2026-07-06T10:00+08:00" for window in payload["umbrella_windows"]),
            "后续雨窗应保留10:00或裁剪后的后续窗口",
        )

    def test_weather_lookup_retains_early_minutely_and_later_afternoon_hourly_rain(self):
        fixture = self._load_weather_fixture()
        self._stub_qweather(fixture, [])

        payload = json.loads(
            self.module.handle_qintopia_weather_lookup({"intent": "umbrella"})
        )

        windows = payload["umbrella_windows"]
        self.assertGreaterEqual(len(windows), 2)
        starts = [window["start"] for window in windows]
        self.assertIn("2026-07-06T07:10+08:00", starts)
        self.assertIn("2026-07-06T15:00+08:00", starts)

        periods = self._periods_by_id(payload)
        self.assertEqual(periods["midday"]["status"], "complete")
        self.assertEqual(periods["afternoon"]["status"], "complete")
        self.assertGreater(periods["afternoon"]["max_precip_mm"], 0.0)

    def test_weather_lookup_hourly_missing_or_failed_yields_unknown_without_optimistic_copy(self):
        fixture = self._load_weather_fixture()
        failing_bundle = self._qweather_fixture_call_bundle(fixture)
        failing_bundle["hourly"] = {"success": False, "error": "hourly unavailable"}

        self.module._qweather_call_bundle = lambda location: failing_bundle
        self.module._qweather_weather_alert_current = lambda location: {
            "success": False,
            "source": "weatheralert_v1",
            "error": "timeout",
        }
        self.module._qintopia_weather_now_iso = lambda: fixture["generated_at"]

        payload = json.loads(self.module.handle_qintopia_weather_lookup({"intent": "general"}))
        daily = payload["daily_forecast"]

        self.assertIn("status", daily)
        self.assertEqual(daily["status"], "unknown")
        self.assertEqual(payload["warning_status"], "unknown")
        self.assertIn("官方预警数据暂未确认", payload["morning_broadcast"])
        self.assertNotIn("官方暂无秦托邦天气预警", payload["morning_broadcast"])

        lines = self._extract_non_empty_broadcast_lines(payload)
        self.assertTrue(all("降水信号不明显" not in line for line in lines))
        self.assertTrue(all("天气稳" not in line for line in lines))
        self.assertTrue(all("轻松安排" not in line for line in lines))

    def test_weather_lookup_partial_forecast_when_one_period_missing(self):
        fixture = self._load_weather_fixture()
        partial_bundle = self._qweather_fixture_call_bundle(fixture)
        partial_bundle["hourly"]["data"]["hourly"] = [
            row
            for row in partial_bundle["hourly"]["data"]["hourly"]
            if not (
                row["fxTime"].startswith("2026-07-06T14:")
                or row["fxTime"].startswith("2026-07-06T15:")
                or row["fxTime"].startswith("2026-07-06T16:")
                or row["fxTime"].startswith("2026-07-06T17:")
            )
        ]

        self._stub_qweather(
            {
                "generated_at": fixture["generated_at"],
                "bundle": partial_bundle,
            },
            [],
        )

        payload = json.loads(self.module.handle_qintopia_weather_lookup({"intent": "general"}))
        self.assertIn("daily_forecast", payload)
        self.assertIn("periods", payload["daily_forecast"])
        periods = self._periods_by_id(payload)

        self.assertEqual(payload["daily_forecast"]["status"], "partial")
        self.assertEqual(periods["midday"]["status"], "complete")
        self.assertEqual(periods["afternoon"]["status"], "unknown")
        self.assertEqual(periods["evening"]["status"], "complete")
        self.assertEqual(periods["afternoon"]["coverage_hours"], 0)

    def test_weather_lookup_multiple_warning_colors_prioritize_two_and_append_remaining_count(self):
        fixture = self._load_weather_fixture()
        self._stub_qweather(fixture, fixture["bundle"]["weather_alert"]["data"]["alerts"])

        payload = json.loads(self.module.handle_qintopia_weather_lookup({"intent": "warning"}))
        lines = self._extract_non_empty_broadcast_lines(payload)
        warning_lines = [line for line in lines if line.startswith("预警：")]
        self.assertTrue(warning_lines)
        warning_line = warning_lines[0]
        self.assertIn("橙色", warning_line)
        self.assertIn("黄色", warning_line)
        self.assertIn("另有 1 条", warning_line)

        air_lines = [line for line in lines if "空气（鄠邑区）" in line]
        self.assertEqual(len(air_lines), 1)
        self.assertNotIn("今早参考", air_lines[0])

    def test_open_meteo_fallback_broadcast_keeps_fewer_lines_and_never_no_warning_or_aqi_claim(self):
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
                "time": "2026-07-06T08:00",
                "temp_c": 24,
                "feels_like_c": 25,
                "humidity_pct": 72,
                "wind_speed_kmh": 8,
            },
            "umbrella_windows": [
                {"start": "2026-07-06T10:00", "end": "2026-07-06T11:00"}
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
                "空气（鄠邑区）：AQI 暂未确认\n"
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
        lines = self._extract_non_empty_broadcast_lines(payload)
        self.assertLessEqual(len(lines), 8)
        self.assertEqual(payload["warning_status"], "unknown")
        self.assertNotIn("官方暂无秦托邦天气预警", payload["morning_broadcast"])
        self.assertIn("空气（鄠邑区）：AQI 暂未确认", payload["morning_broadcast"])

    def test_open_meteo_fallback_report_includes_unknown_aqi_line(self):
        original_urlopen = self.module.urlrequest.urlopen

        class _FakeHTTPResponse:
            def __init__(self, payload: dict[str, object]):
                self._payload = json.dumps(payload).encode("utf-8")
                self.headers = {}

            def __enter__(self):
                return self

            def __exit__(self, exc_type, exc_val, exc_tb):
                return None

            def read(self, _size: int = -1):
                return self._payload

        try:
            self.module.urlrequest.urlopen = lambda request, timeout=None: _FakeHTTPResponse(
                {
                    "current": {
                        "time": "2026-07-06T08:00",
                        "temperature_2m": 24,
                        "relative_humidity_2m": 72,
                        "apparent_temperature": 25,
                        "wind_speed_10m": 8,
                    },
                    "hourly": {
                        "time": [
                            "2026-07-06T08:00",
                            "2026-07-06T09:00",
                            "2026-07-06T10:00",
                        ],
                        "precipitation_probability": [20, 65, 80],
                        "precipitation": [0.0, 1.2, 0.8],
                    },
                }
            )

            payload = self.module._open_meteo_fallback()
        finally:
            self.module.urlrequest.urlopen = original_urlopen

        self.assertEqual(payload["warning_status"], "unknown")
        self.assertIn("空气（鄠邑区）：AQI 暂未确认", payload["morning_broadcast"])

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
        self.module._qintopia_weather_now_iso = (
            lambda: "2026-06-28T08:00:00+08:00"
        )

        payload = json.loads(
            self.module.handle_qintopia_weather_lookup({"intent": "umbrella", "hours": 24})
        )

        self.assertTrue(payload["success"])
        self.assertEqual(payload["source"], "qweather_mcp")
        self.assertEqual(payload["location"]["name"], "秦托邦·栗峪口")
        self.assertEqual(payload["location"]["coordinates"], "108.5666545,34.0261288")
        self.assertEqual(payload["current"]["text"], "多云")
        self.assertEqual(payload["warnings"][0]["type"], "雷雨大风")
        self.assertEqual(payload["warning_status"], "present")
        self.assertEqual(payload["daily_forecast"]["warning_status"], "present")
        self.assertIn("雷雨大风黄色预警", payload["daily_forecast"]["warning_copy"])
        self.assertTrue(payload["morning_broadcast"].startswith("秦托邦今日天气："))
        self.assertIn("降水/带伞：", payload["morning_broadcast"])
        self.assertIn("预警：雷雨大风黄色预警", payload["morning_broadcast"])
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
                },
                {
                    "start": "2026-06-28T11:00+08:00",
                    "end": "2026-06-28T11:00+08:00",
                    "reason": "小雨、降水概率最高60%、降水最高0.5mm",
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
        self.assertEqual(call_args["get_weather_now"]["location"], "108.5666545,34.0261288")
        self.assertEqual(
            call_args["get_hourly_weather"]["location"], "108.5666545,34.0261288"
        )
        self.assertEqual(
            call_args["get_minutely_5m"]["location"], "108.5666545,34.0261288"
        )
        self.assertEqual(call_args["get_air_quality"]["city"], "鄠邑区")

    def test_weather_lookup_warning_sorting_prefers_red_orange_for_high_priority_and_counts_total(self):
        fixture = self._load_weather_fixture()
        fixture["bundle"]["weather_alert"]["data"]["alerts"] = [
            {
                "id": "blue-a",
                "senderName": "西安市气象台",
                "headline": "西安市气象台发布雷雨大风蓝色预警",
                "eventType": {"name": "雷雨大风"},
                "color": {"code": "Blue"},
                "messageType": {"code": "Alert"},
                "effectiveTime": "2026-07-06T06:00+08:00",
            },
            {
                "id": "yellow-a",
                "senderName": "鄠邑区气象台",
                "headline": "鄠邑区气象台发布大雾黄色预警",
                "eventType": {"name": "大雾"},
                "color": {"code": "Yellow"},
                "messageType": {"code": "Alert"},
                "effectiveTime": "2026-07-06T06:10+08:00",
            },
            {
                "id": "blue-b",
                "senderName": "商洛市气象台",
                "headline": "商洛市气象台发布大风蓝色预警",
                "eventType": {"name": "大风"},
                "color": {"code": "Blue"},
                "messageType": {"code": "Alert"},
                "effectiveTime": "2026-07-06T06:20+08:00",
            },
            {
                "id": "blue-c",
                "senderName": "汉中市气象台",
                "headline": "汉中市气象台发布大雾蓝色预警",
                "eventType": {"name": "大雾"},
                "color": {"code": "Blue"},
                "messageType": {"code": "Alert"},
                "effectiveTime": "2026-07-06T06:30+08:00",
            },
            {
                "id": "yellow-b",
                "senderName": "宝鸡市气象台",
                "headline": "宝鸡市气象台发布霜冻黄色预警",
                "eventType": {"name": "霜冻"},
                "color": {"code": "Yellow"},
                "messageType": {"code": "Alert"},
                "effectiveTime": "2026-07-06T06:40+08:00",
            },
            {
                "id": "red",
                "senderName": "铜川市气象台",
                "headline": "铜川市气象台发布高温红色预警",
                "eventType": {"name": "高温"},
                "color": {"code": "Red"},
                "messageType": {"code": "Alert"},
                "effectiveTime": "2026-07-06T06:50+08:00",
            },
            {
                "id": "orange",
                "senderName": "延安市气象台",
                "headline": "延安市气象台发布高温橙色预警",
                "eventType": {"name": "高温"},
                "color": {"code": "Orange"},
                "messageType": {"code": "Alert"},
                "effectiveTime": "2026-07-06T07:00+08:00",
            },
        ]
        self._stub_qweather(fixture, fixture["bundle"]["weather_alert"]["data"]["alerts"])

        payload = json.loads(self.module.handle_qintopia_weather_lookup({"intent": "warning"}))
        warning_lines = [
            line for line in self._extract_non_empty_broadcast_lines(payload) if line.startswith("预警：")
        ]
        self.assertTrue(warning_lines)
        self.assertIn("红色", warning_lines[0])
        self.assertIn("橙色", warning_lines[0])
        self.assertIn("另有 5 条", warning_lines[0])

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

    def test_weather_lookup_partial_data_reports_unknown_aqi_line(self):
        fixture = self._load_weather_fixture()
        partial_bundle = self._qweather_fixture_call_bundle(fixture)
        partial_bundle["hourly"]["data"]["hourly"] = partial_bundle["hourly"]["data"]["hourly"][:5]
        partial_bundle["air_quality"] = {"success": False, "error": "aqi unavailable"}

        self._stub_qweather(
            {
                "generated_at": fixture["generated_at"],
                "bundle": partial_bundle,
            },
            [],
        )

        payload = json.loads(self.module.handle_qintopia_weather_lookup({"intent": "general"}))
        lines = self._extract_non_empty_broadcast_lines(payload)

        self.assertIn("空气（鄠邑区）：AQI 暂未确认", lines)
        self.assertIsNone(payload["air_quality"])
        self.assertEqual(payload["warning_status"], "none")

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
                "空气（鄠邑区）：AQI 暂未确认\n"
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
        self.assertIn("空气（鄠邑区）：AQI 暂未确认", payload["morning_broadcast"])
        self.assertNotIn("官方暂无秦托邦天气预警", payload["morning_broadcast"])

    def test_weather_lookup_rejects_non_fixed_location(self):
        os.environ["QINTOPIA_WEATHER_LOCATION"] = "西安"

        payload = json.loads(self.module.handle_qintopia_weather_lookup({}))

        self.assertFalse(payload["success"])
        self.assertTrue(payload["guardrails"]["fixed_location_only"])
        self.assertIn("fixed lon,lat", payload["error"])


if __name__ == "__main__":
    unittest.main()
