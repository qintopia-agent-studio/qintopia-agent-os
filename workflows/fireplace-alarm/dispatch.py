"""Deliver pending notifications through the designated Hermes profile."""

import argparse
import json
import logging
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path
from zoneinfo import ZoneInfo

# systemd uses isolated Python. This directory is part of the root-owned release.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import state


TEMPLATES = Path(__file__).resolve().parents[2] / "skills/qintopia-tuya-sensors/references/alarm-messages.json"
ROUTES = Path("/etc/qintopia-tuya-sensors/alarm-routes.json")
HAZARDS = {1: "烟雾", 2: "一氧化碳", 3: "烟雾和一氧化碳"}
PLATFORMS = {"silaoshi": "wecom", "erhua": "qiwe"}


def render(job, templates):
    hazard = HAZARDS[job["hazards"]]
    stamp = datetime.fromtimestamp(job["reported_at"] / 1000, ZoneInfo("Asia/Shanghai"))
    reported = stamp.strftime("%Y-%m-%d %H:%M:%S") + "（北京时间）"
    if job["kind"] == "recovery":
        return "设备状态更新｜一楼壁炉区\n\n" + templates["恢复"].format(time=reported, hazard=hazard)
    header = hazard + ("同时报警" if job["hazards"] == 3 else "报警")
    parts = [f"🚨 紧急撤离｜一楼壁炉区{header}", templates[hazard]]
    if job["profile"] == "silaoshi":
        parts.append(templates["当班同事"])
    parts.append(f"设备上报时间 {reported}｜第 {job['ordinal']}/3 次提醒")
    return "\n\n".join(parts)


def send(job, target, templates, run=subprocess.run):
    profile = job["profile"]
    command = ["/usr/local/bin/hermes", "-p", profile, "send", "--json", "--to",
               target, render(job, templates)]
    try:
        result = run(command, capture_output=True, text=True, timeout=24, check=False)
    except OSError as err:
        return "failed", type(err).__name__
    except subprocess.TimeoutExpired:
        return "unknown", "timeout"
    try:
        reply = json.loads(result.stdout)
    except ValueError:
        return "unknown", f"invalid-receipt/exit-{result.returncode}"
    if not isinstance(reply, dict) or reply.get("success") is not True:
        return "unknown", f"unconfirmed/exit-{result.returncode}"
    if profile == "silaoshi":
        if (reply.get("platform") == "wecom" and reply.get("chat_id") == target.split(":", 1)[1]
                and reply.get("message_id")):
            return "accepted", str(reply["message_id"])
    else:
        response = reply.get("raw_response")
        if isinstance(response, dict) and response.get("code") == 0:
            data = response.get("data")
            if isinstance(data, dict) and data.get("isSendSuccess") == 1 and data.get("msgServerId"):
                return "accepted", str(data["msgServerId"])
    return "unknown", "receipt-mismatch"


def newer_normals(active, run=subprocess.run):
    try:
        result = run(["/usr/local/bin/qintopia-sensors", "read", "烟雾报警器"],
                     capture_output=True, text=True, timeout=12, check=True)
        rows = json.loads(result.stdout)["devices"]
        device, = [d for d in rows if d["name"] == "烟雾报警器" and d["area"] == "一楼壁炉区"]
        points = []
        for reading in device["readings"]:
            code = reading["code"]
            if (state.SIGNALS.get(code, 0) & active and reading.get("status") == "ok"
                    and reading.get("value") == "normal"):
                stamp = datetime.fromisoformat(reading["reported_at"])
                if stamp.tzinfo:
                    points.append({"code": code, "value": "normal", "time": int(stamp.timestamp()) * 1000})
        return points
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError) as err:
        logging.warning("could not check a delayed alarm against cloud status: %s", type(err).__name__)
        return []


def serve(profile):
    templates = json.loads(TEMPLATES.read_text())
    target = json.loads(ROUTES.read_text())[profile]
    platform, destination = target.split(":", 1)
    if platform != PLATFORMS[profile] or not destination:
        raise ValueError("incorrect profile destination")
    db = state.connect()
    logging.info("ready profile=%s", profile)
    while True:
        job = state.claim(db, profile, state.now_ms(), verify_late=newer_normals)
        if job is None:
            time.sleep(0.25)
            continue
        status, receipt = send(job, target, templates)
        state.finish(db, job, status, receipt)
        level = logging.INFO if status == "accepted" else logging.ERROR
        logging.log(level, "incident=%d profile=%s kind=%s slot=%d status=%s",
                    job["incident"], profile, job["kind"], job["ordinal"], status)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("profile", choices=state.PROFILES)
    parser.add_argument("--deliver", action="store_true", help="deliver via the host's configured Hermes route")
    args = parser.parse_args()
    if not args.deliver:
        parser.error("No sender started. Run the offline tests, or explicitly enable reviewed delivery with --deliver.")
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    serve(args.profile)
