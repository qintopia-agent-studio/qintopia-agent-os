"""Offline plugin registration against a real, separately installed Hermes core.

Run with the candidate core's Python and --core-dir; never uses live profiles.
Checks discovery, listener startup and reconnect; not business delivery or config parity.
"""
from __future__ import annotations

import argparse
import asyncio
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core-dir", required=True, type=Path)
    parser.add_argument("--plugin-dir", type=Path, help="QiWe directory from the actual deploy payload")
    parser.add_argument("--entry", choices=("gateway", "cron", "standalone"), help=argparse.SUPPRESS)
    args = parser.parse_args()
    plugin = (args.plugin_dir or Path(__file__).resolve().parents[2] / "skills/qiwe").resolve(strict=True)
    if args.entry is None:
        for entry in ("gateway", "cron", "standalone"):
            subprocess.run([sys.executable, "-I", str(Path(__file__).resolve()), "--core-dir", str(args.core_dir), "--plugin-dir", str(plugin), "--entry", entry], check=True)
        return
    core = args.core_dir.resolve(strict=True)
    repo = Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory(prefix="hermes-plugin-probe-") as home:
        profile = Path(home)
        (profile / "plugins").mkdir()
        (profile / "plugins/qiwe-platform").symlink_to(plugin, target_is_directory=True)
        (profile / "config.yaml").write_text("plugins:\n  enabled: [qiwe-platform, wecom]\n")
        os.environ.clear()
        os.environ.update(HOME=home, HERMES_HOME=home, PATH="/usr/bin:/bin")
        sys.dont_write_bytecode = True
        sys.path[:0] = [str(core), str(plugin.parent)]
        original_connect = socket.socket.connect
        def loopback_only(sock, address):
            if not isinstance(address, tuple) or address[0] not in ("127.0.0.1", "::1"):
                raise RuntimeError("external network forbidden in compatibility probe")
            return original_connect(sock, address)
        with patch.object(socket.socket, "connect", loopback_only):
            # Exercise each production consumer before any direct import/registration.
            if args.entry == "gateway":
                from gateway.config import load_gateway_config
                load_gateway_config()
            elif args.entry == "cron":
                from cron.scheduler_delivery import _is_known_delivery_platform
                assert _is_known_delivery_platform("qiwe"), "cron rejected packaged QiWe"
            else:
                from tools.send_message_tool import prepare_send_message_platforms
                prepare_send_message_platforms()
            from hermes_cli.plugins import get_plugin_manager
            manager = get_plugin_manager()
            assert "qiwe" in manager._plugin_platform_names, "actual discovery did not register QiWe"
            import qiwe.adapter as qiwe
            from gateway.platforms.base import BasePlatformAdapter, MessageEvent
            from plugins.platforms.wecom import adapter as wecom

            assert qiwe.BasePlatformAdapter is BasePlatformAdapter, "QiWe fell back to local test classes"
            assert qiwe.MessageEvent is MessageEvent
            assert issubclass(qiwe.QiWeAdapter, BasePlatformAdapter)
            assert issubclass(wecom.WeComAdapter, BasePlatformAdapter)
            qiwe_adapter = qiwe.QiWeAdapter(SimpleNamespace(extra={"token": "synthetic-probe", "host": "127.0.0.1", "port": 0}))
            assert qiwe_adapter.platform.value == "qiwe"
            async def probe_listener():
                from aiohttp import ClientSession
                with patch.object(qiwe_adapter._reminder_worker, "start"):
                    for reconnect in (False, True):
                        try:
                            assert await qiwe_adapter.connect(is_reconnect=reconnect), "listener startup failed"
                            port = qiwe_adapter._runner.addresses[0][1]
                            async with ClientSession() as client:
                                async with client.get(f"http://127.0.0.1:{port}/health") as response:
                                    assert response.status == 200, "listener health failed"
                        finally:
                            await qiwe_adapter.disconnect()
                        assert qiwe_adapter._runner is None, "listener cleanup failed"
            asyncio.run(probe_listener())
            print(f"real_core_plugin_discovery={args.entry}:passed listener_and_reconnect=passed delivery=not_exercised")


if __name__ == "__main__":
    main()
