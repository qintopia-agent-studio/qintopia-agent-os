"""Restart the existing Anan gateway only after an official reversible drain.

No core/Profile rewrite, lock deletion, forced timeout restart, or task replay.
"""
from __future__ import annotations

import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
from uuid import uuid4

CORE = Path("/home/ubuntu/.local/share/hermes-releases/v2026.9.21")
PROFILE = Path("/home/ubuntu/.hermes/profiles/anan")
SERVICE = "hermes-gateway-anan.service"


class Deferred(Exception):
    pass


def drain_and_restart(control, snapshot, restart, *, home, pid,
                      clock=time.monotonic, sleep=time.sleep, timeout=600):
    if control.read_drain_request(home=home) is not None:
        raise Deferred("existing_drain")
    principal = "agent-os-release-" + uuid4().hex
    marker = None
    zero_since = None
    try:
        marker = control.write_drain_request(
            home=home, principal=principal, suppress_notification=True,
        )
        deadline = clock() + timeout
        while clock() < deadline:
            if control.read_drain_request(home=home) != marker:
                raise Deferred("drain_changed")
            record = snapshot()
            if not record:
                zero_since = None
                sleep(1)
                continue
            if record.get("pid") != pid:
                raise Deferred("gateway_identity_changed")
            count = record.get("active_agents")
            ready = (record.get("gateway_state") == "draining"
                     and type(count) is int and count == 0
                     and isinstance(record.get("updated_at"), str))
            if ready:
                # The watcher refreshes aggregate chat/cron/API/deferred work every
                # second. Require two observations rather than the transition alone.
                if (zero_since is not None and clock() - zero_since[0] >= 2
                        and record["updated_at"] != zero_since[1]):
                    restart()
                    return
                if zero_since is None:
                    zero_since = (clock(), record["updated_at"])
            else:
                zero_since = None
            sleep(1)
        raise Deferred("active_work_timeout")
    finally:
        # Never clear an operator's replacement marker.
        current = control.read_drain_request(home=home)
        owned = (marker is not None and current == marker) or (
            marker is None and isinstance(current, dict) and current.get("principal") == principal)
        if owned:
            if not control.clear_drain_request(home=home):
                raise Deferred("drain_cleanup_failed")


def systemctl(*args):
    return subprocess.run(
        ["systemctl", "--user", *args], check=True, timeout=120,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    ).stdout.strip()


def main():
    expected_python = str(CORE / "venv/bin/python")
    # Do not silently import a drifting core or act on a differently owned unit.
    if sys.executable != expected_python:
        raise Deferred("interpreter_mismatch")
    start = systemctl("show", SERVICE, "--property=ExecStart", "--value")
    match = re.search(r"\bpath=([^ ;]+)", start)
    if not match or match[1] != expected_python:
        raise Deferred("service_entrypoint_mismatch")
    if systemctl("show", SERVICE, "--property=WorkingDirectory", "--value") != str(PROFILE):
        raise Deferred("profile_mismatch")
    pid = int(systemctl("show", SERVICE, "--property=MainPID", "--value"))
    if pid <= 0:
        raise Deferred("gateway_not_running")
    os.environ["HERMES_HOME"] = str(PROFILE)
    sys.path.insert(0, str(CORE))
    from gateway import drain_control, status

    if drain_control.drain_request_path(home=PROFILE).exists():
        raise Deferred("existing_drain")

    def snapshot():
        record = status.read_runtime_status(PROFILE / "gateway_state.json")
        if status.runtime_status_is_stale(record, ttl_s=5) or not status.runtime_status_pid_is_live(record):
            return None
        return record

    def restart():
        if int(systemctl("show", SERVICE, "--property=MainPID", "--value")) != pid:
            raise Deferred("gateway_identity_changed")
        systemctl("restart", SERVICE)
        systemctl("is-active", "--quiet", SERVICE)

    drain_and_restart(drain_control, snapshot, restart, home=PROFILE, pid=pid)
    print("anan_gateway_restart=ready")


if __name__ == "__main__":
    def interrupted(*_):
        raise Deferred("restart_interrupted")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        main()
    except Exception:
        # No unit argv, filesystem contents, status body, or subprocess stderr.
        print("anan_gateway_restart=deferred_or_failed", file=sys.stderr)
        sys.exit(1)
