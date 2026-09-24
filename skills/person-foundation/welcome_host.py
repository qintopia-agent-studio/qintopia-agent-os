"""Explicit local welcome presentation host; no production transport or credentials.

The broker callable is injected by the Anan host, never a model tool. It must use
person_foundation_ingress / welcome_group_host and the independent HOST_TOKEN.
"""
from __future__ import annotations


class SimulatedTransport:
    """Only an in-memory transport is supplied in this local completion slice."""

    def __init__(self):
        self.sent = {}

    def send(self, request):
        claim = request["claim"]
        if claim in self.sent:
            raise ValueError("simulation_duplicate_send")
        self.sent[claim] = request
        return "simulated:" + claim

    def readback(self, claim):
        return "simulated:" + claim if claim in self.sent else None


class WelcomeHost:
    def __init__(self, broker, transport, *, local_enabled=False):
        if not local_enabled or type(transport) is not SimulatedTransport:
            raise ValueError("explicit_local_simulation_required")
        self.broker = broker
        self.transport = transport

    def deliver(self, work_item):
        prepared = self.broker({"action": "prepare", "work_item": work_item})
        presentation = prepared["presentation"]
        claim = self.broker({"action": "claim", "presentation": presentation})
        if not claim.get("send"):
            return claim
        # The persistent claim exists before invoking transport. A crash after this
        # point becomes UNKNOWN in the store; a later invocation cannot resend it.
        try:
            receipt = self.transport.send(claim)
        except Exception:
            return self.broker({"action": "receipt", "presentation": presentation,
                                "claim": claim["claim"], "outcome": "unknown", "receipt": None})
        try:
            return self.broker({"action": "receipt", "presentation": presentation,
                                "claim": claim["claim"], "outcome": "delivered", "receipt": receipt})
        except Exception:
            # Do not turn a lost persistence acknowledgement into a second send.
            return {"presentation": presentation, "claim": claim["claim"],
                    "status": "unknown", "send": False}

    def recover(self, presentation, claim):
        status = self.broker({"action": "status", "presentation": presentation})
        if status["status"] == "delivered":
            return status
        receipt = self.transport.readback(claim)
        if receipt is None:
            return status
        return self.broker({"action": "receipt", "presentation": presentation,
                            "claim": claim, "outcome": "delivered", "receipt": receipt})

    def callback(self):
        """The broker derives the sender and text from persisted authenticated input."""
        return self.broker({"action": "callback"})
