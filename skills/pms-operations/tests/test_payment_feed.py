import importlib.util
from pathlib import Path
import unittest
from unittest.mock import Mock

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
