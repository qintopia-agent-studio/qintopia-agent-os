#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON:-}" != "approved-production-xiaoman-weekly-recruitment-hermes-cron" ]]; then
  echo "xiaoman weekly recruitment Hermes cron apply requires explicit owner approval" >&2
  exit 1
fi

MODE="${1:---install}"
if [[ "$MODE" != "--install" && "$MODE" != "--enable" ]]; then
  echo "usage: apply-xiaoman-weekly-recruitment-hermes-cron.sh [--install|--enable]" >&2
  exit 2
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
CRON_FILE="/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json"
PROFILE_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"
HERMES_SCRIPTS_DIR="/home/ubuntu/.hermes/scripts"
WRAPPER_SOURCE="${RELEASE_CURRENT}/runtime/hermes/scripts/qintopia_xiaoman_weekly_recruitment.sh"
WRAPPER_TARGET="${HERMES_SCRIPTS_DIR}/qintopia_xiaoman_weekly_recruitment.sh"
SNAPSHOT_SYNC="${RELEASE_CURRENT}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "qintopia_hermes_cron_apply_safe_failure=xiaoman weekly recruitment Hermes cron apply requires fixed python3" >&2
  echo "xiaoman weekly recruitment Hermes cron apply requires fixed python3" >&2
  exit 1
fi

"$PYTHON_BIN" - \
  "$CRON_FILE" \
  "$PROFILE_ENV_FILE" \
  "$WRAPPER_SOURCE" \
  "$WRAPPER_TARGET" \
  "$HERMES_SCRIPTS_DIR" \
  "$MODE" <<'PY'
from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import stat
import sys
import tempfile
import uuid
from pathlib import Path


cron_file = Path(sys.argv[1])
profile_env_file = Path(sys.argv[2])
wrapper_source = Path(sys.argv[3])
wrapper_target = Path(sys.argv[4])
hermes_scripts_dir = Path(sys.argv[5])
mode = sys.argv[6]

JOB_NAME = "小满·周六活动招募"
JOB_SCHEDULE_EXPR = "0 10 * * 6"
JOB_SCRIPT = "qintopia_xiaoman_weekly_recruitment.sh"
CHAT_ID_KEY = "WECOM_HOME_CHANNEL"
MAX_CRON_BYTES = 1024 * 1024
MAX_ENV_BYTES = 1024 * 1024
MAX_WRAPPER_BYTES = 65536


def fail(message: str) -> None:
    raise SystemExit(
        f"qintopia_hermes_cron_apply_safe_failure=xiaoman weekly recruitment Hermes cron apply failed: {message}"
    )


def safe_chown(path: str, uid: int, gid: int) -> None:
    try:
        os.chown(path, uid, gid)
    except PermissionError:
        current = os.stat(path, follow_symlinks=False)
        if current.st_uid != uid or current.st_gid != gid:
            raise


def read_regular_file(path: Path, maximum_bytes: int, label: str) -> tuple[bytes, os.stat_result]:
    if not path.is_absolute():
        fail(f"{label} path must be absolute")
    try:
        entry_stat = os.lstat(path)
    except FileNotFoundError:
        fail(f"{label} is required")
    if stat.S_ISLNK(entry_stat.st_mode) or not stat.S_ISREG(entry_stat.st_mode):
        fail(f"{label} must be a regular non-symlink file")
    if entry_stat.st_nlink != 1:
        fail(f"{label} hard links are forbidden")
    if entry_stat.st_size <= 0 or entry_stat.st_size > maximum_bytes:
        fail(f"{label} size is invalid")
    if stat.S_IMODE(entry_stat.st_mode) & (stat.S_IWGRP | stat.S_IWOTH):
        fail(f"{label} must not be group or world writable")
    return path.read_bytes(), entry_stat


def atomic_replace(path: Path, payload: bytes, uid: int, gid: int, file_mode: int) -> None:
    fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        safe_chown(temp_name, uid, gid)
        os.chmod(temp_name, file_mode)
        os.replace(temp_name, path)
    except Exception:
        try:
            os.unlink(temp_name)
        except FileNotFoundError:
            pass
        raise
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def resolve_chat_id() -> str:
    payload, _ = read_regular_file(profile_env_file, MAX_ENV_BYTES, "Xiaoman profile env")
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError:
        fail("Xiaoman profile env must be UTF-8")
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
        fail(f"Xiaoman profile env must define exactly one {CHAT_ID_KEY}")
    chat_id = found[0]
    if not chat_id or len(chat_id) > 256:
        fail(f"Xiaoman profile env {CHAT_ID_KEY} is empty or too long")
    for character in chat_id:
        if ord(character) < 0x20 or ord(character) == 0x7F:
            fail(f"Xiaoman profile env {CHAT_ID_KEY} contains control characters")
    return chat_id


def install_wrapper() -> bool:
    source_payload, _ = read_regular_file(
        wrapper_source, MAX_WRAPPER_BYTES, "release-local wrapper source"
    )
    if not wrapper_target.is_absolute():
        fail("Hermes wrapper target path must be absolute")
    try:
        scripts_real = hermes_scripts_dir.resolve(strict=True)
        target_real = wrapper_target.parent.resolve(strict=True)
    except FileNotFoundError:
        fail("Hermes scripts directory is required")
    if target_real != scripts_real:
        fail("Hermes wrapper target must stay inside the Hermes scripts directory")
    scripts_stat = os.lstat(hermes_scripts_dir)
    if stat.S_ISLNK(scripts_stat.st_mode) or not stat.S_ISDIR(scripts_stat.st_mode):
        fail("Hermes scripts directory must be a regular directory")

    if wrapper_target.exists():
        current_payload, target_stat = read_regular_file(
            wrapper_target, MAX_WRAPPER_BYTES, "installed Hermes wrapper"
        )
        uid, gid = target_stat.st_uid, target_stat.st_gid
        if current_payload == source_payload and stat.S_IMODE(target_stat.st_mode) == 0o700:
            return False
    else:
        uid, gid = scripts_stat.st_uid, scripts_stat.st_gid
    atomic_replace(wrapper_target, source_payload, uid, gid, 0o700)
    return True


def verify_installed_wrapper() -> None:
    source_payload, _ = read_regular_file(
        wrapper_source, MAX_WRAPPER_BYTES, "release-local wrapper source"
    )
    target_payload, target_stat = read_regular_file(
        wrapper_target, MAX_WRAPPER_BYTES, "installed Hermes wrapper"
    )
    if target_payload != source_payload:
        fail("installed Hermes wrapper does not match the release-local wrapper source")
    if stat.S_IMODE(target_stat.st_mode) != 0o700:
        fail("installed Hermes wrapper mode must be 0700")


def load_cron() -> tuple[dict, bytes, os.stat_result]:
    payload, entry_stat = read_regular_file(cron_file, MAX_CRON_BYTES, "Hermes cron file")
    parent_stat = os.lstat(cron_file.parent)
    if stat.S_ISLNK(parent_stat.st_mode) or not stat.S_ISDIR(parent_stat.st_mode):
        fail("Hermes cron parent must be a regular directory")
    try:
        document = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        fail("Hermes cron file must be JSON")
    if not isinstance(document, dict):
        fail("Hermes cron file must contain a JSON object envelope")
    if document.get("schema_version") != 1:
        fail("Hermes cron file schema_version must be 1")
    jobs = document.get("jobs")
    if not isinstance(jobs, list):
        fail("Hermes cron file must contain a jobs array")
    for job in jobs:
        if not isinstance(job, dict):
            fail("Hermes cron file contains a malformed job entry")
    return document, payload, entry_stat


def next_job_id(jobs: list[dict]) -> str:
    taken = {str(job.get("id")) for job in jobs}
    for _ in range(64):
        candidate = uuid.uuid4().hex[:12]
        if candidate not in taken:
            return candidate
    fail("could not allocate an unused job id")


def write_cron(document: dict, previous_payload: bytes, entry_stat: os.stat_result) -> dict:
    previous_sha256 = hashlib.sha256(previous_payload).hexdigest()
    replacement = (
        json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    ).encode("utf-8")
    if len(replacement) > MAX_CRON_BYTES:
        fail("updated Hermes cron file would exceed the reviewed size limit")

    applied_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()
    if applied_at.endswith("+00:00"):
        applied_at = f"{applied_at[:-6]}Z"
    stamp = applied_at.replace(":", "").replace("-", "")
    backup_path = cron_file.with_name(
        f"jobs.json.weekly-recruitment-{stamp}-{previous_sha256[:12]}.bak"
    )
    backup_fd = os.open(backup_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        with os.fdopen(backup_fd, "wb") as backup:
            backup.write(previous_payload)
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

    atomic_replace(cron_file, replacement, entry_stat.st_uid, entry_stat.st_gid, 0o600)
    return {
        "previous_sha256": previous_sha256,
        "new_sha256": hashlib.sha256(replacement).hexdigest(),
        "backup_created": True,
    }


def find_reviewed_job(jobs: list[dict]) -> tuple[int, dict]:
    matches = [(index, job) for index, job in enumerate(jobs) if job.get("name") == JOB_NAME]
    if len(matches) != 1:
        fail("Hermes cron file must contain exactly one reviewed weekly recruitment job")
    index, job = matches[0]
    schedule = job.get("schedule")
    if not isinstance(schedule, dict) or schedule.get("expr") != JOB_SCHEDULE_EXPR:
        fail("reviewed weekly recruitment job schedule does not match the reviewed declaration")
    if job.get("script") != JOB_SCRIPT or job.get("no_agent") is not True:
        fail("reviewed weekly recruitment job script contract does not match the declaration")
    if job.get("deliver") != "origin":
        fail("reviewed weekly recruitment job deliver mode does not match the reviewed declaration")
    origin = job.get("origin")
    if not isinstance(origin, dict) or origin.get("platform") != "wecom":
        fail("reviewed weekly recruitment job origin platform does not match the reviewed declaration")
    if origin.get("chat_name") is not None or origin.get("thread_id") is not None:
        fail("reviewed weekly recruitment job origin routing fields do not match the reviewed declaration")
    if origin.get("chat_id") != resolve_chat_id():
        fail("reviewed weekly recruitment job origin chat id drifted from the Xiaoman profile env")
    return index, job


document, previous_payload, entry_stat = load_cron()
jobs = document["jobs"]

if mode == "--install":
    if any(job.get("name") == JOB_NAME for job in jobs):
        fail("Hermes cron file already declares the weekly recruitment job")
    if any(job.get("script") == JOB_SCRIPT for job in jobs):
        fail("Hermes cron file already declares a job bound to the weekly recruitment script")
    chat_id = resolve_chat_id()
    wrapper_installed = install_wrapper()
    jobs.append(
        {
            "id": next_job_id(jobs),
            "name": JOB_NAME,
            "schedule": {
                "kind": "cron",
                "expr": JOB_SCHEDULE_EXPR,
                "display": JOB_SCHEDULE_EXPR,
            },
            "no_agent": True,
            "script": JOB_SCRIPT,
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
    evidence = write_cron(document, previous_payload, entry_stat)
    report = {
        "schema_version": 1,
        "status": "weekly_recruitment_hermes_cron_installed",
        "profile": "xiaoman",
        "job_enabled": False,
        "wrapper_installed": wrapper_installed,
        "origin_chat_id_resolved": True,
        "job_count": len(jobs),
        **evidence,
    }
else:
    verify_installed_wrapper()
    index, job = find_reviewed_job(jobs)
    if job.get("enabled") is True:
        report = {
            "schema_version": 1,
            "status": "weekly_recruitment_hermes_cron_already_enabled",
            "profile": "xiaoman",
            "job_enabled": True,
            "wrapper_installed": False,
            "origin_chat_id_resolved": True,
            "job_count": len(jobs),
            "previous_sha256": hashlib.sha256(previous_payload).hexdigest(),
            "new_sha256": hashlib.sha256(previous_payload).hexdigest(),
            "backup_created": False,
        }
    else:
        jobs[index]["enabled"] = True
        evidence = write_cron(document, previous_payload, entry_stat)
        report = {
            "schema_version": 1,
            "status": "weekly_recruitment_hermes_cron_enabled",
            "profile": "xiaoman",
            "job_enabled": True,
            "wrapper_installed": False,
            "origin_chat_id_resolved": True,
            "job_count": len(jobs),
            **evidence,
        }

report["live_profile_modified"] = report["previous_sha256"] != report["new_sha256"]
report["external_calls_executed"] = False
report["safe_for_chat"] = False
print(json.dumps(report, sort_keys=True, separators=(",", ":")))
PY

snapshot_sync_ok=true
if [[ -x "$SNAPSHOT_SYNC" ]]; then
  if ! QINTOPIA_HERMES_CRON_SNAPSHOT=approved-production-hermes-cron-snapshot \
    "$SNAPSHOT_SYNC" >/dev/null; then
    snapshot_sync_ok=false
  fi
else
  snapshot_sync_ok=false
fi

if [[ "$snapshot_sync_ok" != "true" ]]; then
  echo "xiaoman weekly recruitment Hermes cron snapshot sync did not run; run it manually" >&2
fi
echo "xiaoman_weekly_recruitment_hermes_cron_snapshot_sync_ok=${snapshot_sync_ok}"
echo "xiaoman weekly recruitment Hermes cron apply passed"
