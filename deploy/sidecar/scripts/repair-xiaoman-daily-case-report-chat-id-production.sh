#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-xiaoman-daily-case-report-config-v1"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PROFILE_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
KEY="QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
SOURCE_KEY="WECOM_HOME_CHANNEL"

fail() {
  local reason="$1"
  echo "qintopia_runtime_one_shot_safe_failure=xiaoman daily case report chat id repair: ${reason}" >&2
  echo "xiaoman daily case report chat id repair failed: ${reason}" >&2
  exit 1
}

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID_REPAIR:-}" != "$APPROVAL" ]]; then
  fail "approval missing"
fi

expected_release_sha="${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID_REPAIR_RELEASE_SHA:-}"
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
if [[ ! -f "$PROFILE_ENV_FILE" || -L "$PROFILE_ENV_FILE" ]]; then
  fail "profile env shape invalid"
fi

python3 - "$ENV_FILE" "$PROFILE_ENV_FILE" "$KEY" "$SOURCE_KEY" <<'PY' || fail "persistent env update failed"
from __future__ import annotations

import os
import shlex
import stat
import sys
from pathlib import Path

env_path = Path(sys.argv[1])
profile_path = Path(sys.argv[2])
key = sys.argv[3]
source_key = sys.argv[4]


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
        raise SystemExit("profile env contains invalid quoting") from exc
    if len(parts) != 1:
        raise SystemExit("profile env value is unsafe")
    return parts[0]


def source_chat_id(profile_text: str) -> str:
    values: list[str] = []
    for raw in profile_text.splitlines():
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("export "):
            stripped = stripped[len("export ") :].lstrip()
        source, separator, raw_value = stripped.partition("=")
        if not separator or source.strip() != source_key:
            continue
        value = parse_env_value(raw_value)
        if not value or len(value) > 256:
            raise SystemExit("profile chat id shape invalid")
        if any(ord(character) < 0x20 or ord(character) == 0x7F for character in value):
            raise SystemExit("profile chat id contains control characters")
        values.append(value)
    if len(values) != 1:
        raise SystemExit("profile chat id source is not singleton")
    return values[0]


env_text = validate_regular_file(env_path, "persistent env")
profile_text = validate_regular_file(profile_path, "profile env")
expected = source_chat_id(profile_text)
line = f"{key}={shlex.quote(expected)}"

matches = [
    raw
    for raw in env_text.splitlines()
    if raw.startswith(f"{key}=") or raw.startswith(f"export {key}=")
]
if len(matches) > 1:
    raise SystemExit("chat id key is duplicated")
if len(matches) == 1:
    existing = matches[0]
    if existing.startswith("export "):
        existing = existing[len("export ") :]
    _, _, raw_existing = existing.partition("=")
    if parse_env_value(raw_existing) != expected:
        raise SystemExit("chat id key value invalid")
    print("xiaoman_daily_case_report_chat_id_repair=deduped")
    raise SystemExit(0)

suffix = "" if env_text.endswith("\n") else "\n"
with env_path.open("a", encoding="utf-8") as handle:
    handle.write(f"{suffix}{line}\n")
    handle.flush()
    os.fsync(handle.fileno())

print("xiaoman_daily_case_report_chat_id_repair=applied")
PY

echo "xiaoman daily case report chat id repair completed"
