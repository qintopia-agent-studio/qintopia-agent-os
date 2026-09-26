import importlib.util
import copy
import json
import os
from pathlib import Path
import types
import unittest
from unittest.mock import Mock, patch


spec = importlib.util.spec_from_file_location("pms_production_test", Path(__file__).parents[1] / "__init__.py")
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)
production = plugin.production
IMAGE = "python@sha256:" + "a" * 64
LIVE = {key: "1" for key in production.PRODUCTION_KEYS}


def container():
    return {"State": {"Running": True}, "Config": {"Image": IMAGE, "Env": []},
            "HostConfig": {"NetworkMode": "bridge", "CapDrop": ["ALL"],
                           "SecurityOpt": ["no-new-privileges"]}, "Mounts": []}


class ProductionTests(unittest.TestCase):
    def test_host_mcp_does_not_inherit_terminal_isolation(self):
        production.check_mcp_boundary(None, {}, ["terminal", "qintopia-pms"])
        production.check_mcp_boundary({"configured_but_disabled": {"enabled": False}}, {}, [])
        for configured, portable, registered in (
            ({"filesystem": {"command": "python3", "args": ["host_tool.py"]}}, {}, []),
            ({"remote": {"url": "https://unreviewed.invalid/mcp"}}, {}, []),
            ({}, {"plugin-mcp": {"command": "uvx"}}, []),
            ({}, {}, ["mcp-removed-server"]),
            ({"malformed": False}, {}, []),
            ([], {}, []),
        ):
            with self.subTest(configured=configured, portable=portable, registered=registered), \
                    self.assertRaisesRegex(ValueError, production.ERROR):
                production.check_mcp_boundary(configured, portable, registered)

    def test_browser_policy_rejects_host_routes_and_runtime_overrides(self):
        policy = {"container": "qintopia-anan-browser", "image": IMAGE,
                  "cdp_url": "http://127.0.0.1:19222"}
        managed = {"qintopia_pms_isolation": {"browser": policy}}
        browser = {"backend": "off", "cdp_url": policy["cdp_url"],
                   "auto_local_for_private_urls": False, "use_real_profile": False,
                   "allow_private_urls": False, "extension_control": {"enabled": False}}
        def check(value, override=policy["cdp_url"]):
            return production.browser_policy(managed, {"browser": value}, cdp_override=override)
        self.assertEqual(check(browser), (policy, 19222))
        for change in ({"backend": "browser-use"}, {"backend": ""}, {"cdp_url": "http://localhost:9222"},
                       {"auto_local_for_private_urls": True}, {"use_real_profile": True},
                       {"allow_private_urls": True}, {"extension_control": {"enabled": True}}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                check({**browser, **change})
        with self.assertRaises(ValueError):
            check(browser, "http://127.0.0.1:9222")
        with self.assertRaises(ValueError):
            production.browser_policy({}, {"browser": browser}, cdp_override=policy["cdp_url"])
        production.check_browser_endpoint("ws://127.0.0.1:19222/devtools/browser/simulated-id", 19222)
        for endpoint in ("http://127.0.0.1:19222", "ws://127.0.0.1:9222/devtools/browser/other",
                         "ws://localhost:19222/devtools/browser/id", "ws://secret@127.0.0.1:19222/devtools/browser/id",
                         "ws://127.0.0.1:19222/devtools/page/id", "ws://127.0.0.1:19222/devtools/browser/id?token=simulated"):
            with self.subTest(endpoint=endpoint), self.assertRaises(ValueError):
                production.check_browser_endpoint(endpoint, 19222)

    def test_browser_daemon_proves_no_host_files_secrets_or_foreign_listener(self):
        info = container()
        info["Image"] = "sha256:" + "b" * 64
        info["NetworkSettings"] = {"Ports": {"9222/tcp": [{"HostIp": "127.0.0.1", "HostPort": "19222"}]}}
        image_info = {"Id": info["Image"], "Config": {"Env": []}}
        def check(value):
            production.check_browser_container(value, image_info, policy={"image": IMAGE}, port=19222)
        check(info)
        for mount in ({"Type": "bind", "Source": "/private/anan", "Destination": "/data", "RW": False},
                      {"Type": "volume", "Source": "private-state", "Destination": "/data"},
                      {"Type": "tmpfs", "Destination": "/proc"}):
            with self.subTest(mount=mount), self.assertRaises(ValueError):
                check({**info, "Mounts": [mount]})
        check({**info, "Mounts": [{"Type": "tmpfs", "Destination": "/tmp"}]})
        for changed in ("sha256:" + "c" * 64, None):
            with self.subTest(image=changed), self.assertRaises(ValueError):
                check({**info, "Image": changed})
        # A renamed credential is still an extra runtime environment value.
        with self.assertRaises(ValueError):
            check({**info, "Config": {"Image": IMAGE, "Env": ["RENAMED_SECRET=simulated"]}})
        for ports in ({}, {"9222/tcp": [{"HostIp": "0.0.0.0", "HostPort": "19222"}]},
                      {"9222/tcp": [{"HostIp": "127.0.0.1", "HostPort": "9222"}]},
                      {"9222/udp": [{"HostIp": "127.0.0.1", "HostPort": "19222"}]},
                      {"9222/tcp": [{"HostIp": "127.0.0.1", "HostPort": "19222"}],
                       "9223/tcp": [{"HostIp": "0.0.0.0", "HostPort": "19223"}]}):
            with self.subTest(ports=ports), self.assertRaises(ValueError):
                check({**info, "NetworkSettings": {"Ports": ports}})
        unsafe = copy.deepcopy(info)
        unsafe["State"]["Running"] = False
        with self.assertRaises(ValueError):
            check(unsafe)

    def test_modes_are_explicit_and_mutually_exclusive(self):
        self.assertEqual(production.mode({}), "disabled")
        self.assertEqual(production.mode(LIVE), "production")
        self.assertEqual(production.mode({key: "1" for key in production.LOCAL_KEYS}), "local")
        for key in production.LOCAL_KEYS:
            self.assertEqual(production.mode({**LIVE, key: "1"}), "disabled")
        for key in production.PRODUCTION_KEYS:
            self.assertEqual(production.mode({key: "1"}), "disabled")

    def test_configuration_rejects_host_escape_and_secret_passthrough(self):
        good = {"env_type": "docker", "docker_image": IMAGE}
        def check(config, secrets=None, path="/private/anan/.env"):
            production.check_configuration(config, credentials_file=path,
                profile_home="/profiles/anan", secret_values=secrets or {})
        check(good)
        for override in ({"env_type": "local"}, {"env_type": "ssh"},
                         {"docker_image": "python:latest"}, {"docker_volumes": ["/:/host"]},
                         {"docker_extra_args": ["--privileged"]}, {"docker_snap_compat": True},
                         {"docker_mount_cwd_to_workspace": True},
                         {"docker_forward_env": ["QINTOPIA_FOUNDATION_HOST_TOKEN"]},
                         {"docker_shared_container_key": "other-profile"}):
            with self.subTest(override=override), self.assertRaisesRegex(ValueError, production.ERROR):
                check({**good, **override})
        for key in production.SECRETS:
            with self.subTest(key=key), self.assertRaises(ValueError):
                check(good, {key: "simulated-private-value"})
        for path in ("/profiles/anan/credentials/.env", "/private/anan/credentials.json", "relative/.env"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                check(good, path=path)

    def test_actual_container_state_cannot_override_config(self):
        def check(value):
            production.check_container(value, image=IMAGE, private_paths=["/private/anan/.env", "/private/anan/broker.sock"])
        check(container())
        changes = [{"Privileged": True}, {"PidMode": "host"}, {"NetworkMode": "host"},
                   {"NetworkMode": "container:another"}, {"CapAdd": ["SYS_ADMIN"]},
                   {"CapDrop": []}, {"SecurityOpt": []}, {"Devices": [{}]},
                   {"SecurityOpt": ["no-new-privileges", "seccomp:unconfined"]}]
        for change in changes:
            info = container()
            info["HostConfig"].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                check(info)
        for source, dest, writable in [("/private", "/root/.hermes/skills", False),
                                       ("/proc", "/root/.hermes/cache", False),
                                       ("/var/run/docker.sock", "/run/docker.sock", False),
                                       ("/profiles/anan", "/workspace", True),
                                       ("/profiles/anan/skills", "/root/.hermes/skills", True)]:
            info = container()
            info["Mounts"] = [{"Type": "bind", "Source": source, "Destination": dest, "RW": writable}]
            with self.subTest(source=source), self.assertRaises(ValueError):
                check(info)
        info = container()
        info["Config"]["Env"] = ["GREENPMS_API_TOKEN=simulated"]
        with self.assertRaises(ValueError):
            check(info)
        info = container()
        info["State"]["Running"] = False
        with self.assertRaises(ValueError):
            check(info)

    def test_application_secret_cannot_enter_an_otherwise_allowed_credential_mount(self):
        info = container()
        info["Mounts"] = [{"Type": "bind", "Source": "/etc/qintopia", "RW": False,
                           "Destination": "/root/.hermes/credentials/application"}]
        production.check_container(info, image=IMAGE, private_paths=["/private/anan/.env"])
        with self.assertRaisesRegex(ValueError, production.ERROR):
            production.check_container(info, image=IMAGE,
                private_paths=["/private/anan/.env", "/etc/qintopia/application.json"])

    def test_unconnected_event_routes_fail_before_pms_calls(self):
        # The shared live broker is the capability/authority boundary. A source
        # work item never turns the event itself into permission to execute.
        rejected = {"ok": False, "error": {"code": "agent_tool_denied"}}
        for name, args in [
                ("prepare", {"binding": "binding", "operation": "pms.command.RECORD_COLLECTION",
                             "work_item": "event-work", "input": {},
                             "reason": {"code": "fixture", "note": "模拟事项"}}),
                ("link", {"action": "action", "work_item": "event-work"}),
                ("reminder_snooze", {"binding": "binding", "work_item": "event-work", "until": "2026-09-26T00:00:00Z"})]:
            pms = Mock()
            broker = Mock(return_value=rejected)
            operations = plugin.Operations(pms, broker, lambda: {})
            with self.subTest(name=name):
                with self.assertRaisesRegex(ValueError, "agent_tool_denied"):
                    operations.invoke(name, args)
                self.assertTrue(broker.called)
                self.assertEqual(pms.mock_calls, [])

    def test_failure_is_denial_before_credentials_or_pms_client(self):
        class Context:
            def __init__(self): self.handlers = {}; self.hooks = {}
            def register_hook(self, name, handler): self.hooks[name] = handler
            def register_tool(self, **tool): self.handlers[tool["name"]] = tool["handler"]
        ctx = Context()
        plugin.register(ctx)
        with patch.dict(os.environ, LIVE, clear=True), \
                patch.object(production, "require", side_effect=RuntimeError("private diagnostic")), \
                patch.object(plugin.credentials, "load") as load, patch.object(plugin.client, "Client") as client:
            result = json.loads(ctx.handlers["qintopia_pms_context"]({}))
            self.assertFalse(result["ok"])
            self.assertNotIn("private diagnostic", json.dumps(result))
            load.assert_not_called()
            client.assert_not_called()
            self.assertEqual(ctx.hooks["pre_tool_call"]("terminal", {"command": "true"})["action"], "block")

    def test_capture_failure_preserves_chat_without_authorizing_business(self):
        class Context:
            def __init__(self): self.hooks = {}
            def register_hook(self, name, handler): self.hooks[name] = handler
            def register_tool(self, **tool): pass
        ctx = Context()
        plugin.register(ctx)
        event = types.SimpleNamespace(source=types.SimpleNamespace(
            platform="wecom", chat_type="dm", chat_id="fixture-chat", user_id="fixture-staff",
            is_bot=False, profile="anan"), internal=False, text="模拟普通咨询", message_id="fixture-message")
        rejected = {"ok": False, "error": {"code": "trusted_message_evidence_required"}}
        with patch.dict(os.environ, {**LIVE, "QINTOPIA_FOUNDATION_GATEWAY_ID": "fixture-gateway"}, clear=True):
            for outcome in (rejected, RuntimeError("private diagnostic")):
                kwargs = {"side_effect": outcome} if isinstance(outcome, Exception) else {"return_value": outcome}
                with patch.object(plugin, "transport", **kwargs), self.assertLogs(plugin.host.logger, level="WARNING") as logs:
                    self.assertIsNone(ctx.hooks["pre_gateway_dispatch"](event))
                self.assertNotIn("private diagnostic", " ".join(logs.output))
                self.assertNotIn(event.text, " ".join(logs.output))
        pms = Mock()
        operations = plugin.Operations(pms, lambda *a, **k: rejected, lambda: {})
        with self.assertRaisesRegex(ValueError, "trusted_message_evidence_required"):
            operations.read({"query": "order", "binding": "fixture-binding", "resource": "fixture-order"})
        pms.read.assert_not_called()

    def test_policy_retains_sandbox_tools_and_plain_cron(self):
        with patch.dict(os.environ, LIVE, clear=True), patch.object(production, "require"):
            for name in ("terminal", "execute_code", "read_file", "write_file", "browser_navigate", "skill_manage"):
                self.assertIsNone(production.guard(name, {}))
            self.assertIsNone(production.guard("cronjob_manage", {"action": "create", "prompt": "模拟任务"}))
            for name, args in [("browser_exec", {"code": "print(1)"}),
                               ("cronjob", {"script": "a.py"}), ("cronjob", {"no_agent": True}),
                               ("cronjob", {"monitor": "a.py"}), ("cronjob", {"monitor_script": "a.py"}),
                               ("cronjob_manage", {"script": "a.py"}),
                               ("cronjob_manage", {"no_agent": True}),
                               ("cronjob_manage", {"monitor": "a.py"}),
                               ("cronjob_manage", {"monitor_script": "a.py"})]:
                self.assertEqual(production.guard(name, args)["action"], "block")
        with patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(production.guard("browser_exec", {}))


if __name__ == "__main__":
    unittest.main()
