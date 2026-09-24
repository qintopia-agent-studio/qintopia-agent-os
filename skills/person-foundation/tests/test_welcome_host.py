"""Crash/uncertain-result ordering for the explicit simulated host."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("welcome_host", Path(__file__).parents[1] / "welcome_host.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Broker:
    def __init__(self):
        self.status = "pending"
        self.calls = []
        self.lose_receipt = False
        self.approved = False

    def __call__(self, request):
        self.calls.append(request["action"])
        action = request["action"]
        if action == "prepare":
            return {"presentation": "p", "status": self.status}
        if action == "claim":
            if self.status != "pending":
                return {"send": False, "status": self.status}
            self.status = "claimed"
            return {"send": True, "claim": "c", "chat_id": "fixed-simulated-group", "text": "模拟核对", "artifacts": []}
        if action == "receipt":
            if self.lose_receipt:
                raise ConnectionError("lost acknowledgement")
            self.status = request["outcome"]
        if action == "callback":
            self.approved = True
        return {"status": self.status}


class WelcomeHostTests(unittest.TestCase):
    def test_claim_precedes_transport_and_confirmation_is_separate(self):
        broker, transport = Broker(), module.SimulatedTransport()
        host = module.WelcomeHost(broker, transport, local_enabled=True)
        self.assertEqual(host.deliver("work")["status"], "delivered")
        self.assertEqual(broker.calls, ["prepare", "claim", "receipt"])
        self.assertEqual(len(transport.sent), 1)
        self.assertFalse(broker.approved)
        host.callback()
        self.assertTrue(broker.approved)
        host.deliver("work")
        self.assertEqual(len(transport.sent), 1)

    def test_lost_receipt_recovers_original_without_resend(self):
        broker, transport = Broker(), module.SimulatedTransport()
        broker.lose_receipt = True
        host = module.WelcomeHost(broker, transport, local_enabled=True)
        self.assertEqual(host.deliver("work")["status"], "unknown")
        broker.status = "unknown"  # server claim expiry
        host.deliver("work")
        self.assertEqual(len(transport.sent), 1)
        broker.lose_receipt = False
        self.assertEqual(host.recover("p", "c")["status"], "delivered")
        self.assertEqual(len(transport.sent), 1)

    def test_restart_without_transport_receipt_stays_unknown(self):
        broker = Broker()
        broker.status = "unknown"
        host = module.WelcomeHost(broker, module.SimulatedTransport(), local_enabled=True)
        self.assertEqual(host.recover("p", "c")["status"], "unknown")
        self.assertNotIn("receipt", broker.calls)

    def test_no_implicit_or_arbitrary_external_transport(self):
        for transport, enabled in [(module.SimulatedTransport(), False), (object(), True)]:
            with self.assertRaises(ValueError):
                module.WelcomeHost(Broker(), transport, local_enabled=enabled)


if __name__ == "__main__":
    unittest.main()
