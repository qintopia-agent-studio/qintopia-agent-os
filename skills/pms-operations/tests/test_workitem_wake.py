"""Dependency-free contract tests for the read-only WorkItem webhook boundary."""
from contextlib import contextmanager
from contextvars import ContextVar
import importlib.util
import os
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "pms_workitem_wake_unit", Path(__file__).resolve().parents[1] / "workitem_wake.py")
wake = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wake)
WORK = "ca371c22-2d18-4cbe-9e09-5fe3ee9614f0"
BINDING = "23b46446-aa4a-43ee-8cb3-56f97ecbd305"
BROKER_RESULT = {"work_item_id": WORK, "kind": "payment", "status": "awaiting_review",
                 "readback_required": True, "contact_status": "pending_verified_channel",
                 "requires_human_confirmation": True}


class Production:
    def __init__(self):
        self.calls = 0

    def mode(self):
        return "production"

    def require(self):
        self.calls += 1


def event(**changes):
    source = types.SimpleNamespace(platform="webhook", chat_type="webhook",
        user_id=wake.USER_ID, chat_id=wake.CHAT_PREFIX + WORK, profile=None,
        message_id=None, is_bot=False, profile_route_rejected=False)
    item = types.SimpleNamespace(source=source, internal=False, message_type="text",
        raw_message={"schema_version": 1, "work_item_id": WORK}, message_id=WORK,
        text="/restart", allow_gateway_control=True, prompt_response=None,
        reply_to_message_id=None, reply_to_text=None, reply_to_author_id=None,
        reply_to_author_name=None, reply_to_is_own_message=False,
        ledger_message_id=None, reply_anchor_override=None, media_urls=[],
        media_types=[], media_text_inlined=[], auto_skill=None, channel_prompt=None,
        channel_context=None, metadata={})
    for key, value in changes.items():
        target, attr = (source, key[7:]) if key.startswith("source_") else (item, key)
        setattr(target, attr, value)
    return item


@contextmanager
def session(**changes):
    values = {"platform": "webhook", "chat_type": "webhook", "user_id": wake.USER_ID,
              "chat_id": wake.CHAT_PREFIX + WORK, "message_id": WORK, "profile": "anan"}
    values.update(changes)
    gateway = types.ModuleType("gateway")
    context = types.ModuleType("gateway.session_context")
    context._VAR_MAP = {"HERMES_SESSION_" + key.upper(): ContextVar(key)
                        for key in values}
    gateway.session_context = context
    tokens = [(context._VAR_MAP["HERMES_SESSION_" + key.upper()], value)
              for key, value in values.items()]
    with patch.dict(sys.modules, {"gateway": gateway, "gateway.session_context": context}):
        bound = [(var, var.set(value)) for var, value in tokens]
        try:
            yield
        finally:
            for var, token in reversed(bound):
                var.reset(token)


class WorkItemWakeUnitTests(unittest.TestCase):
    def setUp(self):
        self.production = Production()

    def test_exact_body_and_source_are_normalized_after_validation(self):
        item = event()
        with patch.object(wake, "available", return_value=True):
            self.assertIsNone(wake.inbound(item, production=self.production))
        self.assertEqual(self.production.calls, 1)
        self.assertEqual(item.source.profile, "anan")
        self.assertEqual(item.source.message_id, WORK)
        self.assertFalse(item.allow_gateway_control)
        self.assertEqual(item.text, wake.PROMPT)
        self.assertIsNone(item.raw_message)

    def test_other_route_profile_control_and_payload_are_dropped(self):
        changes = ({"source_user_id": "webhook:other"},
                   {"source_profile": "erhua"}, {"source_profile": "default"},
                   {"source_chat_id": "webhook:other:" + WORK},
                   {"message_id": "other"},
                   {"raw_message": {"schema_version": True, "work_item_id": WORK}},
                   {"raw_message": {"schema_version": 1, "work_item_id": WORK, "text": "send"}},
                   {"raw_message": {"schema_version": 1, "work_item_id": WORK.upper()}},
                   {"media_urls": ["/private"]}, {"prompt_response": {"option_id": "yes"}},
                   {"metadata": {"command": "restart"}})
        with patch.object(wake, "available", return_value=True):
            for change in changes:
                with self.subTest(change=change):
                    item = event(**change)
                    self.assertEqual(wake.inbound(item, production=self.production), wake.SKIP)
                    self.assertIsNone(item.source.message_id)
                    self.assertFalse(item.allow_gateway_control)
                    self.assertEqual(item.text, wake.PROMPT)
        self.assertEqual(self.production.calls, 0)

    def test_bound_context_limits_tools_and_host_read(self):
        seen = []
        def broker(request, **kwargs):
            seen.append((request, kwargs))
            return {"ok": True, "result": BROKER_RESULT}
        env = {"QINTOPIA_PMS_EVENT_BINDING": BINDING,
               "QINTOPIA_FOUNDATION_GATEWAY_ID": "simulated-gateway"}
        with patch.dict(os.environ, env), patch.object(wake, "available", return_value=True), session():
            self.assertIsNone(wake.guard(wake.TOOL, {}, production=self.production))
            for tool in ("terminal", "qintopia_pms_execute", "tool_call", "send_message"):
                self.assertEqual(wake.guard(tool, {}, production=self.production), wake.DENIED)
            self.assertEqual(wake.guard(wake.TOOL, {"work_item": WORK},
                                        production=self.production), wake.DENIED)
            self.assertEqual(wake.read({}, transport=broker, production=self.production),
                             {"ok": True, "result": BROKER_RESULT})
        self.assertEqual(seen, [({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_workitem_read", "trusted_context": {
                "gateway_id": "simulated-gateway", "platform": "host", "chat_type": "",
                "chat_id": "", "sender_id": "", "message_id": ""},
            "arguments": {"binding": BINDING, "work_item": WORK}}, {"host": True})])

    def test_unbound_or_changed_context_never_reaches_broker(self):
        calls = []
        def broker(*args, **kwargs):
            calls.append((args, kwargs))
            return {"ok": True, "result": BROKER_RESULT}
        with patch.object(wake, "available", return_value=True):
            self.assertFalse(wake.read({}, transport=broker, production=self.production)["ok"])
            for changed in ({"profile": "erhua"}, {"message_id": ""},
                            {"chat_id": "webhook:other:" + WORK}):
                with session(**changed):
                    self.assertEqual(wake.guard(wake.TOOL, {}, production=self.production),
                                     wake.DENIED)
                    self.assertFalse(wake.read({}, transport=broker,
                                               production=self.production)["ok"])
        self.assertEqual(calls, [])

    def test_broker_projection_requires_six_matching_fields(self):
        self.assertEqual(wake._projection({**BROKER_RESULT, "phone": "private",
                                           "amount": 999}, WORK), BROKER_RESULT)
        for result in ({**BROKER_RESULT, "work_item_id": "other"},
                       {**BROKER_RESULT, "kind": "application"},
                       {**BROKER_RESULT, "requires_human_confirmation": None},
                       {key: value for key, value in BROKER_RESULT.items()
                        if key != "contact_status"},
                       {"id": WORK, "kind": "payment", "status": "awaiting_review"}):
            with self.subTest(result=result), self.assertRaises(ValueError):
                wake._projection(result, WORK)

    def test_default_route_secret_must_be_private_and_exclusive(self):
        secret = "simulated-private-route-key-" + "x" * 24
        config = {"platforms": {"webhook": {"extra": {"host": "127.0.0.1", "routes": {
            wake.ROUTE: {"deliver": "log", "secret": secret}}}}}}
        managed = types.ModuleType("hermes_cli.managed_scope")
        managed.get_managed_dir = lambda: Path("/simulated/managed")
        managed.load_managed_config = lambda: config
        managed.load_managed_env = lambda: {}
        hermes_cli = types.ModuleType("hermes_cli")
        hermes_cli.managed_scope = managed
        constants = types.ModuleType("hermes_constants")
        constants.get_hermes_home = lambda: Path("/simulated/profile")
        secret_scope = types.ModuleType("agent.secret_scope")
        secret_scope.load_env_file = lambda path: {}
        agent = types.ModuleType("agent")
        agent.secret_scope = secret_scope
        modules = {"hermes_cli": hermes_cli, "hermes_cli.managed_scope": managed,
                   "hermes_constants": constants, "agent": agent,
                   "agent.secret_scope": secret_scope}
        with patch.dict(os.environ, {}, clear=True), patch.dict(sys.modules, modules):
            self.assertTrue(wake._private_route_ready())
            route = config["platforms"]["webhook"]["extra"]["routes"][wake.ROUTE]
            route["profile"] = "anan"
            self.assertFalse(wake._private_route_ready())
            del route["profile"]
            config["platforms"]["webhook"]["extra"]["secret"] = secret
            self.assertFalse(wake._private_route_ready())
            del config["platforms"]["webhook"]["extra"]["secret"]
            with patch.dict(os.environ, {"ANAN_WORKITEM_WEBHOOK_SECRET": secret}):
                self.assertFalse(wake._private_route_ready())

    def test_wake_enable_requires_active_anan_profile(self):
        profiles = types.ModuleType("hermes_cli.profiles")
        profiles.get_active_profile_name = lambda: "erhua"
        hermes_cli = types.ModuleType("hermes_cli")
        hermes_cli.profiles = profiles
        with patch.dict(sys.modules, {"hermes_cli": hermes_cli,
                                      "hermes_cli.profiles": profiles}), \
                patch.dict(os.environ, {"QINTOPIA_PMS_WORKITEM_WAKE_ENABLE": "1"}), \
                patch.object(wake, "_private_route_ready", return_value=True), \
                patch.object(wake, "_tool_search_off", return_value=True):
            self.assertFalse(wake.available(self.production))
            profiles.get_active_profile_name = lambda: "anan"
            self.assertTrue(wake.available(self.production))


if __name__ == "__main__":
    unittest.main()
