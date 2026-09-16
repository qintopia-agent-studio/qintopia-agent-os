"""Probe real official WeCom send semantics against a synthetic transport.

No credentials, live profiles, sockets or recipients are used. An accepted send whose
acknowledgement is lost must not trigger a second delivery through the fallback path.
"""
from __future__ import annotations

import argparse
import asyncio
import os
from pathlib import Path
import socket
import sys
import tempfile
from unittest.mock import AsyncMock, patch


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core-dir", required=True, type=Path)
    args = parser.parse_args()
    core = args.core_dir.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="hermes-wecom-probe-") as home:
        os.environ.clear()
        os.environ.update(HOME=home, HERMES_HOME=home, PATH="/usr/bin:/bin")
        sys.path.insert(0, str(core))
        sys.dont_write_bytecode = True
        with patch.object(socket.socket, "connect", side_effect=RuntimeError("network forbidden")):
            from gateway.config import PlatformConfig
            from plugins.platforms.wecom.adapter import WeComAdapter, STREAM_NOT_SUBSCRIBED_ERRCODE

            async def probe():
                adapter = WeComAdapter(PlatformConfig(extra={}))
                adapter._last_chat_req_ids["fixture-chat"] = "fixture-request"
                adapter._reply_req_ids["fixture-message"] = "fixture-request"
                await adapter._force_reconnect_on_stale_subscription(STREAM_NOT_SUBSCRIBED_ERRCODE)
                cleared = not adapter._last_chat_req_ids and not adapter._reply_req_ids
                print("wecom_stale_request_clear=" + ("passed" if cleared else "blocked"))
                adapter._last_chat_req_ids["fixture-chat"] = "fixture-request"
                deliveries = []
                async def accepted_without_ack(*args, **kwargs):
                    deliveries.append("passive")
                    raise asyncio.TimeoutError("fixture acknowledgement lost after acceptance")
                async def fallback(*args, **kwargs):
                    deliveries.append("proactive")
                    return {"errcode": 0}
                adapter._send_reply_markdown = AsyncMock(side_effect=accepted_without_ack)
                adapter._send_proactive_markdown = AsyncMock(side_effect=fallback)
                await adapter._send_inner("fixture-chat", "synthetic fixture")
                safe = len(deliveries) == 1
                print("wecom_uncertain_send=" + ("passed" if safe else "blocked_duplicate_effect_possible"))
                return cleared and safe
            passed = asyncio.run(probe())
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
