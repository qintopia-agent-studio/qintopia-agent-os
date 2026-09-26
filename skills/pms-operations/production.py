"""Explicit production mode and the official Docker tool boundary.

No credentials are exported to Hermes' secret registry or subprocess environment.
This module verifies the effective backend AND the running container. Deployment
must additionally verify plugin loading, immutable host configuration and the
operator's chosen tool policy before installing a private credentials file.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
from urllib.parse import urlsplit


SECRETS = frozenset({"GREENPMS_API_TOKEN", "QINTOPIA_FOUNDATION_TOKEN",
                     "QINTOPIA_FOUNDATION_HOST_TOKEN"})
LOCAL_KEYS = ("QINTOPIA_PMS_LOCAL_ENABLE", "QINTOPIA_FOUNDATION_LOCAL_ENABLE")
PRODUCTION_KEYS = ("QINTOPIA_PMS_PRODUCTION_ENABLE", "QINTOPIA_FOUNDATION_PRODUCTION_ENABLE")
ERROR = "pms_isolation_unavailable"


def mode(environ=None):
    env = os.environ if environ is None else environ
    local = [env.get(key) == "1" for key in LOCAL_KEYS]
    production = [env.get(key) == "1" for key in PRODUCTION_KEYS]
    if any(local) and any(production):
        return "disabled"
    if all(local):
        return "local"
    if all(production):
        return "production"
    return "disabled"


def requested():
    return any(os.environ.get(key) == "1" for key in PRODUCTION_KEYS)


def check_configuration(config, *, credentials_file, profile_home, secret_values):
    """Validate effective (scope-aware) official terminal config, not raw YAML."""
    if config.get("env_type") != "docker":
        raise ValueError(ERROR)
    if not re.fullmatch(r"[^\s]+@sha256:[a-f0-9]{64}", config.get("docker_image", "")):
        raise ValueError(ERROR)
    # None of these may smuggle host files, privileged flags or profile overrides.
    forbidden = ("docker_volumes", "docker_extra_args", "docker_mount_cwd_to_workspace",
                 "docker_forward_env", "docker_env", "docker_shared_container_key",
                 "docker_snap_compat", "docker_run_as_host_user")
    if any(config.get(key) for key in forbidden) or any(secret_values.get(key) for key in SECRETS):
        raise ValueError(ERROR)
    private = Path(credentials_file)
    profile = Path(profile_home).resolve()
    # Official host media/credential guards recognize .env everywhere. A custom
    # JSON basename is not covered by those host tool guards.
    if (not private.is_absolute() or private.name != ".env" or ".." in private.parts
            or private.resolve().is_relative_to(profile)):
        raise ValueError(ERROR)


def check_container(info, *, image, private_paths):
    """Check Docker's actual isolation metadata; no values are returned/logged."""
    host = info.get("HostConfig", {})
    security = host.get("SecurityOpt") or []
    capabilities = {str(x).upper().removeprefix("CAP_") for x in host.get("CapAdd") or []}
    if (not info.get("State", {}).get("Running")
            or info.get("Config", {}).get("Image") != image
            or host.get("Privileged") or host.get("Devices") or host.get("DeviceRequests")
            or host.get("PidMode") or host.get("IpcMode") in {"host"}
            or host.get("NetworkMode") not in {"bridge", "default", "none"}
            or host.get("UsernsMode") == "host"
            or "ALL" not in {str(x).upper() for x in host.get("CapDrop") or []}
            or not capabilities <= {"DAC_OVERRIDE", "CHOWN", "FOWNER", "SETUID", "SETGID"}
            or not any(x.split(":")[0] == "no-new-privileges" for x in security)
            or any("unconfined" in x for x in security)):
        raise ValueError(ERROR)
    if any(x.split("=", 1)[0] in SECRETS for x in info.get("Config", {}).get("Env") or []):
        raise ValueError(ERROR)
    protected = [Path(x).resolve() for x in private_paths]
    for mount in info.get("Mounts") or []:
        source = Path(mount.get("Source", "/")).resolve()
        destination = Path(mount.get("Destination", "/"))
        if (mount.get("Type") != "bind" or not source.is_absolute()
                or str(destination) in {"/", "/proc", "/sys", "/dev", "/var/run/docker.sock"}
                or source == Path("/") or source.is_relative_to("/proc")
                or source.is_relative_to("/sys") or source.is_relative_to("/dev")
                or source.name.endswith(".sock")
                or any(p == source or p.is_relative_to(source) for p in protected)):
            raise ValueError(ERROR)
        # Official Docker may mount only its workspace/home and read-only skill,
        # credential and media caches. Extra arbitrary mounts fail closed.
        if destination in (Path("/workspace"), Path("/root")):
            if source.name not in {"workspace", "home"} or "sandboxes" not in source.parts:
                raise ValueError(ERROR)
        elif (not destination.is_relative_to("/root/.hermes") or mount.get("RW")):
            raise ValueError(ERROR)


def browser_policy(managed, effective, *, cdp_override):
    """Pin the official CDP backend to an independently inspected container."""
    policy = managed.get("qintopia_pms_isolation", {}).get("browser", {})
    browser = effective.get("browser", {})
    if (not isinstance(policy, dict) or set(policy) != {"container", "image", "cdp_url"}
            or not isinstance(browser, dict)
            or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]{0,127}", policy.get("container", ""))
            or not re.fullmatch(r"[^\s]+@sha256:[a-f0-9]{64}", policy.get("image", ""))):
        raise ValueError(ERROR)
    url = urlsplit(policy["cdp_url"])
    if (url.scheme != "http" or url.hostname != "127.0.0.1" or not url.port
            or policy["cdp_url"] != f"http://127.0.0.1:{url.port}"
            or cdp_override != policy["cdp_url"]
            or browser.get("cdp_url") != policy["cdp_url"]
            # Official 'off' disables browser_use Python, retaining browser_*.
            or browser.get("backend") not in (False, "off")
            or browser.get("auto_local_for_private_urls") is not False
            or browser.get("use_real_profile") is not False
            or browser.get("allow_private_urls") is not False
            or browser.get("extension_control", {}).get("enabled") is not False):
        raise ValueError(ERROR)
    return policy, url.port


def check_browser_endpoint(endpoint, port):
    parsed = urlsplit(endpoint)
    if (parsed.scheme != "ws" or parsed.netloc != f"127.0.0.1:{port}"
            or parsed.query or parsed.fragment
            or not re.fullmatch(r"/devtools/browser/[a-zA-Z0-9_-]{1,128}", parsed.path)):
        raise ValueError(ERROR)


def check_browser_container(info, image_info, *, policy, port):
    # The browser may have private ephemeral tmpfs storage, but no host binds,
    # named volumes, credentials or sockets. Reuse the terminal's namespace and
    # capability checks; do not add an exception to that tool's mount policy.
    for mount in info.get("Mounts") or []:
        if (mount.get("Type") != "tmpfs"
                or mount.get("Destination") not in {"/tmp", "/dev/shm", "/home/browser"}):
            raise ValueError(ERROR)
    check_container({**info, "Mounts": []}, image=policy["image"], private_paths=())
    if (info.get("Image") != image_info.get("Id") or not image_info.get("Id")
            or sorted(info.get("Config", {}).get("Env") or [])
            != sorted(image_info.get("Config", {}).get("Env") or [])
            or info.get("HostConfig", {}).get("PublishAllPorts")):
        raise ValueError(ERROR)
    # Check Docker's actual published ports, not just the requested port binding.
    ports = info.get("NetworkSettings", {}).get("Ports", {})
    published = [(key, value) for key, value in ports.items() if value]
    if (len(published) != 1 or not re.fullmatch(r"[0-9]{1,5}/tcp", published[0][0])
            or not 1 <= int(published[0][0].split("/")[0]) <= 65535
            or published[0][1] != [{"HostIp": "127.0.0.1", "HostPort": str(port)}]):
        raise ValueError(ERROR)


def require_browser(managed, docker_exe):
    """Validate the effective official routing before any private token is read.

    No browser is launched or reconfigured here. A missing browser, fallback,
    stale host session, changed endpoint or uninspected container is a denial.
    """
    from hermes_cli.config import load_config_readonly, read_raw_config
    from tools import browser_tool as browser
    from tools import browser_tool_cdp as cdp
    from tools import browser_tool_cloud as cloud
    from gateway.browser_control_broker import browser_control_enabled
    policy, port = browser_policy(managed, load_config_readonly(),
                                  cdp_override=cdp._get_cdp_override_raw())
    # The reviewed core's browser helpers still read raw Profile settings.
    # Managed policy alone therefore cannot prove their effective routing.
    # Require both views to agree; never edit the live Profile from a tool call.
    browser_policy(managed, read_raw_config(), cdp_override=cdp._get_cdp_override_raw())
    if (browser._is_browser_use_cli_mode() or browser._is_camofox_mode()
            or browser_control_enabled() or cloud._auto_local_for_private_urls()
            or cloud._use_real_profile() or cloud._allow_private_urls()
            or cloud._get_browser_engine() == "lightpanda"):
        raise ValueError(ERROR)
    def inspect(kind, reference):
        result = subprocess.run([docker_exe, "inspect", "--type", kind, reference],
                                capture_output=True, text=True, timeout=5, check=True)
        objects = json.loads(result.stdout)
        if not isinstance(objects, list) or len(objects) != 1:
            raise ValueError(ERROR)
        return objects[0]
    check_browser_container(inspect("container", policy["container"]),
                            inspect("image", policy["image"]), policy=policy, port=port)
    check_browser_endpoint(cdp._get_cdp_override(), port)
    # A pre-existing local/cloud session is otherwise reused ahead of a new CDP
    # override. Never mutate core caches to disguise such an installation error.
    with browser._cleanup_lock:
        for key, session in browser._active_sessions.items():
            if browser._is_local_sidecar_key(key) or not session.get("features", {}).get("cdp_override"):
                raise ValueError(ERROR)
            check_browser_endpoint(session.get("cdp_url", ""), port)


def require(*, task_id="default", private_paths=()):
    """Verify before credentials are opened or business network calls start.

    Uses the same official environment cache as terminal/file/execute_code.
    The trusted Docker daemon metadata is checked on each call; there is no
    reusable readiness boolean and no fallback to local execution.
    """
    try:
        if (not isinstance(private_paths, (tuple, list))
                or any(not isinstance(path, str) or not Path(path).is_absolute()
                       for path in private_paths)):
            raise ValueError(ERROR)
        if mode() != "production":
            raise ValueError(ERROR)
        if os.environ.get("QINTOPIA_PMS_TOOL_POLICY") != "docker-v1":
            raise ValueError(ERROR)
        from hermes_constants import get_hermes_home
        from hermes_cli.profiles import get_active_profile_name
        from agent.secret_scope import get_secret
        from tools.terminal_tool import _get_env_config
        from tools.code_execution_tool import _get_or_create_env
        from tools.environments.docker import DockerEnvironment
        from gateway.media_policy import media_delivery_strict, media_delivery_trust_recent
        if get_active_profile_name() != "anan":
            raise ValueError(ERROR)
        profile = str(get_hermes_home())
        private = os.environ.get("QINTOPIA_PMS_CREDENTIALS_FILE", "")
        socket_path = os.environ.get("QINTOPIA_FOUNDATION_SOCKET", "")
        if not Path(socket_path).is_absolute():
            raise ValueError(ERROR)
        config = _get_env_config()
        managed = check_host_files(profile)
        check_configuration(config, credentials_file=private, profile_home=profile,
                            secret_values={key: os.environ.get(key) or get_secret(key, "") for key in SECRETS})
        if not media_delivery_strict() or media_delivery_trust_recent():
            raise ValueError(ERROR)
        env, backend = _get_or_create_env(task_id)
        if backend != "docker" or type(env) is not DockerEnvironment:
            raise ValueError(ERROR)
        container_id = env._container_id
        if not isinstance(container_id, str) or not re.fullmatch(r"[a-f0-9]{12,64}", container_id):
            raise ValueError(ERROR)
        result = subprocess.run([env._docker_exe, "inspect", container_id], capture_output=True,
                                text=True, timeout=5, check=True)
        objects = json.loads(result.stdout)
        if not isinstance(objects, list) or len(objects) != 1:
            raise ValueError(ERROR)
        check_container(objects[0], image=config["docker_image"],
                        private_paths=(*private_paths, private, socket_path, str(Path(profile) / ".env"),
                                       str(Path(profile) / "config.yaml"), str(Path(profile) / "plugins")))
        require_browser(managed, env._docker_exe)
    except Exception:
        # Never expose Docker inspect output, paths or arbitrary library errors.
        raise ValueError(ERROR) from None


def check_managed_files(directory):
    """Validate the complete root-owned path before loading private policy.

    A root-owned leaf under a writable parent can still be replaced wholesale.
    The final directory and file contain a private webhook key; other users may
    not read them. The service's protected group may read without writing.
    """
    import stat
    directory = Path(directory)
    if not directory.is_absolute() or ".." in directory.parts:
        raise ValueError(ERROR)
    for path in reversed((directory, *directory.parents)):
        info = path.lstat()
        if (not stat.S_ISDIR(info.st_mode) or info.st_uid != 0
                or stat.S_IMODE(info.st_mode) & 0o022):
            raise ValueError(ERROR)
    if stat.S_IMODE(directory.stat().st_mode) & 0o007:
        raise ValueError(ERROR)
    info = (directory / "config.yaml").lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1
            or stat.S_IMODE(info.st_mode) & 0o027):
        raise ValueError(ERROR)


def check_host_files(profile_home):
    """The preflight may not ignore an existing host cron script or plugin change.

    Official managed scope pins backend policy without replacing the live Profile
    config. Its files must be outside model-writable directories. A missing or
    malformed overlay is an error here even though the core tolerates it.
    """
    from hermes_cli.managed_scope import get_managed_dir, load_managed_config
    from hermes_cli.config import load_config_readonly
    from hermes_cli.plugins import get_plugin_manager
    from tools.registry import registry
    directory = get_managed_dir()
    if directory is None:
        raise ValueError(ERROR)
    check_managed_files(directory)
    managed = load_managed_config()
    effective = load_config_readonly()
    # MCP stdio does not use the terminal backend. Do not discover/connect MCP
    # here: inspect configuration plus the already-loaded plugin/registry state.
    check_mcp_boundary(effective.get("mcp_servers"),
                       get_plugin_manager().get_portable_mcp_servers(),
                       registry.get_registered_toolset_names())
    if (not managed or managed.get("skills", {}).get("inline_shell") is not False
            or effective.get("skills", {}).get("inline_shell") is not False
            or os.environ.get("HERMES_ENABLE_PROJECT_PLUGINS", "").lower() in {"1", "true", "yes"}):
        raise ValueError(ERROR)
    home = Path(profile_home)
    scripts = home / "scripts"
    if scripts.exists() and (scripts.is_symlink() or any(scripts.iterdir())):
        raise ValueError(ERROR)
    jobs = home / "cron" / "jobs.json"
    if jobs.exists():
        value = json.loads(jobs.read_text())
        entries = value if isinstance(value, list) else value.get("jobs", [])
        if isinstance(entries, dict):
            entries = list(entries.values())
        if not isinstance(entries, list) or any(
                not isinstance(job, dict) or job.get("script") or job.get("monitor_script")
                or job.get("no_agent") for job in entries):
            raise ValueError(ERROR)
    return managed


def check_mcp_boundary(configured, portable, registered_toolsets):
    """An unreviewed host MCP cannot inherit the Docker isolation conclusion.

    The observed Anan baseline has no MCP. Preserve its configuration; a later
    addition needs a verified execution boundary before production PMS starts.
    Cached tools are checked too, even if their source config has been removed.
    """
    for entries in (configured, portable):
        if entries is None:
            continue
        if not isinstance(entries, dict) or any(
                not isinstance(config, dict) or config.get("enabled") is not False
                for config in entries.values()):
            raise ValueError(ERROR)
    if any(not isinstance(name, str) or name.startswith("mcp-") for name in registered_toolsets):
        raise ValueError(ERROR)


def guard(tool_name, args, *, task_id="default", **_):
    """Official pre_tool_call hook; return a denial on every failure.

    Dormant outside explicitly selected production policy. This does not revoke
    ordinary cron prompts or terminal/file/Python capabilities in the sandbox.
    """
    if not requested():
        return None
    try:
        require(task_id=task_id or "default")
        if tool_name == "browser_exec":
            raise ValueError(ERROR)
        if tool_name in {"cronjob", "cronjob_manage"}:
            if not isinstance(args, dict):
                raise ValueError(ERROR)
            monitor = args.get("monitor")
            if (args.get("script") or args.get("monitor_script") or args.get("no_agent")
                    or (monitor and not str(monitor).startswith(("https://", "http://")))):
                raise ValueError(ERROR)
        return None
    except Exception:
        return {"action": "block", "message": ERROR}
