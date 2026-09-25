"""Opt-in Rust listener -> official webhook -> real Unix broker simulation.

The ignored Sidecar integration case owns the simulated PostgreSQL fixture and
private token file. This probe has no model, PMS business call or channel send.
Host isolation and managed installation are separate Linux checks: this probe
explicitly substitutes those policy prerequisites while exercising real HTTP,
official ingress hooks/ContextVars, credential loading and Unix broker transport.
"""
import asyncio
import importlib.util
import json
import os
from pathlib import Path
import sys
from unittest.mock import patch


if os.environ.get("ANAN_WORKITEM_JOINT_SIMULATE") != "1":
    raise RuntimeError("explicit_simulation_required")
secret = os.environ["ANAN_WORKITEM_SIMULATED_WEBHOOK_SECRET"]
if not secret.startswith("simulated-") or not 32 <= len(secret) <= 256:
    raise RuntimeError("simulated_webhook_key_required")
binding = os.environ["QINTOPIA_PMS_EVENT_BINDING"]
gateway = os.environ["QINTOPIA_FOUNDATION_GATEWAY_ID"]
private = Path(os.environ["QINTOPIA_PMS_CREDENTIALS_FILE"])
if not private.is_absolute() or private.name != ".env":
    raise RuntimeError("private_simulation_file_required")
tokens = json.loads(private.read_text())
if not tokens or not all(isinstance(v, str) and v.startswith("simulated-") for v in tokens.values()):
    raise RuntimeError("simulated_tokens_required")

# Reuse the package's explicit official-core check and real hook fixture. Its
# setUp substitutes only policy/installation prerequisites, never HTTP or broker.
spec = importlib.util.spec_from_file_location("workitem_official_joint_fixture",
    Path(__file__).with_name("workitem_official_hook_journey.py"))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
fixture.BINDING = binding
case = fixture.WorkItemWakeTests("test_read_uses_only_bound_context_and_projects_broker_result")
case.setUp()
plugin, wake = fixture.plugin, fixture.wake


async def main():
    done = asyncio.Event()
    result = {}
    received = 0
    adapter = fixture.WebhookAdapter(fixture.PlatformConfig(enabled=True, extra={
        "host": "127.0.0.1", "port": 0, "routes": {wake.ROUTE: {
            "secret": secret, "deliver": "log", "prompt": wake.PROMPT}}}))

    def read(item):
        if case.inbound(item) is not item:
            raise AssertionError("official_hook_rejected_joint_event")
        case.bind(item)
        try:
            for name in ("terminal", "execute_code", "send_message", "qintopia_pms_execute", "tool_call"):
                if case.tool_block(name, {}) != wake.DENIED["message"]:
                    raise AssertionError("webhook_tool_not_denied")
            if case.tool_block(wake.TOOL, {}) is not None:
                raise AssertionError("workitem_read_unavailable")
            value = json.loads(case.ctx.tools[wake.TOOL]["handler"]({}))
            if value.get("ok") is not True:
                raise AssertionError("real_broker_projection_unavailable")
            projected = value["result"]
            if (set(projected) != set(wake.FIELDS)
                    or projected["work_item_id"] != item.source.message_id
                    or projected["kind"] != "payment"
                    or projected["status"] != "awaiting_review"
                    or projected["requires_human_confirmation"] is not True):
                raise AssertionError("joint_projection_mismatch")
            return projected
        finally:
            fixture.sc.reset_session_vars()

    async def handle(item):
        nonlocal received
        received += 1
        try:
            projection = await asyncio.to_thread(read, item)
            result.update(ok=True, work_item_id=projection["work_item_id"], projection=projection)
        except Exception:
            # No private values or arbitrary diagnostic text in retained output.
            result.update(ok=False, error="joint_webhook_broker_failed")
        finally:
            done.set()

    adapter.handle_message = handle
    app = fixture.web.Application()
    app.router.add_post("/webhooks/{route_name}", adapter._handle_webhook)
    server = fixture.TestServer(app, host="127.0.0.1")
    with patch.dict(os.environ, {"QINTOPIA_FOUNDATION_GATEWAY_ID": gateway}):
        await server.start_server()
        try:
            print(json.dumps({"ready": True, "port": server.port}), flush=True)
            await asyncio.wait_for(done.wait(), timeout=30)
            if received != 1:
                raise AssertionError("unexpected_joint_delivery_count")
            result["simulation_boundaries"] = ["isolation_and_managed_installation_substituted",
                                               "no_model", "no_channel_send", "no_pms_business_write"]
            print(json.dumps(result, separators=(",", ":")), flush=True)
            if result.get("ok") is not True:
                raise AssertionError("joint_probe_failed")
        finally:
            await server.close()


try:
    asyncio.run(main())
finally:
    case.doCleanups()
