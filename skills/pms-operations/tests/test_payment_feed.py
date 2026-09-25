import importlib.util
import json
import os
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("pms_payment_feed", Path(__file__).parents[1] / "payment_feed.py")
feed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(feed)


def context(state):
    return {"schemaVersion": "pms.payments.v1", "sourceInstance": "synthetic-pms",
            "propertyId": "property_a", "bindingVersion": 3, "state": state}


class FeedTests(unittest.TestCase):
    def test_nonzero_head_is_sent_once_and_first_page_starts_at_durable_cursor(self):
        pms = Mock()
        pms.payment_head.return_value = {"schemaVersion": "pms.payments.v1", "propertyId": "property_a", "headCursor": "42"}
        pms.read.return_value = {"schemaVersion": "pms.payments.v1", "propertyId": "property_a", "events": [], "nextCursor": "42"}
        host = Mock(side_effect=[context(None), {}, context({"cursor": "42"}), {"cursor": "42"}])
        self.assertTrue(feed.synchronize(pms, host)["caught_up"])
        self.assertEqual(host.call_args_list[1].args[0]["head"], {**pms.payment_head.return_value,
            "sourceInstance": "synthetic-pms", "bindingVersion": 3})
        self.assertEqual(pms.read.call_args.kwargs["filters"]["cursor"], "42")

    def test_restart_after_lost_open_ack_does_not_sample_or_replace_baseline(self):
        pms = Mock()
        pms.read.return_value = {"events": [], "nextCursor": "44"}
        host = Mock(side_effect=[context({"cursor": "44"}), {}, context({"cursor": "44"}), {"cursor": "44"}])
        self.assertEqual(feed.synchronize(pms, host)["cursor"], "44")
        pms.payment_head.assert_not_called()
        self.assertEqual(host.call_args_list[1].args[0], {"action": "open"})

    def test_wrong_property_and_unknown_page_ack_stop_without_retry(self):
        pms = Mock()
        pms.payment_head.return_value = {"schemaVersion": "pms.payments.v1", "propertyId": "other", "headCursor": "99"}
        host = Mock(return_value=context(None))
        with self.assertRaisesRegex(ValueError, "payment_source_mismatch"):
            feed.synchronize(pms, host)
        self.assertEqual(host.call_count, 1)
        pms.read.assert_not_called()
        pms.read.return_value = {"events": [{"sequence": "43"}], "nextCursor": "43"}
        host = Mock(side_effect=[context({"cursor": "42"}), {}, context({"cursor": "42"}), ValueError("lost_ack")])
        with self.assertRaisesRegex(ValueError, "lost_ack"):
            feed.synchronize(pms, host)
        self.assertEqual(pms.read.call_count, 1)

    def test_production_host_uses_private_credentials_and_fixed_https_origin(self):
        values = {"GREENPMS_API_TOKEN": "simulated-pms-secret-1234567890",
                  "QINTOPIA_FOUNDATION_HOST_TOKEN": "simulated-host-secret-12345678901234567890"}
        plugin = Mock()
        plugin.client.PRODUCTION_ORIGIN = "https://pms.qintopia.cn"
        plugin.client.MAX_BYTES = 262144
        plugin.credentials.load.return_value = values
        hermes = types.ModuleType("hermes_constants")
        hermes.get_hermes_home = lambda: "/private/profile"
        with patch.dict(sys.modules, {"hermes_constants": hermes}), patch.dict(os.environ, {
                "QINTOPIA_PMS_CREDENTIALS_FILE": "/private/payment.json",
                "QINTOPIA_FOUNDATION_SOCKET": "/private/foundation.sock",
                "GREENPMS_BASE_URL": "http://127.0.0.1:54321",
            }, clear=True), patch.object(feed.Path, "stat") as path_stat:
            path_stat.return_value.st_mode = 0o140600
            path_stat.return_value.st_uid = os.getuid()
            pms, transport = feed._production_host(plugin)
        plugin.credentials.load.assert_called_once_with("/private/payment.json", profile_home="/private/profile")
        plugin.client.Client.assert_called_once_with("https://pms.qintopia.cn", values["GREENPMS_API_TOKEN"],
                                                     production_enabled=True)
        self.assertIs(pms, plugin.client.Client.return_value)
        connection = Mock()
        connection.__enter__ = Mock(return_value=connection)
        connection.__exit__ = Mock(return_value=False)
        connection.recv.return_value = b'{"ok":true,"result":{"cursor":"42"}}\n'
        plugin.client.decode.return_value = {"ok": True, "result": {"cursor": "42"}}
        with patch.object(feed.socket, "socket", return_value=connection):
            self.assertEqual(transport({"action": "context"})["result"]["cursor"], "42")
        payload = json.loads(connection.sendall.call_args.args[0])
        self.assertEqual(payload, {"action": "context", "token": values["QINTOPIA_FOUNDATION_HOST_TOKEN"]})
        plugin.client.decode.assert_called_once_with(connection.recv.return_value)

    def test_production_host_rejects_missing_private_socket_before_client_creation(self):
        plugin = Mock()
        plugin.credentials.load.return_value = {"GREENPMS_API_TOKEN": "simulated-secret",
                                                "QINTOPIA_FOUNDATION_HOST_TOKEN": "simulated-host-token"}
        hermes = types.ModuleType("hermes_constants")
        hermes.get_hermes_home = lambda: "/private/profile"
        with patch.dict(sys.modules, {"hermes_constants": hermes}), patch.dict(os.environ, {
                "QINTOPIA_PMS_CREDENTIALS_FILE": "/private/payment.json",
                "QINTOPIA_FOUNDATION_SOCKET": "relative.sock",
            }, clear=True):
            with self.assertRaisesRegex(ValueError, "foundation_unavailable"):
                feed._production_host(plugin)
        plugin.client.Client.assert_not_called()

    def test_production_host_denies_before_opening_credentials_or_connecting(self):
        plugin = Mock()
        plugin.production.require.side_effect = ValueError("pms_isolation_unavailable")
        with patch.object(feed.socket, "socket") as connect:
            with self.assertRaisesRegex(ValueError, "pms_isolation_unavailable"):
                feed._production_host(plugin)
        plugin.credentials.load.assert_not_called()
        plugin.client.Client.assert_not_called()
        connect.assert_not_called()
