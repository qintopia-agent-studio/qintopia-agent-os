from __future__ import annotations

import importlib.util
import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path


def load_broadcast_cli():
    script_path = (
        Path(__file__).resolve().parents[1]
        / "scripts"
        / "qintopia-erhua-weather-broadcast.py"
    )
    spec = importlib.util.spec_from_file_location(
        "qintopia_erhua_weather_broadcast", script_path
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def canonical_payload() -> dict:
    broadcast = (
        "2026-07-17 秦托邦·栗峪口今日天气\n"
        "全天：午后有降水窗口，出门带伞。\n"
        "分时：中午多云24-27°C；下午阵雨25-28°C；晚上阴22-25°C\n"
        "预警：官方预警数据暂未确认\n"
        "今早参考：多云23°C，体感24°C\n"
        "空气（鄠邑区）：AQI 52，良\n"
        "二花提醒：下午留意雨势，播报完毕～"
    )
    return {
        "success": True,
        "current": {"text": "多云", "temp_c": "23"},
        "daily_forecast": {
            "forecast_date": "2026-07-17",
            "status": "complete",
            "periods": [
                {"id": "midday", "status": "complete"},
                {"id": "afternoon", "status": "complete"},
                {"id": "evening", "status": "complete"},
            ],
        },
        "morning_broadcast": broadcast,
    }


class ErhuaWeatherBroadcastCliTest(unittest.TestCase):
    def setUp(self) -> None:
        self.module = load_broadcast_cli()

    def test_extracts_canonical_morning_broadcast_without_re_rendering(self):
        payload = canonical_payload()

        result = self.module.extract_morning_broadcast(
            json.dumps(payload, ensure_ascii=False)
        )

        self.assertEqual(result, payload["morning_broadcast"])
        self.assertNotEqual(result, payload["current"]["text"])

    def test_main_uses_fixed_whole_day_arguments_and_prints_only_broadcast(self):
        payload = canonical_payload()
        received_arguments = []

        def handler(arguments):
            received_arguments.append(arguments)
            return json.dumps(payload, ensure_ascii=False)

        self.module._load_weather_handler = lambda: handler
        stdout = io.StringIO()
        stderr = io.StringIO()

        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = self.module.main()

        self.assertEqual(exit_code, 0)
        self.assertEqual(received_arguments, [{"intent": "general", "hours": 24}])
        self.assertEqual(stdout.getvalue(), payload["morning_broadcast"] + "\n")
        self.assertEqual(stderr.getvalue(), "")

    def test_rejects_current_only_output_even_when_current_data_exists(self):
        for current_only in [
            "现在：多云，约23°C，体感24°C",
            "现在天气多云，约23°C，体感24°C",
        ]:
            payload = canonical_payload()
            payload["morning_broadcast"] = current_only

            with self.subTest(current_only=current_only):
                with self.assertRaisesRegex(
                    self.module.BroadcastContractError, "current-first"
                ):
                    self.module.extract_morning_broadcast(
                        json.dumps(payload, ensure_ascii=False)
                    )

    def test_rejects_payload_without_fixed_day_periods(self):
        payload = canonical_payload()
        payload["daily_forecast"]["periods"] = [{"id": "midday"}]

        with self.assertRaisesRegex(
            self.module.BroadcastContractError, "periods do not match"
        ):
            self.module.extract_morning_broadcast(
                json.dumps(payload, ensure_ascii=False)
            )

    def test_failure_keeps_stdout_empty(self):
        current_only = canonical_payload()
        current_only["morning_broadcast"] = "现在天气晴，约23°C"
        cases = [
            ({"success": False, "current": {"text": "晴"}}, "did not report success"),
            (current_only, "current-first"),
        ]

        for payload, expected_error in cases:
            with self.subTest(expected_error=expected_error):
                self.module._load_weather_handler = lambda: (
                    lambda arguments: json.dumps(payload, ensure_ascii=False)
                )
                stdout = io.StringIO()
                stderr = io.StringIO()

                with redirect_stdout(stdout), redirect_stderr(stderr):
                    exit_code = self.module.main()

                self.assertEqual(exit_code, 1)
                self.assertEqual(stdout.getvalue(), "")
                self.assertIn(expected_error, stderr.getvalue())

    def test_rejects_malformed_json_missing_broadcast_and_overlong_copy(self):
        cases = [
            ("{", "malformed JSON"),
            (
                json.dumps(
                    {
                        "success": True,
                        "daily_forecast": {"periods": []},
                    }
                ),
                "morning_broadcast is missing",
            ),
        ]
        overlong = canonical_payload()
        overlong["morning_broadcast"] = "\n".join(
            ["2026-07-17 秦托邦今日天气", "分时：中午、下午、晚上"]
            + [f"补充{i}" for i in range(7)]
        )
        cases.append((json.dumps(overlong, ensure_ascii=False), "eight-line"))

        for raw_payload, expected_error in cases:
            with self.subTest(expected_error=expected_error):
                with self.assertRaisesRegex(
                    self.module.BroadcastContractError, expected_error
                ):
                    self.module.extract_morning_broadcast(raw_payload)


if __name__ == "__main__":
    unittest.main()
