#!/usr/bin/env python3
"""Stable Hermes scripts-directory entrypoint for the release-managed bridge."""

from __future__ import annotations

import os
import runpy
import sys


BRIDGE = "/home/ubuntu/qintopia-agent-os-releases/current/workflows/silaoshi-daily-ops/bin/script_action_bridge.py"
CONFIG = "/etc/qintopia/silaoshi-script-action.json"

os.environ["QINTOPIA_SILAOSHI_SCRIPT_ACTION_CONFIG"] = CONFIG
sys.argv = [BRIDGE, "transform", "--config", CONFIG]
runpy.run_path(BRIDGE, run_name="__main__")
