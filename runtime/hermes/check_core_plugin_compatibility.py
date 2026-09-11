"""Offline plugin registration against a real, separately installed Hermes core.

Run with the candidate core's Python and --core-dir; never uses live profiles.
This checks import/registration contracts, not message delivery or configuration parity.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import socket
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core-dir", required=True, type=Path)
    args = parser.parse_args()
    core = args.core_dir.resolve(strict=True)
    repo = Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory(prefix="hermes-plugin-probe-") as home:
        os.environ.clear()
        os.environ.update(HOME=home, HERMES_HOME=home, PATH="/usr/bin:/bin")
        sys.dont_write_bytecode = True
        sys.path[:0] = [str(core), str(repo / "skills")]
        with patch.object(socket.socket, "connect", side_effect=RuntimeError("network forbidden in compatibility probe")):
            import qiwe.adapter as qiwe
            from gateway.platforms.base import BasePlatformAdapter, MessageEvent
            from hermes_cli.plugins import PluginContext, PluginManager, PluginManifest
            from plugins.platforms.wecom import adapter as wecom

            assert qiwe.BasePlatformAdapter is BasePlatformAdapter, "QiWe fell back to local test classes"
            assert qiwe.MessageEvent is MessageEvent
            assert issubclass(qiwe.QiWeAdapter, BasePlatformAdapter)
            assert issubclass(wecom.WeComAdapter, BasePlatformAdapter)
            manager = PluginManager()
            for name, module in (("qiwe-platform", qiwe), ("wecom", wecom)):
                context = PluginContext(PluginManifest(name=name, kind="platform"), manager)
                module.register(context)
            assert {"qiwe", "wecom", "wecom_callback"} <= manager._plugin_platform_names
            qiwe_adapter = qiwe.QiWeAdapter(SimpleNamespace(extra={}))
            assert qiwe_adapter.platform.value == "qiwe"
            print("real_core_qiwe_wecom_registration=passed")


if __name__ == "__main__":
    main()
