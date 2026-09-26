"""Opt-in Docker browser filesystem simulation, without page automation.

Only inspect the container and its CDP discovery endpoint. No Playwright, model,
PMS, real credential, business website or channel is used. This is not a browser
UI acceptance test or proof of the complete production require() path.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import socket
import subprocess
import tempfile
import time
from urllib.request import ProxyHandler, build_opener
import uuid


assert os.environ.get("ANAN_BROWSER_SIMULATE") == "1", "explicit_simulation_required"
image = os.environ["ANAN_BROWSER_IMAGE"]
assert re.fullmatch(r"chromedp/headless-shell@sha256:[a-f0-9]{64}", image), "pinned_image_required"
spec = importlib.util.spec_from_file_location("pms_browser_container", Path(__file__).parents[1] / "production.py")
production = importlib.util.module_from_spec(spec)
spec.loader.exec_module(production)
name = "anan-browser-simulation-" + uuid.uuid4().hex[:12]
created = False


def docker(*args):
    return subprocess.check_output(["docker", *args], text=True, stderr=subprocess.PIPE, timeout=30)


try:
    image_info = json.loads(docker("image", "inspect", image))[0]
    docker("run", "--detach", "--name", name, "--label", "qintopia.anan.simulation=browser",
           "--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
           "--pids-limit", "128", "--memory", "768m", "--cpus", "1",
           "--tmpfs", "/tmp:rw,nosuid,size=256m", "--shm-size", "256m",
           "--publish", "127.0.0.1::9222", image,
           "--user-data-dir=/tmp/anan-browser-simulation", "--disable-background-networking",
           "--disable-component-update", "--disable-sync", "--no-first-run",
           "--no-default-browser-check", "--metrics-recording-only", "about:blank")
    created = True
    info = json.loads(docker("inspect", name))[0]
    port = int(info["NetworkSettings"]["Ports"]["9222/tcp"][0]["HostPort"])
    policy = {"container": name, "image": image, "cdp_url": f"http://127.0.0.1:{port}"}
    production.check_browser_container(info, image_info, policy=policy, port=port)
    opener = build_opener(ProxyHandler({}))
    deadline = time.monotonic() + 15
    while True:
        try:
            with opener.open(policy["cdp_url"] + "/json/version", timeout=1) as response:
                raw = response.read(16385)
                assert len(raw) <= 16384, "oversized_discovery"
                version = json.loads(raw)
            break
        except (OSError, ValueError):
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.1)
    production.check_browser_endpoint(version["webSocketDebuggerUrl"], port)
    process = docker("exec", name, "/bin/cat", "/proc/1/comm").strip()
    assert "headless" in process, "browser_process_missing"
    with tempfile.TemporaryDirectory(prefix="anan-browser-private-", dir="/tmp") as private:
        credentials = Path(private) / ".env"
        credentials.write_text("SIMULATED_PRIVATE_VALUE=never-mounted\n")
        credentials.chmod(0o600)
        broker = Path(private) / "broker.sock"
        listener = socket.socket(socket.AF_UNIX)
        listener.bind(str(broker))
        try:
            probe = " && ".join("test ! -e " + shlex.quote(str(path)) for path in
                                (credentials, broker, "/var/run/docker.sock", "/proc/1/root" + str(credentials)))
            docker("exec", name, "/bin/sh", "-c", probe)
            assert "SIMULATED_PRIVATE_VALUE" not in docker("exec", name, "/usr/bin/env")
        finally:
            listener.close()
    docker("stop", "--time", "5", name)
    stopped = json.loads(docker("inspect", name))[0]
    try:
        production.check_browser_container(stopped, image_info, policy=policy, port=port)
    except ValueError as error:
        assert str(error) == production.ERROR
    else:
        raise AssertionError("stopped_browser_accepted")
    print(json.dumps({"environment": "local Linux Docker browser filesystem simulation",
                      "image": image, "image_id": image_info["Id"],
                      "browser": version.get("Browser"), "browser_process_running": True,
                      "host_private_file_and_broker_visible": False, "docker_socket_visible": False,
                      "mounts": info.get("Mounts", []), "rootfs_readonly": True,
                      "cdp_discovery_matches_inspected_port": True, "stopped_container_denied": True,
                      "page_automation_used": False, "full_production_require_verified": False,
                      "production_acceptance": False}))
finally:
    if created:
        subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=30, check=True)
