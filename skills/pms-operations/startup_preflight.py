"""Credential-free checks for a reviewed, read-only Anan plugin installation.

The service owner must run this in the Gateway's mount namespace before it can
receive events, then check live registration in that same process. This command
alone cannot enforce the official Gateway startup/reload ordering.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import os
from pathlib import Path
import re
import stat


ERROR = "pms_startup_preflight_failed"
RUNTIME_FILES = frozenset({
    "__init__.py", "application_intake.py", "client.py", "credentials.py", "host.py",
    "payment_feed.py", "production.py", "recovery_host.py", "reminder_host.py",
    "startup_preflight.py", "stay_contacts_host.py", "workitem_wake.py", "operations.json", "plugin.yaml",
    "manifest.yaml", "SKILL.md", "README.md",
})
EXPECTED_TOOLS = frozenset({
    "qintopia_workitem_read", "qintopia_pms_context", "qintopia_pms_read",
    "qintopia_pms_prepare", "qintopia_pms_execute", "qintopia_pms_recover",
    "qintopia_pms_status", "qintopia_pms_pause", "qintopia_pms_resume",
    "qintopia_pms_cancel", "qintopia_pms_handoff", "qintopia_pms_reconcile",
    "qintopia_pms_link", "qintopia_pms_reminder_snooze",
})
EXPECTED_HOOKS = {"pre_gateway_dispatch": 1, "pre_tool_call": 2}


def _directory(path, *, required_uid=0):
    path = Path(path)
    if not path.is_absolute() or ".." in path.parts or path.is_symlink():
        raise ValueError(ERROR)
    info = path.lstat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != required_uid
            or stat.S_IMODE(info.st_mode) & 0o022):
        raise ValueError(ERROR)
    return path


def fingerprint(directory, *, required_uid=0):
    """Hash a fixed inventory and reject extra loadable files or nested plugins."""
    root = _directory(directory, required_uid=required_uid)
    names = {entry.name for entry in root.iterdir()}
    if not RUNTIME_FILES <= names or any(
            name not in RUNTIME_FILES and name != "tests" and
            (name.endswith((".py", ".yaml", ".json")) or (root / name).is_dir())
            for name in names):
        raise ValueError(ERROR)
    digest = hashlib.sha256()
    for name in sorted(RUNTIME_FILES):
        path = root / name
        info = path.lstat()
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != required_uid
                or stat.S_IMODE(info.st_mode) & 0o022 or info.st_nlink != 1
                or info.st_size > 1024 * 1024):
            raise ValueError(ERROR)
        digest.update(name.encode() + b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def readonly(path, *, required_uid=0):
    candidate = Path(path)
    if not candidate.is_absolute() or ".." in candidate.parts or candidate.is_symlink():
        raise ValueError(ERROR)
    info = candidate.lstat()
    if (not (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode))
            or info.st_uid != required_uid or stat.S_IMODE(info.st_mode) & 0o022):
        raise ValueError(ERROR)
    if not os.statvfs(candidate).f_flag & os.ST_RDONLY:
        raise ValueError(ERROR)


def check_static(*, installed, release, expected_sha256, profile_config, managed_directory,
                 managed_check=None, required_uid=0):
    if not isinstance(expected_sha256, str) or not re.fullmatch(r"[a-f0-9]{64}", expected_sha256):
        raise ValueError(ERROR)
    installed = _directory(installed, required_uid=required_uid)
    release = _directory(release, required_uid=required_uid)
    readonly(installed, required_uid=required_uid)
    readonly(profile_config, required_uid=required_uid)
    if (fingerprint(release, required_uid=required_uid) != expected_sha256
            or fingerprint(installed, required_uid=required_uid) != expected_sha256):
        raise ValueError(ERROR)
    spec = importlib.util.spec_from_file_location(
        "qintopia_pms_preflight_production", release / "production.py")
    if spec is None or spec.loader is None:
        raise ValueError(ERROR)
    production = importlib.util.module_from_spec(spec)
    # Load only the reviewed source: importlib's loader can write __pycache__
    # into the release and make a subsequent fixed-inventory check fail.
    source = release / "production.py"
    exec(compile(source.read_bytes(), str(source), "exec"), production.__dict__)
    if managed_check is None:
        managed_check = production.check_managed_files
    managed_check(managed_directory)
    import yaml
    def read_config(path):
        candidate = Path(path)
        if candidate.stat().st_size > 1024 * 1024:
            raise ValueError(ERROR)
        value = yaml.safe_load(candidate.read_text(encoding="utf-8"))
        return {} if value is None else value
    production.check_shell_hooks(read_config(profile_config))
    production.check_shell_hooks(read_config(Path(managed_directory) / "config.yaml"))


def check_loaded_plugin(manager, registry, installed):
    """Inspect the actual manager, not a separately imported plugin module."""
    loaded = getattr(manager, "_plugins", {}).get("pms-operations")
    if (loaded is None or loaded.enabled is not True or loaded.error
            or loaded.deferred or loaded.module is None):
        raise ValueError(ERROR)
    path = Path(getattr(loaded.module, "__file__", ""))
    manifest = Path(getattr(loaded.manifest, "path", ""))
    if (path != Path(installed) / "__init__.py" or manifest != Path(installed)
            or set(loaded.tools_registered) != EXPECTED_TOOLS):
        raise ValueError(ERROR)
    try:
        loaded.module.production.check_active_shell_hooks(manager)
    except Exception:
        raise ValueError(ERROR) from None
    hooks = list(loaded.hooks_registered)
    if any(hooks.count(name) != count for name, count in EXPECTED_HOOKS.items()) or len(hooks) != 3:
        raise ValueError(ERROR)
    expected_sources = {
        "pre_gateway_dispatch": {str(Path(installed) / "__init__.py"): 1},
        "pre_tool_call": {str(Path(installed) / "__init__.py"): 1,
                          str(Path(installed) / "production.py"): 1},
    }
    for hook, sources in expected_sources.items():
        active = getattr(manager, "_hooks", {}).get(hook, [])
        for source, count in sources.items():
            if sum(getattr(getattr(callback, "__code__", None), "co_filename", None) == source
                   for callback in active) != count:
                raise ValueError(ERROR)
    for name in EXPECTED_TOOLS:
        entry = registry.get_entry(name, scope=manager.scope_key)
        if (entry is None or getattr(getattr(entry.handler, "__code__", None), "co_filename", None)
                != str(Path(installed) / "__init__.py")):
            raise ValueError(ERROR)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("installed", "release", "expected-sha256", "profile-config", "managed-directory"):
        parser.add_argument("--" + name, required=True)
    args = vars(parser.parse_args())
    try:
        check_static(installed=args["installed"], release=args["release"],
                     expected_sha256=args["expected_sha256"],
                     profile_config=args["profile_config"],
                     managed_directory=args["managed_directory"])
    except Exception:
        raise SystemExit(ERROR) from None
    print("pms_startup_static_ready")


if __name__ == "__main__":
    main()
