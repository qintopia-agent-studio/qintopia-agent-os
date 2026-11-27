import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import sensors
import service
import subscription


class EnvironmentalQuery(unittest.TestCase):
    def setUp(self):
        self.devices = [
            dict(name="壁炉温湿度计", area="一楼壁炉区", device_id="demo-one", profile="climate"),
            dict(name="活动区温湿度计", area="3楼活动区", device_id="demo-three", profile="climate"),
            dict(name="烟雾报警器", area="一楼壁炉区", device_id="demo-alarm", profile="smoke-co-alarm"),
        ]
        self.profiles = {
            "climate": {
                "temp_current": {"label": "温度", "scale": 1, "unit_from": {
                    "code": "temp_unit_convert", "values": {"c": "°C"}}},
                "humidity_value": {"label": "湿度", "scale": 1, "unit": "%"},
            },
            "smoke-co-alarm": {
                "smoke_sensor_state": {"label": "烟雾报警状态"},
                "co_state": {"label": "一氧化碳报警状态"},
            },
        }
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.registry = Path(self.temp.name) / "devices.json"
        self.save_registry()
        env = patch.dict(os.environ, {"TUYA_DEVICES_FILE": str(self.registry)})
        env.start()
        self.addCleanup(env.stop)
        self.reader = object.__new__(service.Reader)
        self.cloud = Mock()
        self.reader.read = lambda device, profiles: sensors.read_device(self.cloud, device, profiles)
        self.at = 1_795_775_921_000

    def save_registry(self):
        self.registry.write_text(json.dumps({"devices": self.devices, "profiles": self.profiles}))

    def response(self, readings):
        return {"t": self.at + 20_000, "result": {"properties": [
            {"code": k, "value": v, "time": self.at + offset} for k, v, offset in readings]}}

    def test_area_query_returns_climate_and_both_independent_alarm_states(self):
        self.cloud.get.side_effect = [
            self.response([("temp_current", 215, 0), ("humidity_value", 467, 1000),
                           ("temp_unit_convert", "c", 0)]),
            self.response([("smoke_sensor_state", "normal", 2000), ("co_state", "alarm", 3000)]),
        ]
        result = self.reader.handle({"op": "read", "target": "1楼壁炉区"})
        self.assertTrue(result["ok"])
        climate, detector = result["devices"]
        self.assertEqual([(r["value"], r["unit"]) for r in climate["readings"]], [(21.5, "°C"), (46.7, "%")])
        smoke, co = detector["readings"]
        self.assertEqual((smoke["value"], co["value"]), ("normal", "alarm"))
        self.assertNotEqual(smoke["reported_at"], co["reported_at"])
        self.assertNotEqual(co["reported_at"], detector["queried_at"])
        self.assertIn("codes=co_state,smoke_sensor_state", self.cloud.get.call_args.args[0])

    def test_device_name_list_and_partial_failure(self):
        self.assertEqual(len(self.reader.handle({"op": "list"})["devices"]), 3)
        self.cloud.get.side_effect = [sensors.QueryError("模拟设备不可用"),
                                     self.response([("smoke_sensor_state", "normal", 0)])]
        first, second = self.reader.handle({"op": "read", "target": "一楼壁炉区"})["devices"]
        self.assertFalse(first["ok"])
        self.assertEqual([r["status"] for r in second["readings"]], ["ok", "not_reported"])
        devices, _ = sensors.load_registry()
        self.assertEqual(sensors.select_devices(devices, "烟雾报警器"), [self.devices[-1]])
        self.assertEqual(sensors.select_devices(devices, "三楼活动区"), [self.devices[1]])
        self.assertEqual(sensors.select_devices(devices, None), self.devices)

    def test_subscription_fails_closed_on_ambiguous_registered_detector(self):
        self.assertEqual(subscription.fireplace_id(), "demo-alarm")
        self.devices.append(dict(self.devices[-1], name="第二个报警器", device_id="demo-another"))
        self.save_registry()
        with self.assertRaisesRegex(ValueError, "exactly one"):
            subscription.fireplace_id()

    def test_read_only_socket_rejects_other_operations(self):
        with self.assertRaises(sensors.QueryError):
            self.reader.handle({"op": "send", "target": "烟雾报警器"})
        self.cloud.get.assert_not_called()
