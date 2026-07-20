#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Xiaoman legacy cron observation skipped: set QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 to inspect runtime cron state" >&2
  exit 0
fi

DEFAULT_PROFILE_DIR="/home/ubuntu/.hermes/profiles/xiaoman"
DEFAULT_CRON_FILE="/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json"
PROFILE_DIR="${QINTOPIA_XIAOMAN_PROFILE_DIR:-$DEFAULT_PROFILE_DIR}"
CRON_FILE="${QINTOPIA_XIAOMAN_LEGACY_CRON_FILE:-$DEFAULT_CRON_FILE}"
TEST_MODE="${QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_TEST_MODE:-0}"
TEST_ROOT="${QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_TEST_ROOT:-}"

if [[ "$TEST_MODE" != "1" ]]; then
  if [[ "$PROFILE_DIR" != "$DEFAULT_PROFILE_DIR" || "$CRON_FILE" != "$DEFAULT_CRON_FILE" ]]; then
    echo "Xiaoman legacy cron observation requires the fixed production Xiaoman profile path" >&2
    exit 1
  fi
else
  if [[ "$TEST_ROOT" != /tmp/* && "$TEST_ROOT" != /private/tmp/* ]]; then
    echo "Xiaoman legacy cron observation test mode requires a /tmp test root" >&2
    exit 1
  fi
  for test_path in "$PROFILE_DIR" "$CRON_FILE"; do
    case "$test_path" in
      "$TEST_ROOT"/*) ;;
      *)
        echo "Xiaoman legacy cron observation test paths must stay under the test root" >&2
        exit 1
        ;;
    esac
  done
fi

python3 - "$PROFILE_DIR" "$CRON_FILE" <<'PY'
import hashlib
import json
import os
import stat
import sys

profile_dir, cron_file = sys.argv[1:3]

def fail(message: str) -> None:
    raise SystemExit(message)

if not os.path.isabs(profile_dir) or not os.path.isabs(cron_file):
    fail("Xiaoman legacy cron observation requires absolute paths")

if os.path.realpath(cron_file) != os.path.join(os.path.realpath(profile_dir), "cron", "jobs.json"):
    fail("Xiaoman legacy cron file must stay under the Xiaoman profile cron directory")

if not os.path.exists(cron_file):
    print(json.dumps({
        "schema_version": 1,
        "status": "no_legacy_cron_jobs",
        "profile": "xiaoman",
        "cron_file_present": False,
        "cron_decl_count": 0,
        "cron_file_sha256": None,
        "live_profile_modified": False,
        "external_calls_executed": False,
        "safe_for_chat": False,
    }, separators=(",", ":")))
    raise SystemExit(0)

entry_stat = os.lstat(cron_file)
if stat.S_ISLNK(entry_stat.st_mode) or not stat.S_ISREG(entry_stat.st_mode):
    fail("Xiaoman legacy cron file must be a regular file")
if entry_stat.st_size > 65536:
    fail("Xiaoman legacy cron file is too large for observation")
if entry_stat.st_mode & 0o022:
    fail("Xiaoman legacy cron file must not be group/world writable")

with open(cron_file, "rb") as handle:
    payload = handle.read()
cron_hash = hashlib.sha256(payload).hexdigest()

try:
    value = json.loads(payload.decode("utf-8"))
except (UnicodeDecodeError, json.JSONDecodeError) as exc:
    raise SystemExit("Xiaoman legacy cron file must be JSON") from exc

JOB_KEYS = {
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

def looks_like_job(item) -> bool:
    if not isinstance(item, dict):
        return False
    keys = {str(key).lower() for key in item}
    return bool(keys & JOB_KEYS)

def count_jobs(item) -> int:
    if isinstance(item, list):
        return sum(count_jobs(child) for child in item)
    if isinstance(item, dict):
        own = 1 if looks_like_job(item) else 0
        return own + sum(count_jobs(child) for child in item.values())
    return 0

cron_decl_count = count_jobs(value)
if cron_decl_count != 0:
    fail("Xiaoman legacy cron observation found runtime cron job declarations")

print(json.dumps({
    "schema_version": 1,
    "status": "no_legacy_cron_jobs",
    "profile": "xiaoman",
    "cron_file_present": True,
    "cron_decl_count": 0,
    "cron_file_sha256": cron_hash,
    "live_profile_modified": False,
    "external_calls_executed": False,
    "safe_for_chat": False,
}, separators=(",", ":")))
PY

echo "Xiaoman legacy cron observation passed"
