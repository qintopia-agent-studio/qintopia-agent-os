"""Opt-in official browser routing simulation. No browser or network is used.

The real Hermes config/CDP/session code runs against recorded Docker metadata
and an HTTP discovery substitute. This does not prove a browser filesystem or
production root-owned configuration is isolated.
"""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest.mock import Mock, patch


assert os.environ.get("ANAN_BROWSER_SIMULATE") == "1", "explicit_simulation_required"
source = Path(os.environ["ANAN_HERMES_SOURCE"]).resolve()
core = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
assert core == "d337b736aa1e8ebecfab043842d13e4a2d2f48a3", "reviewed_core_required"
spec = importlib.util.spec_from_file_location("pms_browser_simulation", Path(__file__).parents[1] / "production.py")
production = importlib.util.module_from_spec(spec)
spec.loader.exec_module(production)
image = "simulated-browser@sha256:" + "a" * 64
policy = {"container": "simulated-anan-browser", "image": image, "cdp_url": "http://127.0.0.1:19222"}
managed = {"qintopia_pms_isolation": {"browser": policy}}
endpoint = "ws://127.0.0.1:19222/devtools/browser/simulated-browser-id"
info = {"State": {"Running": True}, "Config": {"Image": image, "Env": ["PATH=/usr/bin"]},
        "Image": "sha256:" + "b" * 64, "Mounts": [],
        "HostConfig": {"NetworkMode": "bridge", "CapDrop": ["ALL"], "SecurityOpt": ["no-new-privileges"]},
        "NetworkSettings": {"Ports": {"9222/tcp": [{"HostIp": "127.0.0.1", "HostPort": "19222"}]}}}
image_info = {"Id": info["Image"], "Config": {"Env": ["PATH=/usr/bin"]}}
inspections = []


def inspect(args, **kwargs):
    inspections.append(args)
    assert args[:3] == ["docker", "inspect", "--type"]
    kind, reference = args[3:]
    assert (kind, reference) in {("container", policy["container"]), ("image", image)}
    return Mock(stdout=json.dumps([info if kind == "container" else image_info]))


def discovery(url, **kwargs):
    assert url == policy["cdp_url"] + "/json/version", "unexpected_network_target"
    return Mock(json=lambda: {"webSocketDebuggerUrl": endpoint})


with tempfile.TemporaryDirectory(prefix="anan-browser-simulation-") as temporary:
    home = Path(temporary) / "profiles" / "anan"
    home.mkdir(parents=True)
    home.joinpath("config.yaml").write_text(
        "browser:\n  backend: 'off'\n  cdp_url: http://127.0.0.1:19222\n"
        "  auto_local_for_private_urls: false\n  use_real_profile: false\n"
        "  allow_private_urls: false\n  extension_control:\n    enabled: false\n"
        "plugins:\n  enabled: []\n", encoding="utf-8")
    with patch.dict(os.environ, {"HERMES_HOME": str(home), "HERMES_PROFILE": "anan",
                                "BROWSER_CDP_URL": "", "CAMOFOX_URL": ""}):
        sys.path.insert(0, str(source))
        import requests
        from tools import browser_tool as browser
        from tools import browser_tool_session as sessions
        from tools import browser_cdp_tool as cdp
        from tools import browser_tool_eval_policy as evaluation
        calls = []

        async def record(resolved, method, params, target_id, timeout):
            calls.append({"endpoint": resolved, "method": method})
            assert resolved == endpoint, "browser_route_changed"
            return {}

        with patch.object(production.subprocess, "run", side_effect=inspect), \
                patch.object(requests, "get", side_effect=discovery), \
                patch.object(sessions, "_create_local_session", side_effect=AssertionError("host_browser_fallback")), \
                patch.object(cdp, "_WS_AVAILABLE", True), \
                patch.object(evaluation, "_current_page_private_url", return_value=None), \
                patch.object(cdp, "_cdp_call", side_effect=record):
            production.require_browser(managed, "docker")
            session = sessions._create_session_for_key("simulation", False)
            assert session["features"]["cdp_override"] and session["cdp_url"] == endpoint
            assert browser._navigation_session_key("simulation", "http://127.0.0.1/") == "simulation"
            result = json.loads(cdp.browser_cdp("DOM.setFileInputFiles",
                {"nodeId": 1, "files": ["/simulated-private/.env"]}, task_id="simulation"))
            assert result["success"] and len(calls) == 1
            rejected = []

            def denied(label):
                try:
                    production.require_browser(managed, "docker")
                except ValueError:
                    rejected.append(label)
                    return
                raise AssertionError("unsafe_browser_accepted: " + label)

            with patch.dict(os.environ, {"BROWSER_CDP_URL": "http://127.0.0.1:9222"}):
                denied("runtime_endpoint_override")
            with patch.object(browser, "_active_sessions", {"old": {"features": {"local": True}}}):
                denied("cached_host_session")
            unsafe = copy.deepcopy(info)
            unsafe["State"]["Running"] = False
            original = info
            info = unsafe
            denied("stopped_container")
            info = original
            endpoint = "ws://127.0.0.1:9222/devtools/browser/foreign-browser"
            denied("discovery_redirects_to_foreign_endpoint")
            endpoint = "ws://127.0.0.1:19222/devtools/browser/simulated-browser-id"
            with patch.object(requests, "get", side_effect=ConnectionError("simulated_browser_unavailable")):
                denied("discovery_failure")
            production.require_browser(managed, "docker")
            assert len(calls) == 1, "denied_routes_reached_browser"

report = {"official_core": core, "scope": "offline official browser routing simulation",
          "actual_config_and_session_code": True, "metadata_and_discovery": "simulated",
          "browser_and_network_used": False, "raw_cdp_target": "inspected-container-port",
          "denied": rejected, "host_fallback": False,
          "browser_filesystem_verified": False, "production_acceptance": False}
print(json.dumps(report, ensure_ascii=False))
