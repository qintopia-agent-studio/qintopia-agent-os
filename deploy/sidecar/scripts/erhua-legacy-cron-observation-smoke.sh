#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Erhua legacy cron observation skipped: set QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE=1 to inspect runtime cron state" >&2
  exit 0
fi

DEFAULT_PROFILE_DIR="/home/ubuntu/.hermes/profiles/erhua"
DEFAULT_CRON_FILE="/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json"
DEFAULT_REGISTRY_FILE="/home/ubuntu/qintopia-agent-os-releases/current/runtime/hermes/cron/reviewed-cron-jobs.json"
PROFILE_DIR="${QINTOPIA_ERHUA_PROFILE_DIR:-$DEFAULT_PROFILE_DIR}"
CRON_FILE="${QINTOPIA_ERHUA_LEGACY_CRON_FILE:-$DEFAULT_CRON_FILE}"
REGISTRY_FILE="${QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_REGISTRY:-$DEFAULT_REGISTRY_FILE}"
TEST_MODE="${QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_TEST_MODE:-0}"
TEST_ROOT="${QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_TEST_ROOT:-}"
PYTHON_BIN="/usr/bin/python3"

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "Erhua legacy cron observation requires fixed python3" >&2
  exit 1
fi

if [[ "$TEST_MODE" != "1" ]]; then
  if [[ "$PROFILE_DIR" != "$DEFAULT_PROFILE_DIR" || "$CRON_FILE" != "$DEFAULT_CRON_FILE" || "$REGISTRY_FILE" != "$DEFAULT_REGISTRY_FILE" ]]; then
    echo "Erhua legacy cron observation requires the fixed production Erhua profile path" >&2
    exit 1
  fi
else
  if [[ "$TEST_ROOT" != /tmp/* && "$TEST_ROOT" != /private/tmp/* ]]; then
    echo "Erhua legacy cron observation test mode requires a /tmp test root" >&2
    exit 1
  fi
  for test_path in "$PROFILE_DIR" "$CRON_FILE" "$REGISTRY_FILE"; do
    case "$test_path" in
      "$TEST_ROOT"/*) ;;
      *)
        echo "Erhua legacy cron observation test paths must stay under the test root" >&2
        exit 1
        ;;
    esac
  done
fi

"$PYTHON_BIN" - "$PROFILE_DIR" "$CRON_FILE" "$REGISTRY_FILE" <<'PY'
import hashlib
import json
import os
import stat
import sys

profile_dir, cron_file, registry_file = sys.argv[1:4]


def fail(message: str) -> None:
    raise SystemExit(message)


if not os.path.isabs(profile_dir) or not os.path.isabs(cron_file):
    fail("Erhua legacy cron observation requires absolute paths")

if os.path.realpath(cron_file) != os.path.join(os.path.realpath(profile_dir), "cron", "jobs.json"):
    fail("Erhua legacy cron file must stay under the Erhua profile cron directory")

try:
    with open(registry_file, "rb") as handle:
        registry = json.loads(handle.read().decode("utf-8"))
except FileNotFoundError:
    fail("Erhua reviewed cron registry is missing from release/current")
except (UnicodeDecodeError, json.JSONDecodeError) as exc:
    raise SystemExit("Erhua reviewed cron registry must be JSON") from exc

if not isinstance(registry, dict) or registry.get("schema_version") != 1:
    fail("Erhua reviewed cron registry has an unsupported schema")
reviewed_jobs = registry.get("reviewed_jobs")
if not isinstance(reviewed_jobs, list):
    fail("Erhua reviewed cron registry must list reviewed_jobs")


def schedule_expr(job):
    schedule = job.get("schedule")
    if isinstance(schedule, dict):
        expr = schedule.get("expr")
        return expr if isinstance(expr, str) else None
    return schedule if isinstance(schedule, str) else None


def is_reviewed(job) -> bool:
    for entry in reviewed_jobs:
        if not isinstance(entry, dict) or entry.get("profile") != "erhua":
            continue
        if (
            entry.get("name") == job.get("name")
            and entry.get("schedule_expr") == schedule_expr(job)
            and entry.get("script") == job.get("script")
            and bool(entry.get("no_agent")) == bool(job.get("no_agent"))
        ):
            return True
    return False


if not os.path.exists(cron_file):
    print(json.dumps({
        "schema_version": 1,
        "status": "reviewed_declarations_only",
        "profile": "erhua",
        "cron_file_present": False,
        "cron_decl_count": 0,
        "reviewed_decl_count": 0,
        "cron_file_sha256": None,
        "live_profile_modified": False,
        "external_calls_executed": False,
        "safe_for_chat": False,
    }, separators=(",", ":")))
    raise SystemExit(0)

entry_stat = os.lstat(cron_file)
if stat.S_ISLNK(entry_stat.st_mode) or not stat.S_ISREG(entry_stat.st_mode):
    fail("Erhua legacy cron file must be a regular file")
if entry_stat.st_size > 65536:
    fail("Erhua legacy cron file is too large for observation")
if entry_stat.st_mode & 0o022:
    fail("Erhua legacy cron file must not be group/world writable")

with open(cron_file, "rb") as handle:
    payload = handle.read()
cron_hash = hashlib.sha256(payload).hexdigest()

try:
    value = json.loads(payload.decode("utf-8"))
except (UnicodeDecodeError, json.JSONDecodeError) as exc:
    raise SystemExit("Erhua legacy cron file must be JSON") from exc

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


def collect_jobs(item, sink) -> None:
    if isinstance(item, list):
        for child in item:
            collect_jobs(child, sink)
        return
    if not isinstance(item, dict):
        return
    keys = {str(key).lower() for key in item}
    if keys & JOB_KEYS:
        sink.append(item)
    for child in item.values():
        collect_jobs(child, sink)


declarations = []
collect_jobs(value, declarations)
cron_decl_count = len(declarations)

unreviewed = [job for job in declarations if not is_reviewed(job)]
if unreviewed:
    offenders = [
        {
            "name": job.get("name"),
            "schedule_expr": schedule_expr(job),
            "script": job.get("script"),
            "no_agent": bool(job.get("no_agent")),
        }
        for job in unreviewed
    ]
    fail(
        "Erhua legacy cron observation found unreviewed cron job declarations "
        f"(offenders={json.dumps(offenders, ensure_ascii=False)}, "
        f"cron_file_sha256={cron_hash}, external_calls_executed=false, "
        "safe_for_chat=false)"
    )

print(json.dumps({
    "schema_version": 1,
    "status": "reviewed_declarations_only",
    "profile": "erhua",
    "cron_file_present": True,
    "cron_decl_count": cron_decl_count,
    "reviewed_decl_count": cron_decl_count,
    "cron_file_sha256": cron_hash,
    "live_profile_modified": False,
    "external_calls_executed": False,
    "safe_for_chat": False,
}, separators=(",", ":")))
PY

echo "Erhua legacy cron observation passed"
