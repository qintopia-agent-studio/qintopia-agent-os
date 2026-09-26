"""Official-hook regression for the fixed read-only webhook wake (simulated data)."""
import asyncio
import hashlib
import hmac
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import types
import unittest
from unittest.mock import patch

SOURCE = os.environ.get("ANAN_HERMES_SOURCE")
if not SOURCE:
    raise RuntimeError("ANAN_HERMES_SOURCE_required")
CORE = Path(SOURCE).expanduser()
if not CORE.is_dir():
    raise RuntimeError("ANAN_HERMES_SOURCE_directory_missing")
CORE = CORE.resolve(strict=True)
EXPECTED_CORE_SHA = "d337b736aa1e8ebecfab043842d13e4a2d2f48a3"
head = subprocess.run(["git", "-C", str(CORE), "rev-parse", "HEAD"],
                      capture_output=True, text=True, check=True).stdout.strip()
dirty = subprocess.run(["git", "-C", str(CORE), "status", "--porcelain"],
                       capture_output=True, text=True, check=True).stdout
if head != EXPECTED_CORE_SHA or dirty:
    raise RuntimeError("official_hermes_core_sha_or_cleanliness_mismatch")

from aiohttp import web
from aiohttp.test_utils import TestClient, TestServer

sys.path.insert(0, str(CORE))

from gateway.config import Platform, PlatformConfig
from gateway.platforms.event import MessageEvent, MessageType
from gateway.platforms.webhook import WebhookAdapter
from gateway.run_inbound import GatewayInboundMixin
from gateway.session import SessionSource
from gateway import session_context as sc
from hermes_cli import lifecycle, plugins


spec = importlib.util.spec_from_file_location("pms_plugin_wake_test", Path(__file__).parents[1] / "__init__.py")
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)
wake = plugin.workitem_wake
WORK = "ca371c22-2d18-4cbe-9e09-5fe3ee9614f0"
BINDING = "23b46446-aa4a-43ee-8cb3-56f97ecbd305"


class Context:
    def __init__(self):
        self.hooks = {}
        self.tools = {}

    def register_hook(self, name, callback):
        self.hooks.setdefault(name, []).append(callback)

    def register_tool(self, **tool):
        self.tools[tool["name"]] = tool


class Pending:
    def __init__(self):
        self.called = []

    def _hm_update_prompt_reply(self, *_):
        self.called.append("update")

    async def _hm_clarify_reply(self, *_):
        self.called.append("clarify")

    async def _hm_slash_confirm_reply(self, *_):
        self.called.append("confirm")


def event(work=WORK, **changes):
    source = SessionSource(platform=Platform.WEBHOOK, chat_type="webhook",
        chat_id=f"{wake.CHAT_PREFIX}{work}", user_id=wake.USER_ID)
    item = MessageEvent(text="/restart", source=source, message_type=MessageType.TEXT,
        raw_message={"schema_version": 1, "work_item_id": work}, message_id=work)
    for key, value in changes.items():
        target, attr = (source, key.removeprefix("source_")) if key.startswith("source_") else (item, key)
        setattr(target, attr, value)
    return item


class WorkItemWakeTests(unittest.TestCase):
    def setUp(self):
        self.ctx = Context()
        plugin.register(self.ctx)
        self.env = patch.dict(os.environ, {
            "QINTOPIA_PMS_WORKITEM_WAKE_ENABLE": "1", "QINTOPIA_PMS_EVENT_BINDING": BINDING,
            "QINTOPIA_FOUNDATION_GATEWAY_ID": "simulated-gateway"})
        self.env.start()
        self.addCleanup(self.env.stop)
        self.mode = patch.object(plugin.production, "mode", return_value="production")
        self.mode.start()
        self.addCleanup(self.mode.stop)
        self.require = patch.object(plugin.production, "require")
        self.require.start()
        self.addCleanup(self.require.stop)
        self.private = patch.object(wake, "_private_route_ready", return_value=True)
        self.private.start()
        self.addCleanup(self.private.stop)
        self.search = patch.object(wake, "_tool_search_off", return_value=True)
        self.search.start()
        self.addCleanup(self.search.stop)
        from hermes_cli import profiles
        profile_root = Path("/simulated/.hermes/profiles")
        self.profile_root = patch.object(profiles, "_get_profiles_root", return_value=profile_root)
        self.profile_root.start()
        self.addCleanup(self.profile_root.stop)
        self.profile_home = patch("hermes_constants.get_hermes_home",
                                  return_value=profile_root / "anan")
        self.profile_home.start()
        self.addCleanup(self.profile_home.stop)
        self.assertEqual(profiles.get_active_profile_name(), "anan")
        sc.reset_session_vars()
        self.addCleanup(sc.reset_session_vars)

    def inbound(self, item):
        def invoke(name, **kwargs):
            if name != "pre_gateway_dispatch":
                return []
            return [r for callback in self.ctx.hooks[name]
                    if (r := callback(kwargs["event"])) is not None]
        with patch.object(lifecycle, "invoke_hook", side_effect=invoke):
            return GatewayInboundMixin._hm_pre_gateway_dispatch_hook(object(), item, item.source)

    def bind(self, item, **changes):
        values = {"platform": "webhook", "chat_type": "webhook", "user_id": wake.USER_ID,
                  "chat_id": item.source.chat_id, "message_id": item.source.message_id or "",
                  "profile": "anan"}
        values.update(changes)
        sc.set_session_vars(**values)

    def tool_block(self, name, args):
        def invoke(hook, **kwargs):
            if hook != "pre_tool_call":
                return []
            return [r for callback in self.ctx.hooks[hook]
                    if (r := callback(kwargs["tool_name"], kwargs["args"])) is not None]
        with patch.object(lifecycle, "invoke_hook", side_effect=invoke):
            return plugins.get_pre_tool_call_block_message(name, args)

    def test_official_inbound_hook_disables_command_and_pending_before_dispatch(self):
        item = event()
        self.assertTrue(item.is_command())
        self.assertIs(self.inbound(item), item)
        self.assertFalse(item.internal)
        self.assertFalse(item.allow_gateway_control)
        self.assertEqual(item.text, wake.PROMPT)
        self.assertIsNone(item.raw_message)
        self.assertEqual(item.source.profile, "anan")
        self.assertEqual(item.source.message_id, WORK)
        self.assertFalse(item.is_command())
        pending = Pending()
        self.assertIsNone(asyncio.run(GatewayInboundMixin._hm_pending_reply_intercepts(
            pending, item, item.source, "simulated-key")))
        self.assertEqual(pending.called, [])
        already = event(source_profile="anan")
        self.assertIs(self.inbound(already), already)

    def test_signed_official_webhook_resolves_default_route_then_runs_inbound_hook(self):
        secret = "simulated-private-route-key-" + "x" * 24
        route = {"profile": "anan", "secret": secret, "deliver": "log",
                 "prompt": "WorkItem {work_item_id}"}
        adapter = WebhookAdapter(PlatformConfig(enabled=True, extra={
            "host": "127.0.0.1", "port": 0, "routes": {wake.ROUTE: route}}))
        received = []
        done = asyncio.Event()

        async def handle(item):
            original_profile = item.source.profile
            accepted = self.inbound(item)
            received.append((original_profile, accepted, item))
            done.set()

        adapter.handle_message = handle
        app = web.Application()
        app.router.add_post("/webhooks/{route_name}", adapter._handle_webhook)
        app.router.add_post("/p/{profile}/webhooks/{route_name}", adapter._handle_webhook)
        body = json.dumps({"schema_version": 1, "work_item_id": WORK},
                          separators=(",", ":")).encode()
        timestamp = str(int(time.time()))
        signature = hmac.new(secret.encode(), timestamp.encode() + b"." + body,
                             hashlib.sha256).hexdigest()
        headers = {"Content-Type": "application/json", "X-Request-ID": WORK,
                   "X-Webhook-Timestamp": timestamp, "X-Webhook-Signature-V2": signature}

        async def exercise():
            async with TestClient(TestServer(app)) as client:
                wrong = await client.post(f"/webhooks/{wake.ROUTE}", data=body, headers=headers)
                self.assertEqual(wrong.status, 404)
                with patch("hermes_cli.profiles.profile_matches_home", return_value=True):
                    prefixed = await client.post(f"/p/anan/webhooks/{wake.ROUTE}",
                                                 data=body, headers=headers)
                self.assertEqual(prefixed.status, 404)
                del route["profile"]
                unsigned = await client.post(f"/webhooks/{wake.ROUTE}", data=body,
                                             headers={**headers, "X-Webhook-Signature-V2": "0" * 64})
                self.assertEqual(unsigned.status, 401)
                accepted = await client.post(f"/webhooks/{wake.ROUTE}", data=body, headers=headers)
                self.assertEqual(accepted.status, 202)
                self.assertEqual((await accepted.json())["delivery_id"], WORK)
                await asyncio.wait_for(done.wait(), timeout=1)

        asyncio.run(exercise())
        self.assertEqual(len(received), 1)
        initial_profile, accepted, item = received[0]
        self.assertIsNone(initial_profile)
        self.assertIs(accepted, item)
        self.assertEqual(item.source.profile, "anan")
        self.assertEqual(item.source.message_id, WORK)
        self.assertEqual(item.text, wake.PROMPT)
        self.assertIsNone(item.raw_message)
        self.assertFalse(item.allow_gateway_control)

    def test_invalid_events_and_hook_error_drop_in_official_inbound(self):
        for change in (
            {"raw_message": {"schema_version": 1, "work_item_id": WORK, "text": "/restart"}},
            {"raw_message": {"schema_version": True, "work_item_id": WORK}},
            {"source_user_id": "webhook:other"}, {"source_profile": "erhua"},
            {"source_profile": "default"},
            {"source_chat_id": "webhook:other:" + WORK}, {"message_id": "other"},
            {"media_urls": ["/private/media"]}, {"prompt_response": {"option_id": "approve"}},
            {"reply_to_message_id": "earlier"}, {"metadata": {"command": "/restart"}},
        ):
            with self.subTest(change=change):
                item = event(**change)
                self.assertIsNone(self.inbound(item))
                self.assertIsNone(item.source.message_id)
                self.assertFalse(item.allow_gateway_control)
                self.assertEqual(item.text, wake.PROMPT)
        with patch.object(wake, "_valid_event", side_effect=RuntimeError("private diagnostic")):
            self.assertIsNone(self.inbound(event()))
        with patch.object(plugin.production, "require", side_effect=RuntimeError("private diagnostic")):
            item = event()
            self.assertIsNone(self.inbound(item))
            self.assertFalse(item.allow_gateway_control)
            self.assertIsNone(item.source.message_id)

    def test_official_tool_hook_blocks_all_other_webhook_tools_and_wrappers(self):
        item = event()
        self.inbound(item)
        self.bind(item)
        for name in ("terminal", "browser_navigate", "cronjob_manage", "send_message",
                     "qintopia_pms_execute", "tool_search", "tool_describe", "tool_call"):
            with self.subTest(name=name):
                self.assertEqual(self.tool_block(name, {}), wake.DENIED["message"])
        self.assertEqual(self.tool_block(wake.TOOL, {"work_item": WORK}), wake.DENIED["message"])
        self.assertIsNone(self.tool_block(wake.TOOL, {}))
        self.bind(item, message_id="")
        self.assertEqual(self.tool_block(wake.TOOL, {}), wake.DENIED["message"])

    def test_read_uses_only_bound_context_and_projects_broker_result(self):
        item = event()
        self.inbound(item)
        self.bind(item)
        seen = []
        def broker(request, **kwargs):
            seen.append((request, kwargs))
            # Match the live broker response asserted in business_live_tests.rs.
            return {"ok": True, "result": {"work_item_id": WORK, "kind": "payment",
                "status": "awaiting_review", "readback_required": True,
                "contact_status": "pending_verified_channel",
                "requires_human_confirmation": True}}
        with patch.object(plugin, "transport", side_effect=broker):
            answer = json.loads(self.ctx.tools[wake.TOOL]["handler"]({}))
        self.assertEqual(answer, {"ok": True, "result": {"work_item_id": WORK,
            "kind": "payment", "status": "awaiting_review", "readback_required": True,
            "contact_status": "pending_verified_channel",
            "requires_human_confirmation": True}})
        self.assertEqual(seen, [({"operation": "person_foundation_ingress", "schema_version": 1,
            "agent": "anan", "tool": "pms_workitem_read",
            "trusted_context": {"gateway_id": "simulated-gateway", "platform": "host",
                "chat_type": "", "chat_id": "", "sender_id": "", "message_id": ""},
            "arguments": {"binding": BINDING, "work_item": WORK}}, {"host": True})])
        with patch.object(plugin, "transport", return_value={"ok": True, "result": {
                **answer["result"], "phone": "private", "amount": 999,
                "raw_text": "private", "order": {"id": "private"}}}):
            projected = json.loads(self.ctx.tools[wake.TOOL]["handler"]({}))
            self.assertEqual(projected, answer)
        with patch.object(plugin, "transport", return_value={"ok": True, "result": {
                "id": WORK, "kind": "payment", "status": "awaiting_review"}}):
            denied = json.loads(self.ctx.tools[wake.TOOL]["handler"]({}))
            self.assertEqual(denied["error"]["code"], "workitem_wake_unavailable")
        for changed in ({"kind": "application"}, {"requires_human_confirmation": None}):
            with patch.object(plugin, "transport", return_value={"ok": True, "result": {
                    **answer["result"], **changed}}):
                denied = json.loads(self.ctx.tools[wake.TOOL]["handler"]({}))
                self.assertEqual(denied["error"]["code"], "workitem_wake_unavailable")
        missing = {key: value for key, value in answer["result"].items()
                   if key != "requires_human_confirmation"}
        with patch.object(plugin, "transport", return_value={"ok": True, "result": missing}):
            denied = json.loads(self.ctx.tools[wake.TOOL]["handler"]({}))
            self.assertEqual(denied["error"]["code"], "workitem_wake_unavailable")
        with patch.object(plugin, "transport") as transport:
            self.bind(item, chat_id="webhook:other:" + WORK)
            denied = json.loads(self.ctx.tools[wake.TOOL]["handler"]({}))
            self.assertEqual(denied["error"]["code"], "workitem_wake_unavailable")
            transport.assert_not_called()

    def test_folded_tool_cannot_read_and_wecom_hook_unchanged(self):
        item = event()
        self.inbound(item)
        self.bind(item)
        with patch.object(wake, "_tool_search_off", return_value=False), patch.object(plugin, "transport") as transport:
            self.assertEqual(self.tool_block("tool_call", {"name": wake.TOOL}), wake.DENIED["message"])
            self.assertEqual(self.tool_block(wake.TOOL, {}), wake.DENIED["message"])
            denied = json.loads(self.ctx.tools[wake.TOOL]["handler"]({}))
            self.assertFalse(denied["ok"])
            transport.assert_not_called()
        sc.set_session_vars(platform="wecom", chat_type="dm", user_id="simulated-staff",
            chat_id="simulated-chat", message_id="simulated-message", profile="anan")
        self.assertIsNone(wake.guard("qintopia_pms_read", {}, production=plugin.production))

    def test_managed_literal_required_and_never_reused_from_gateway_env(self):
        self.private.stop()
        secret = "simulated-private-route-key-" + "x" * 24
        config = {"platforms": {"webhook": {"extra": {"host": "127.0.0.1", "routes": {
            wake.ROUTE: {"deliver": "log", "secret": secret}}}}}}
        from hermes_cli import managed_scope
        from agent import secret_scope
        with patch.object(managed_scope, "get_managed_dir", return_value=Path("/simulated/managed")), \
                patch.object(managed_scope, "load_managed_config", return_value=config), \
                patch.object(managed_scope, "load_managed_env", return_value={}), \
                patch("hermes_constants.get_hermes_home", return_value=Path("/simulated/profile")), \
                patch.object(secret_scope, "load_env_file", return_value={}):
            self.assertTrue(wake._private_route_ready())
            config["platforms"]["webhook"]["extra"]["routes"][wake.ROUTE]["profile"] = "anan"
            self.assertFalse(wake._private_route_ready())
            config["platforms"]["webhook"]["extra"]["routes"][wake.ROUTE]["profile"] = "default"
            self.assertTrue(wake._private_route_ready())
            del config["platforms"]["webhook"]["extra"]["routes"][wake.ROUTE]["profile"]
            config["platforms"]["webhook"]["extra"]["secret"] = secret
            self.assertFalse(wake._private_route_ready())
            del config["platforms"]["webhook"]["extra"]["secret"]
            config["platforms"]["webhook"]["extra"]["routes"]["other"] = {"secret": secret}
            self.assertFalse(wake._private_route_ready())
            del config["platforms"]["webhook"]["extra"]["routes"]["other"]
            with patch.dict(os.environ, {"ANAN_WORKITEM_WEBHOOK_SECRET": secret}):
                self.assertFalse(wake._private_route_ready())
            config["platforms"]["webhook"]["extra"]["routes"][wake.ROUTE]["secret"] = "${env:ANY_KEY}"
            self.assertFalse(wake._private_route_ready())

    def test_other_active_profile_cannot_accept_default_route(self):
        item = event()
        with patch("hermes_constants.get_hermes_home",
                   return_value=Path("/simulated/.hermes/profiles/erhua")):
            self.assertIsNone(self.inbound(item))
        self.assertIsNone(item.source.profile)
        self.assertIsNone(item.source.message_id)


if __name__ == "__main__":
    unittest.main()
