"""Opt-in official Hermes + Docker isolation simulation, without a model or PMS.

Run with ANAN_HERMES_SOURCE, ANAN_ISOLATION_IMAGE (immutable digest), and a fresh
ANAN_ISOLATION_DIRECTORY outside any real Profile. Only fabricated credentials
are used. No production enablement, network business request or channel send.
"""
import importlib.util
import json
import os
from pathlib import Path
import shlex
import socket
import subprocess
import sys
import tempfile
from unittest.mock import patch


assert os.environ.get("ANAN_ISOLATION_SIMULATE") == "1", "explicit_simulation_required"
full_production = os.environ.get("ANAN_ISOLATION_FULL_PRODUCTION") == "1"
root = Path(os.environ["ANAN_ISOLATION_DIRECTORY"]).resolve()
assert not root.exists(), "fresh_simulation_directory_required"
root.mkdir(parents=True, mode=0o700)
home = root / "profiles" / "anan"
home.mkdir(parents=True)
home.joinpath("config.yaml").write_text("plugins:\n  enabled: [pms-operations]\nskills:\n  inline_shell: false\n")
private = root / "private" / ".env"
private.parent.mkdir(mode=0o700)
tokens = {"GREENPMS_API_TOKEN": "simulated-pms-" + "x" * 40,
          "QINTOPIA_FOUNDATION_TOKEN": "simulated-tool-" + "y" * 40,
          "QINTOPIA_FOUNDATION_HOST_TOKEN": "simulated-host-" + "z" * 40}
private.write_text(json.dumps(tokens))
private.chmod(0o600)
application_private = private.parent / "application.json"
application_private.write_text(json.dumps({"foundation_host_token": tokens["QINTOPIA_FOUNDATION_HOST_TOKEN"]}))
application_private.chmod(0o600)
socket_directory = tempfile.TemporaryDirectory(prefix="anan-722-socket-")
socket_path = Path(socket_directory.name) / "broker.sock"
listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
listener.bind(str(socket_path))
listener.listen(1)
os.environ.update({"HERMES_HOME": str(home), "HERMES_PROFILE": "anan",
                   "TERMINAL_ENV": "docker", "TERMINAL_DOCKER_IMAGE": os.environ["ANAN_ISOLATION_IMAGE"],
                   "TERMINAL_CWD": "/workspace", "TERMINAL_CONTAINER_PERSISTENT": "false",
                   "TERMINAL_DOCKER_PERSIST_ACROSS_PROCESSES": "false",
                   "TERMINAL_DOCKER_ORPHAN_REAPER": "false", "TERMINAL_CONTAINER_MEMORY": "1024",
                   "HERMES_MEDIA_DELIVERY_STRICT": "1", "HERMES_MEDIA_TRUST_RECENT_FILES": "0"})
assert not any(os.environ.get(key) for key in tokens), "real_credentials_forbidden"
if full_production:
    assert sys.platform == "linux" and os.getuid() == 0, "disposable_linux_root_required"
    assert root == Path.home() / ".hermes", "official_named_profile_layout_required"
    managed = Path("/etc/anan-isolation-simulation")
    managed.mkdir(mode=0o750)
    browser_name = os.environ["ANAN_ISOLATION_BROWSER_CONTAINER"]
    browser_image = os.environ["ANAN_ISOLATION_BROWSER_IMAGE"]
    browser_port = int(os.environ["ANAN_ISOLATION_BROWSER_PORT"])
    cdp_url = f"http://127.0.0.1:{browser_port}"
    browser_settings = {"backend": "off", "cdp_url": cdp_url, "auto_local_for_private_urls": False,
                        "use_real_profile": False, "allow_private_urls": False,
                        "extension_control": {"enabled": False}}
    managed.joinpath("config.yaml").write_text(json.dumps({
        "skills": {"inline_shell": False},
        "browser": browser_settings,
        "qintopia_pms_isolation": {"browser": {"container": browser_name,
            "image": browser_image, "cdp_url": cdp_url}}}))
    managed.joinpath("config.yaml").chmod(0o640)
    # v2026.9.21 browser helpers read the raw Profile config, without managed
    # overlay. Mirror only these non-secret settings in this fresh simulation.
    home.joinpath("config.yaml").write_text(json.dumps({"plugins": {"enabled": ["pms-operations"]},
        "skills": {"inline_shell": False}, "browser": browser_settings}))
    os.environ.update({"HERMES_MANAGED_DIR": str(managed),
        "QINTOPIA_PMS_PRODUCTION_ENABLE": "1", "QINTOPIA_FOUNDATION_PRODUCTION_ENABLE": "1",
        "QINTOPIA_PMS_TOOL_POLICY": "docker-v1", "QINTOPIA_PMS_CREDENTIALS_FILE": str(private),
        "QINTOPIA_FOUNDATION_SOCKET": str(socket_path)})
source = Path(os.environ["ANAN_HERMES_SOURCE"]).resolve()
core = subprocess.check_output(["git", "-c", "safe.directory=" + str(source),
                               "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
assert core == "d337b736aa1e8ebecfab043842d13e4a2d2f48a3", "reviewed_core_required"
sys.path.insert(0, str(source))
spec = importlib.util.spec_from_file_location("pms_isolation_journey", Path(__file__).parents[1] / "__init__.py")
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)
from tools.terminal_tool import _get_env_config, terminal_tool
from tools.code_execution_tool import _get_or_create_env, execute_code
from tools.file_tools import read_file_tool, write_file_tool
from tools.credential_files import register_credential_file, get_credential_file_mounts
from tools.skills_tool import skill_view
from hermes_cli.plugins import get_plugin_manager
from tools.registry import registry

image = os.environ["ANAN_ISOLATION_IMAGE"]
env = None
try:
    # Exercise actual discovery/import, not just direct register() invocation.
    plugin_dir = Path(__file__).resolve().parents[1]
    import shutil
    installed = home / "plugins" / "pms-operations"
    shutil.copytree(plugin_dir, installed, ignore=shutil.ignore_patterns("__pycache__", "tests"))
    manager = get_plugin_manager()
    manager.discover_and_load()
    assert "pms-operations" in manager._plugins, "plugin_discovery_failed"
    loaded = manager._plugins["pms-operations"]
    assert not getattr(loaded, "error", None), "plugin_load_failed: " + str(getattr(loaded, "error", ""))
    assert registry.get_entry("qintopia_pms_context", scope=manager.scope_key) is not None
    assert manager._hooks.get("pre_tool_call") and manager._hooks.get("pre_gateway_dispatch")
    handler = registry.get_entry("qintopia_pms_context", scope=manager.scope_key).handler
    if not full_production:
        assert json.loads(handler({}))["error"]["code"] == "pms_disabled"

    # The official skill registry must not expose a Profile-external file, even
    # when a Skill asks for all three known names or attempts a symlink mount.
    skill = home / "skills" / "isolation-canary"
    skill.mkdir(parents=True)
    skill.joinpath("SKILL.md").write_text("---\nname: isolation-canary\ndescription: Simulation canary.\n"
        "required_environment_variables:\n" + "".join("  - " + key + "\n" for key in tokens)
        + "---\nRead the isolation simulation.\n")
    assert "isolation-canary" in skill_view("isolation-canary", preprocess=False)
    home.joinpath("credentials").mkdir()
    home.joinpath("credentials", "escape.json").symlink_to(private)
    assert not register_credential_file("credentials/escape.json")
    assert not register_credential_file(str(private))
    assert not register_credential_file("../../private/.env")
    assert all(Path(m["host_path"]).resolve() != private for m in get_credential_file_mounts())

    plugin.production.check_configuration(_get_env_config(), credentials_file=str(private),
        profile_home=str(home), secret_values={key: os.environ.get(key) for key in tokens})
    env, backend = _get_or_create_env("pms-isolation")
    assert backend == "docker"
    if full_production:
        # No external service call: check actual policy, official Docker/cache,
        # real browser discovery and private file opening within this Linux host.
        profile_config = home / "config.yaml"
        aligned_profile = profile_config.read_text()
        profile_config.write_text(json.dumps({"plugins": {"enabled": ["pms-operations"]},
                                             "skills": {"inline_shell": False}}))
        try:
            plugin.production.require(task_id="pms-isolation", private_paths=(str(private),))
        except ValueError as error:
            assert str(error) == plugin.production.ERROR
        else:
            raise AssertionError("managed_only_browser_policy_accepted")
        finally:
            profile_config.write_text(aligned_profile)
        # A legitimate-looking host MCP command must not inherit the terminal
        # backend's isolation, even before discovery has spawned that command.
        mcp_profile = json.loads(aligned_profile)
        mcp_profile["mcp_servers"] = {"simulated-host-file-tool": {
            "command": "python3", "args": ["simulated_mcp.py"]}}
        profile_config.write_text(json.dumps(mcp_profile))
        try:
            with patch.object(plugin.credentials, "load") as unopened:
                try:
                    plugin.credential_values()
                except ValueError as error:
                    assert str(error) == plugin.production.ERROR
                else:
                    raise AssertionError("host_mcp_accepted")
                unopened.assert_not_called()
        finally:
            profile_config.write_text(aligned_profile)
        # A cached MCP tool remains a host capability after config removal.
        registry.register(name="simulated_cached_mcp", toolset="mcp-simulated-removed",
            schema={"name": "simulated_cached_mcp", "parameters": {"type": "object"}},
            handler=lambda args: "unreachable", scope=manager.scope_key)
        try:
            try:
                plugin.production.require(task_id="pms-isolation")
            except ValueError as error:
                assert str(error) == plugin.production.ERROR
            else:
                raise AssertionError("cached_mcp_accepted")
        finally:
            registry.deregister("simulated_cached_mcp", scope=manager.scope_key)
        diagnostics = []
        def trace(frame, event, value):
            if event == "exception" and frame.f_code.co_filename == plugin.production.__file__:
                diagnostics.append({"function": frame.f_code.co_name, "line": frame.f_lineno,
                                    "exception_type": value[0].__name__})
            return trace
        sys.settrace(trace)
        try:
            plugin.production.require(task_id="pms-isolation", private_paths=(str(application_private),))
        except Exception:
            # No exception value, frame local, path contents or secret is logged.
            print(json.dumps({"simulation_policy_failure": diagnostics[-16:]}))
            raise
        finally:
            sys.settrace(None)
        assert plugin.credentials.load(str(private), profile_home=str(home)) == tokens
    info = json.loads(subprocess.check_output([env._docker_exe, "inspect", env._container_id], text=True))[0]
    root.joinpath("container-policy.json").write_text(json.dumps({
        "host": {key: info["HostConfig"].get(key) for key in ("Privileged", "PidMode", "IpcMode", "NetworkMode",
            "UsernsMode", "CapDrop", "CapAdd", "SecurityOpt", "Devices", "DeviceRequests")},
        "image": info["Config"]["Image"], "running": info["State"]["Running"], "mounts": info["Mounts"]}, indent=2))
    plugin.production.check_container(info, image=image,
        private_paths=(str(private), str(socket_path), str(application_private)))
    probe = ("import json,os,socket; from pathlib import Path; "
        f"p=Path({str(private)!r}); s=Path({str(socket_path)!r}); "
        f"print(json.dumps({{'private_visible':p.exists(),'broker_visible':s.exists(),"
        f"'application_private_visible':Path({str(application_private)!r}).exists(),"
        f"'credential_env':any(os.environ.get(k) for k in {list(tokens)!r}),"
        "'docker_socket':Path('/var/run/docker.sock').exists()}))")
    terminal = json.loads(terminal_tool("python3 -c " + shlex.quote(probe), task_id="pms-isolation"))
    root.joinpath("terminal-probe.json").write_text(json.dumps(terminal, ensure_ascii=False, indent=2))
    assert terminal.get("exit_code") == 0, "terminal_probe_failed"
    observed = json.loads(terminal["output"].strip())
    assert not any(observed.values()), "terminal_credentials_reachable"
    code = json.loads(execute_code(probe, task_id="pms-isolation", enabled_tools=[]))
    root.joinpath("python-probe.json").write_text(json.dumps(code, ensure_ascii=False, indent=2))
    assert code.get("status") == "success", "execute_code_probe_failed"
    assert not any(json.loads(code["output"].strip()).values()), "python_credentials_reachable"
    for response in (read_file_tool(str(private), task_id="pms-isolation"),
                     read_file_tool(str(application_private), task_id="pms-isolation"),
                     read_file_tool(str(socket_path), task_id="pms-isolation")):
        assert not any(value in response for value in tokens.values()), "file_tool_leak"
        assert "error" in response.lower() or "denied" in response.lower(), "file_tool_not_denied"
    ordinary = write_file_tool("/workspace/simulation.txt", "simulation preserved", task_id="pms-isolation")
    assert "error" not in ordinary.lower(), "ordinary_write_failed"
    assert "simulation preserved" in read_file_tool("/workspace/simulation.txt", task_id="pms-isolation")
    # A failed Docker environment must not cause a local command to execute.
    subprocess.run([env._docker_exe, "stop", env._container_id], capture_output=True, check=True)
    if full_production:
        try:
            plugin.production.require(task_id="pms-isolation")
        except ValueError as error:
            assert str(error) == plugin.production.ERROR
        else:
            raise AssertionError("stopped_terminal_accepted")
    host_canary = root / "must-not-exist"
    failure = json.loads(terminal_tool("touch " + shlex.quote(str(host_canary)), task_id="pms-isolation", timeout=5))
    assert not host_canary.exists(), "local_fallback_detected"
    assert failure.get("exit_code", -1) != 0 or failure.get("error"), "failure_not_reported"
    report = {"environment": "local Docker simulation", "official_core": core, "image": image,
              "plugin_discovered": True, "production_disabled": not full_production,
              "full_production_require_simulated": full_production,
              "managed_only_browser_policy_denied": full_production,
              "unreviewed_host_and_cached_mcp_denied": full_production,
              "additional_host_credential_inaccessible": not observed["application_private_visible"],
              "gateway_uid": os.getuid(), "terminal": observed,
              "execute_code": "private file, broker and credential environment inaccessible",
              "file_tools": "private access denied; ordinary workspace read/write passed",
              "skill_mounts": "absolute, traversal and symlink requests rejected",
              "backend_failure": "no host fallback", "production_acceptance": False}
    root.joinpath("evidence.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False))
finally:
    listener.close()
    socket_directory.cleanup()
    if env is not None:
        env.cleanup(force_remove=True)
        env.wait_for_cleanup(timeout=30)
