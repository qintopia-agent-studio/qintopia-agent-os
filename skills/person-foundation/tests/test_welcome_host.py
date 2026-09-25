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
        if action == "confirmation_context":
            return {"requires_contacts": False, "work_item": None, "presentation": "p", "replayed": False}
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

    def test_phone_refresh_uses_same_callback_and_does_not_confirm_on_failure(self):
        calls = []
        def broker(request):
            calls.append(request["action"])
            if request["action"] == "confirmation_context":
                return {"requires_contacts": True, "work_item": "application-work", "presentation": "original"}
            return {"confirmed": True}
        def refresh(work, presentation):
            self.assertEqual((work, presentation), ("application-work", "original"))
            calls.append("actual-read")
            return {"status": "complete", "scan_complete": True}
        host = module.WelcomeHost(broker, module.SimulatedTransport(), local_enabled=True)
        self.assertTrue(host.callback(refresh_contacts=refresh)["confirmed"])
        self.assertEqual(calls, ["confirmation_context", "actual-read", "callback"])
        calls.clear()
        with self.assertRaises(ValueError):
            host.callback(refresh_contacts=lambda *_: {"status": "incomplete", "scan_complete": False})
        self.assertEqual(calls, ["confirmation_context"])

    def test_replayed_or_independent_confirmation_needs_no_contact_function(self):
        for replayed in [True, False]:
            calls = []
            def broker(request):
                calls.append(request["action"])
                return {"requires_contacts": False, "replayed": replayed}
            host = module.WelcomeHost(broker, module.SimulatedTransport(), local_enabled=True)
            host.callback(refresh_contacts=lambda *_: self.fail("must not refresh"))
            self.assertEqual(calls, ["confirmation_context", "callback"])

    def test_no_implicit_or_arbitrary_external_transport(self):
        for transport, enabled in [(module.SimulatedTransport(), False), (object(), True)]:
            with self.assertRaises(ValueError):
                module.WelcomeHost(Broker(), transport, local_enabled=enabled)


if __name__ == "__main__":
    unittest.main()
