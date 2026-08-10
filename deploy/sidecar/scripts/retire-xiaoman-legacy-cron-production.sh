#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_LEGACY_CRON_RETIREMENT:-}" != "approved-production-xiaoman-legacy-cron-retirement" ]]; then
  echo "Xiaoman legacy cron retirement requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
CRON_FILE="/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json"
EXPECTED_PREVIOUS_SHA256="2a1619eeabc82bc71e0364eff829877b1fe51be06da13e287b7753f34687eed6"

fail() {
  echo "Xiaoman legacy cron retirement failed: $1" >&2
  exit 1
}

if [[ ! -x "$PYTHON_BIN" ]]; then
  fail "fixed python3 is required"
fi

"$PYTHON_BIN" - "$CRON_FILE" "$EXPECTED_PREVIOUS_SHA256" <<'PY'
from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import stat
import sys
import tempfile
from pathlib import Path


cron_file = Path(sys.argv[1])
expected_previous_sha256 = sys.argv[2]
retirement_script = "retire-xiaoman-legacy-cron-production.sh"


def fail(message: str) -> None:
    raise SystemExit(message)


def safe_chown(path: str, uid: int, gid: int) -> None:
    try:
        os.chown(path, uid, gid)
    except PermissionError:
        current = os.stat(path, follow_symlinks=False)
        if current.st_uid != uid or current.st_gid != gid:
            raise


def count_jobs(item) -> int:
    job_keys = {
        "active",
        "command",
        "cron",
        "enabled",
        "handler",
        "interval",
        "message",
        "prompt",
        "schedule",
        "target",
        "tool",
    }
    if isinstance(item, list):
        return sum(count_jobs(child) for child in item)
    if isinstance(item, dict):
        keys = {str(key).lower() for key in item}
        own = 1 if keys & job_keys else 0
        return own + sum(count_jobs(child) for child in item.values())
    return 0


if not cron_file.is_absolute():
    fail("legacy cron file path must be absolute")
if not cron_file.exists():
    fail("legacy cron file is required")

parent_stat = os.lstat(cron_file.parent)
if stat.S_ISLNK(parent_stat.st_mode) or not stat.S_ISDIR(parent_stat.st_mode):
    fail("legacy cron parent must be a regular directory")

entry_stat = os.lstat(cron_file)
if stat.S_ISLNK(entry_stat.st_mode) or not stat.S_ISREG(entry_stat.st_mode):
    fail("legacy cron file must be a regular file")
if entry_stat.st_size > 65536:
    fail("legacy cron file is too large")

previous_mode = stat.S_IMODE(entry_stat.st_mode)
if previous_mode & 0o002:
    fail("legacy cron file must not be world writable")

payload = cron_file.read_bytes()
previous_sha256 = hashlib.sha256(payload).hexdigest()

try:
    value = json.loads(payload.decode("utf-8"))
except (UnicodeDecodeError, json.JSONDecodeError) as exc:
    raise SystemExit("legacy cron file must be JSON") from exc

previous_decl_count = count_jobs(value)
if previous_sha256 != expected_previous_sha256:
    fail(
        "legacy cron file sha256 does not match the reviewed production observation "
        f"(actual_sha256={previous_sha256}, expected_sha256={expected_previous_sha256}, "
        f"current_decl_count={previous_decl_count}, external_calls_executed=false, "
        "safe_for_chat=false)"
    )
if previous_decl_count == 0:
    fail("legacy cron file has no job declarations to retire")

retired_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()
if retired_at.endswith("+00:00"):
    retired_at = f"{retired_at[:-6]}Z"

replacement = {
    "schema_version": 1,
    "retired_by": retirement_script,
    "retired_at": retired_at,
    "previous_sha256": previous_sha256,
    "previous_decl_count": previous_decl_count,
    "previous_mode": f"{previous_mode:04o}",
    "jobs": [],
}
replacement_bytes = (
    json.dumps(replacement, ensure_ascii=True, indent=2, sort_keys=True) + "\n"
).encode("utf-8")
new_sha256 = hashlib.sha256(replacement_bytes).hexdigest()

backup_path = cron_file.with_name(
    f"jobs.json.retired-{retired_at.replace(':', '').replace('-', '')}-{previous_sha256[:12]}.bak"
)
backup_fd = os.open(
    backup_path,
    os.O_CREAT | os.O_EXCL | os.O_WRONLY,
    0o600,
)
try:
    with os.fdopen(backup_fd, "wb") as backup:
        backup.write(payload)
        backup.flush()
        os.fsync(backup.fileno())
    safe_chown(str(backup_path), entry_stat.st_uid, entry_stat.st_gid)
    os.chmod(backup_path, 0o600)
except Exception:
    try:
        os.unlink(backup_path)
    except FileNotFoundError:
        pass
    raise

fd, temp_name = tempfile.mkstemp(
    prefix=f".{cron_file.name}.",
    dir=str(cron_file.parent),
)
try:
    with os.fdopen(fd, "wb") as temp:
        temp.write(replacement_bytes)
        temp.flush()
        os.fsync(temp.fileno())
    safe_chown(temp_name, entry_stat.st_uid, entry_stat.st_gid)
    os.chmod(temp_name, 0o600)
    os.replace(temp_name, cron_file)
except Exception:
    try:
        os.unlink(temp_name)
    except FileNotFoundError:
        pass
    raise

print(
    json.dumps(
        {
            "schema_version": 1,
            "status": "legacy_cron_retired",
            "profile": "xiaoman",
            "previous_sha256": previous_sha256,
            "new_sha256": new_sha256,
            "previous_decl_count": previous_decl_count,
            "new_decl_count": 0,
            "previous_mode": f"{previous_mode:04o}",
            "new_mode": "0600",
            "backup_created": True,
            "live_profile_modified": True,
            "external_calls_executed": False,
            "safe_for_chat": False,
        },
        separators=(",", ":"),
    )
)
PY

echo "Xiaoman legacy cron retirement passed"
