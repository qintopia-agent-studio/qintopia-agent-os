#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HERMES_CRON_LIVE_PARITY_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Hermes cron live parity observation skipped: set QINTOPIA_HERMES_CRON_LIVE_PARITY_OBSERVATION_ENABLE=1" >&2
  exit 0
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
export PATH

PYTHON_BIN="/usr/bin/python3"
REGISTRY_FILE="/home/ubuntu/qintopia-agent-os-releases/current/runtime/hermes/cron/reviewed-cron-jobs.json"
XIAOMAN_CRON_FILE="/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json"
XIAOMAN_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"
ERHUA_CRON_FILE="/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json"
ERHUA_ENV_FILE="/home/ubuntu/.hermes/profiles/erhua/.env"

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "hermes_cron_live_parity_observation_error=python_unavailable"
  exit 1
fi

"$PYTHON_BIN" - \
  "$REGISTRY_FILE" \
  "$XIAOMAN_CRON_FILE" \
  "$XIAOMAN_ENV_FILE" \
  "$ERHUA_CRON_FILE" \
  "$ERHUA_ENV_FILE" <<'PY'
from __future__ import annotations

import json
import os
import stat
import sys
from pathlib import Path


registry_file = Path(sys.argv[1])
cron_files = {
    "xiaoman": Path(sys.argv[2]),
    "erhua": Path(sys.argv[4]),
}
env_files = {
    "xiaoman": Path(sys.argv[3]),
    "erhua": Path(sys.argv[5]),
}

MAX_JSON_BYTES = 65536
MAX_ENV_BYTES = 1024 * 1024
CHAT_ID_KEY = "WECOM_HOME_CHANNEL"


def fail(reason: str) -> None:
    print(f"hermes_cron_live_parity_observation_error={reason}")
    raise SystemExit(1)


def read_regular(path: Path, max_bytes: int, reason: str) -> bytes:
    try:
        entry_stat = os.lstat(path)
    except FileNotFoundError:
        fail(reason)
    if stat.S_ISLNK(entry_stat.st_mode) or not stat.S_ISREG(entry_stat.st_mode):
        fail(reason)
    if entry_stat.st_size <= 0 or entry_stat.st_size > max_bytes:
        fail(reason)
    if stat.S_IMODE(entry_stat.st_mode) & (stat.S_IWGRP | stat.S_IWOTH):
        fail(reason)
    return path.read_bytes()


def load_json(path: Path, reason: str) -> dict:
    try:
        value = json.loads(read_regular(path, MAX_JSON_BYTES, reason).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        fail(reason)
    if not isinstance(value, dict):
        fail(reason)
    return value


def resolve_chat_id(path: Path) -> str:
    try:
        text = read_regular(path, MAX_ENV_BYTES, "profile_env_invalid").decode("utf-8")
    except UnicodeDecodeError:
        fail("profile_env_invalid")
    found: list[str] = []
    for raw in text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("export "):
            stripped = stripped[len("export ") :].lstrip()
        key, separator, value = stripped.partition("=")
        if not separator or key.strip() != CHAT_ID_KEY:
            continue
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        found.append(value)
    if len(found) != 1:
        fail("profile_env_invalid")
    chat_id = found[0]
    if not chat_id or len(chat_id) > 256:
        fail("profile_env_invalid")
    if any(ord(character) < 0x20 or ord(character) == 0x7F for character in chat_id):
        fail("profile_env_invalid")
    return chat_id


def schedule_expr(job: dict) -> str | None:
    schedule = job.get("schedule")
    if isinstance(schedule, dict):
        expr = schedule.get("expr")
        return expr if isinstance(expr, str) else None
    return schedule if isinstance(schedule, str) else None


registry = load_json(registry_file, "registry_invalid")
if registry.get("schema_version") != 1:
    fail("registry_invalid")
reviewed_jobs = registry.get("reviewed_jobs")
if not isinstance(reviewed_jobs, list):
    fail("registry_invalid")

profiles = {"xiaoman", "erhua"}
expected = [
    job
    for job in reviewed_jobs
    if isinstance(job, dict) and job.get("profile") in profiles
]
if len(expected) != 5:
    fail("registry_invalid")

chat_ids = {profile: resolve_chat_id(path) for profile, path in env_files.items()}
live_jobs: dict[str, list[dict]] = {}
live_count = 0
enabled_count = 0
for profile, cron_file in cron_files.items():
    value = load_json(cron_file, "cron_invalid")
    if value.get("schema_version") != 1:
        fail("cron_invalid")
    jobs = value.get("jobs")
    if not isinstance(jobs, list) or any(not isinstance(job, dict) for job in jobs):
        fail("cron_invalid")
    live_jobs[profile] = jobs
    live_count += len(jobs)
    enabled_count += sum(1 for job in jobs if job.get("enabled") is True)

for entry in expected:
    profile = str(entry.get("profile"))
    matches = [job for job in live_jobs[profile] if job.get("name") == entry.get("name")]
    if len(matches) != 1:
        fail("missing_or_duplicate_reviewed_job")
    job = matches[0]
    origin = job.get("origin")
    if not isinstance(origin, dict):
        fail("route_drift")
    if (
        schedule_expr(job) != entry.get("schedule_expr")
        or job.get("script") != entry.get("script")
        or bool(job.get("no_agent")) != bool(entry.get("no_agent"))
        or job.get("deliver") != entry.get("deliver")
        or origin.get("platform") != entry.get("origin_platform")
        or origin.get("chat_id") != chat_ids[profile]
        or origin.get("chat_name") is not None
        or origin.get("thread_id") is not None
    ):
        fail("reviewed_job_drift")

reviewed_names = {
    (str(entry.get("profile")), str(entry.get("name")))
    for entry in expected
}
for profile, jobs in live_jobs.items():
    for job in jobs:
        if (profile, str(job.get("name"))) not in reviewed_names:
            fail("unreviewed_live_job")

print("hermes_cron_live_parity_result=success")
print(f"hermes_cron_live_parity_reviewed_count={len(expected)}")
print(f"hermes_cron_live_parity_live_count={live_count}")
print(f"hermes_cron_live_parity_enabled_count={enabled_count}")
PY

