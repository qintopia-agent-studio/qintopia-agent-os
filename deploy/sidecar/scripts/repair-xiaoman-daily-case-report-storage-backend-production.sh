#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-xiaoman-daily-case-report-config-v1"
ENV_FILE="/etc/qintopia/message-sidecar.env"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
KEY="QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND"
DESIRED_VALUE="feishu-base"

fail() {
  local reason="$1"
  echo "qintopia_runtime_one_shot_safe_failure=xiaoman daily case report storage backend repair: ${reason}" >&2
  echo "xiaoman daily case report storage backend repair failed: ${reason}" >&2
  exit 1
}

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND_REPAIR:-}" != "$APPROVAL" ]]; then
  fail "approval missing"
fi

expected_release_sha="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND_REPAIR_RELEASE_SHA:-}"
if [[ ! "$expected_release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  fail "release sha invalid"
fi

if [[ ! -L "$RELEASE_CURRENT" ]]; then
  fail "release current missing"
fi

release_target="$(readlink -f "$RELEASE_CURRENT")"
if [[ "${release_target##*/}" != "$expected_release_sha" ]]; then
  fail "release sha drift"
fi

if [[ ! -f "$ENV_FILE" || -L "$ENV_FILE" ]]; then
  fail "persistent env shape invalid"
fi

# The desired storage backend is a fixed, allowlisted literal. It must be one
# of the values the auto-publish worker accepts; anything else is a bug in the
# shipped one-shot and must not be written to production.
case "$DESIRED_VALUE" in
  feishu-base|https-public) ;;
  *) fail "desired storage backend value unsupported" ;;
esac

python3 - "$ENV_FILE" "$KEY" "$DESIRED_VALUE" <<'PY' || fail "persistent env update failed"
from __future__ import annotations

import os
import shlex
import stat
import sys
from pathlib import Path

env_path = Path(sys.argv[1])
key = sys.argv[2]
desired = sys.argv[3]


def validate_regular_file(path: Path, label: str) -> str:
    metadata = path.lstat()
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise SystemExit(f"{label} is not a regular file")
    if metadata.st_nlink != 1:
        raise SystemExit(f"{label} has unsafe links")
    if stat.S_IMODE(metadata.st_mode) & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(f"{label} is group/world writable")
    if metadata.st_size <= 0 or metadata.st_size > 1024 * 1024:
        raise SystemExit(f"{label} size invalid")
    return path.read_text(encoding="utf-8")


def parse_env_value(raw_value: str) -> str:
    lexer = shlex.shlex(raw_value.strip(), posix=True)
    lexer.whitespace_split = True
    lexer.commenters = ""
    try:
        parts = list(lexer)
    except ValueError as exc:
        raise SystemExit("persistent env contains invalid quoting") from exc
    if len(parts) != 1:
        raise SystemExit("persistent env value is unsafe")
    return parts[0]


env_text = validate_regular_file(env_path, "persistent env")

matches = [
    raw
    for raw in env_text.splitlines()
    if raw.startswith(f"{key}=") or raw.startswith(f"export {key}=")
]
if len(matches) > 1:
    raise SystemExit("storage backend key is duplicated")
if len(matches) == 1:
    existing = matches[0]
    if existing.startswith("export "):
        existing = existing[len("export ") :]
    _, _, raw_existing = existing.partition("=")
    if parse_env_value(raw_existing) != desired:
        raise SystemExit("storage backend key value invalid")
    print("xiaoman_daily_case_report_storage_backend_repair=deduped")
    raise SystemExit(0)

line = f"{key}={shlex.quote(desired)}"
suffix = "" if env_text.endswith("\n") else "\n"
with env_path.open("a", encoding="utf-8") as handle:
    handle.write(f"{suffix}{line}\n")
    handle.flush()
    os.fsync(handle.fileno())

print("xiaoman_daily_case_report_storage_backend_repair=applied")
PY

echo "xiaoman daily case report storage backend repair completed"
