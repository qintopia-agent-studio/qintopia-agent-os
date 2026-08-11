#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON:-}" != "approved-production-erhua-morning-brief-hermes-cron" ]]; then
  echo "Erhua morning brief Hermes cron apply requires explicit owner approval" >&2
  exit 1
fi

if [[ "${1:-}" != "--install" && "${1:-}" != "--enable" ]]; then
  echo "usage: apply-erhua-morning-brief-hermes-cron.sh --install|--enable" >&2
  exit 2
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
WRAPPER_SOURCE="${RELEASE_DIR}/runtime/hermes/scripts/qintopia_erhua_morning_brief.sh"
WRAPPER_DEST="/home/ubuntu/.hermes/scripts/qintopia_erhua_morning_brief.sh"
CRON_FILE="/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json"
PROFILE_ENV="/home/ubuntu/.hermes/profiles/erhua/.env"
SNAPSHOT_SYNC="${RELEASE_DIR}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"
WRAPPER_MODE="${1}"

fail() {
  echo "qintopia_hermes_cron_apply_safe_failure=$1" >&2
  echo "Erhua morning brief Hermes cron apply failed: $1" >&2
  exit 1
}

[[ -x "$PYTHON_BIN" ]] || fail "fixed python3 is required"
[[ "$(readlink -f "$RELEASE_DIR")" != "$RELEASE_DIR" ]] || fail "release/current must be a symlink"
[[ -f "$WRAPPER_SOURCE" ]] || fail "reviewed wrapper is missing from release/current"
[[ -f "$CRON_FILE" ]] || fail "Erhua cron jobs.json is missing"
[[ -f "$PROFILE_ENV" ]] || fail "Erhua profile env is missing"
[[ -x "$SNAPSHOT_SYNC" ]] || fail "snapshot sync script is missing from release/current"

umask 077
mkdir -p "$(dirname "$WRAPPER_DEST")"
cp "$WRAPPER_SOURCE" "$WRAPPER_DEST"
chmod 0700 "$WRAPPER_DEST"

"$PYTHON_BIN" - "$CRON_FILE" "$PROFILE_ENV" "$WRAPPER_MODE" <<'PY'
from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import re
import shlex
import stat
import sys
import tempfile
from pathlib import Path


cron_file = Path(sys.argv[1])
profile_env = Path(sys.argv[2])
mode = sys.argv[3]
job_name = "二花·每日早报"
job_schedule = {"kind": "cron", "expr": "10 8 * * *", "display": "10 8 * * *"}
job_script = "qintopia_erhua_morning_brief.sh"


def fail(message: str) -> None:
    raise SystemExit(f"qintopia_hermes_cron_apply_safe_failure={message}")


def safe_chown(path: str, uid: int, gid: int) -> None:
    try:
        os.chown(path, uid, gid)
    except PermissionError:
        current = os.stat(path, follow_symlinks=False)
        if current.st_uid != uid or current.st_gid != gid:
            raise


def parse_chat_id(path: Path) -> str:
    env_stat = os.lstat(path)
    if stat.S_ISLNK(env_stat.st_mode) or not stat.S_ISREG(env_stat.st_mode):
        fail("Erhua profile env must be a regular non-symlink file")
    mode_bits = stat.S_IMODE(env_stat.st_mode)
    if mode_bits & (stat.S_IWGRP | stat.S_IWOTH):
        fail("Erhua profile env must not be group/world writable")
    if env_stat.st_size <= 0 or env_stat.st_size > 1024 * 1024:
        fail("Erhua profile env size is invalid")
    text = path.read_text(encoding="utf-8")
    assignment = re.compile(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$")
    values = {}
    for raw in text.splitlines():
        match = assignment.match(raw)
        if not match:
            continue
        key, raw_value = match.groups()
        if key != "WECOM_HOME_CHANNEL":
            continue
        if key in values:
            fail("Erhua profile env contains duplicate WECOM_HOME_CHANNEL")
        lexer = shlex.shlex(raw_value, posix=True)
        lexer.whitespace_split = True
        lexer.commenters = ""
        try:
            parts = list(lexer)
        except ValueError as exc:
            fail("Erhua profile env contains unsafe WECOM_HOME_CHANNEL quoting")
        if len(parts) != 1:
            fail("Erhua profile env contains unsafe WECOM_HOME_CHANNEL")
        value = parts[0]
        if not re.fullmatch(r"[A-Za-z0-9_@.\-]{1,128}", value):
            fail("Erhua profile env WECOM_HOME_CHANNEL is not a supported chat id")
        values[key] = value
    if "WECOM_HOME_CHANNEL" not in values:
        fail("Erhua profile env is missing WECOM_HOME_CHANNEL")
    return values["WECOM_HOME_CHANNEL"]


chat_id = parse_chat_id(profile_env)

parent_stat = os.lstat(cron_file.parent)
if stat.S_ISLNK(parent_stat.st_mode) or not stat.S_ISDIR(parent_stat.st_mode):
    fail("Erhua cron parent must be a regular directory")
entry_stat = os.lstat(cron_file)
if stat.S_ISLNK(entry_stat.st_mode) or not stat.S_ISREG(entry_stat.st_mode):
    fail("Erhua cron file must be a regular file")
if entry_stat.st_size > 65536:
    fail("Erhua cron file is too large")
previous_mode = stat.S_IMODE(entry_stat.st_mode)
if previous_mode & 0o002:
    fail("Erhua cron file must not be world writable")

payload = cron_file.read_bytes()
try:
    value = json.loads(payload.decode("utf-8"))
except (UnicodeDecodeError, json.JSONDecodeError) as exc:
    fail("Erhua cron jobs.json must be JSON")

if not isinstance(value, dict) or not isinstance(value.get("jobs"), list):
    fail("Erhua cron jobs.json must be an envelope with a jobs list")
jobs = value["jobs"]


def job_def_matches(job) -> bool:
    return (
        job.get("name") == job_name
        and job.get("schedule") == job_schedule
        and job.get("script") == job_script
        and job.get("no_agent") is True
        and job.get("deliver") == "origin"
        and isinstance(job.get("origin"), dict)
        and job["origin"].get("platform") == "wecom"
        and job["origin"].get("chat_id") == chat_id
        and job["origin"].get("chat_name") is None
        and job["origin"].get("thread_id") is None
    )


existing = [job for job in jobs if isinstance(job, dict) and job.get("name") == job_name]
if len(existing) > 1:
    fail("Erhua cron jobs.json contains duplicate morning brief jobs")

changed = False
if mode == "--install":
    if existing:
        if not job_def_matches(existing[0]):
            fail("existing morning brief job definition drifts from the reviewed declaration")
    else:
        jobs.append(
            {
                "id": os.urandom(6).hex(),
                "name": job_name,
                "schedule": job_schedule,
                "no_agent": True,
                "script": job_script,
                "deliver": "origin",
                "origin": {
                    "platform": "wecom",
                    "chat_id": chat_id,
                    "chat_name": None,
                    "thread_id": None,
                },
                "enabled": False,
                "skills": [],
            }
        )
        changed = True
else:
    if not existing:
        fail("cannot enable morning brief job before it is installed")
    if not job_def_matches(existing[0]):
        fail("existing morning brief job definition drifts from the reviewed declaration")
    if existing[0].get("enabled") is not True:
        existing[0]["enabled"] = True
        changed = True

applied_job = existing[0] if existing else jobs[-1]
backup_created = False
new_sha256 = hashlib.sha256(payload).hexdigest()
if changed:
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()
    if now.endswith("+00:00"):
        now = f"{now[:-6]}Z"
    backup_path = cron_file.with_name(
        f"jobs.json.pre-{now.replace(':','').replace('-','')}-{hashlib.sha256(payload).hexdigest()[:12]}.bak"
    )
    backup_fd = os.open(backup_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
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
    backup_created = True

    replacement_bytes = (
        json.dumps(value, ensure_ascii=True, indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")
    new_sha256 = hashlib.sha256(replacement_bytes).hexdigest()
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
            "status": "erhua_morning_brief_hermes_cron_applied",
            "mode": mode,
            "profile": "erhua",
            "job_name": job_name,
            "job_id": applied_job.get("id", ""),
            "origin_chat_id_sha256": hashlib.sha256(chat_id.encode("utf-8")).hexdigest(),
            "enabled": applied_job.get("enabled") is True,
            "cron_decl_count": len(jobs),
            "updated_at_preserved": isinstance(value.get("updated_at"), str),
            "new_sha256": new_sha256,
            "backup_created": backup_created,
            "live_profile_modified": changed,
            "external_calls_executed": False,
            "safe_for_chat": False,
        },
        separators=(",", ":"),
        ensure_ascii=False,
    )
)
PY

if ! QINTOPIA_HERMES_CRON_SNAPSHOT="approved-production-hermes-cron-snapshot" \
  "$SNAPSHOT_SYNC" >/dev/null; then
  fail "snapshot sync failed"
fi

echo "Erhua morning brief Hermes cron apply passed"
